//! Tests that verify the environment variable filtering and replacement
//! functionality in Ground Control.

use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::common::{assert_startup_aborted, start, stop};

mod common;

/// By default, all environment variables are flowed through to every
/// command.
#[test_log::test(tokio::test)]
async fn all_vars_by_default() {
    // Create some environment variables, including an override of the
    // PATH variable so that it contains a predictable value.
    std::env::set_var("PATH", "im_the_path");
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "-c", "echo $PATH $TESTVAR1 $TESTVAR2 >> {result_path}" ]
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            im_the_path one two
        "#},
        output
    );
}

/// Additional environment variables can be added using the `env` map in
/// the Ground Control config.
#[test_log::test(tokio::test)]
async fn add_more_vars() {
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [env]
        TESTVAR3 = "three"
        TESTVAR4 = "four"

        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "-c", "echo $TESTVAR1 $TESTVAR2 $TESTVAR3 $TESTVAR4 >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            one two three four
        "#},
        output
    );
}

/// Configuration-provided variables override pre-existing environment
/// variables.
#[test_log::test(tokio::test)]
async fn additional_vars_override_existing_vars() {
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [env]
        TESTVAR2 = "too"

        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "-c", "echo $TESTVAR1 $TESTVAR2 >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            one too
        "#},
        output
    );
}

/// Ground Control can expand environment variables in command lines
/// using a special template syntax.
#[test_log::test(tokio::test)]
async fn template_expansion() {
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [env]
        TESTVAR3 = "three"
        TESTVAR4 = "four"

        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "-c", "echo {{TESTVAR1}} $TESTVAR2 {{ TESTVAR3 }} $TESTVAR4 >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            one two three four
        "#},
        output
    );
}

/// `only-env` can be used to restrict the variables available to the
/// command; if provided, but empty, then only `PATH` will be allowed.
#[test_log::test(tokio::test)]
async fn only_path_by_default() {
    // Create some environment variables, including an override of the
    // PATH variable so that it contains a predictable value.
    std::env::set_var("PATH", "im_the_path");
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [env]
        TESTVAR3 = "three"

        [[processes]]
        name = "daemon"
        run = { only-env = [], command = [ "/bin/sh", "-c", "echo $PATH $TESTVAR1 $TESTVAR2 $TESTVAR3 >> {result_path}" ] }
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            im_the_path
        "#},
        output
    );
}

/// Other variables can be included *on a per-command basis.*
#[test_log::test(tokio::test)]
async fn allow_additional_vars() {
    // Create some environment variables, including an override of the
    // PATH variable so that it contains a predictable value.
    std::env::set_var("PATH", "im_the_path");
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [env]
        TESTVAR3 = "three"

        [[processes]]
        name = "daemon"
        pre = { only-env = [], command = [ "/bin/sh", "-c", "echo pre: $PATH $TESTVAR1 $TESTVAR2 $TESTVAR3 >> {result_path}" ] }
        run = { only-env = ["TESTVAR2", "TESTVAR3"], command = [ "/bin/sh", "-c", "echo run: $PATH $TESTVAR1 $TESTVAR2 $TESTVAR3 >> {result_path}" ] }
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            pre: im_the_path
            run: im_the_path two three
        "#},
        output
    );
}

/// Allowed environment variables must exist in the environment.
#[test_log::test(tokio::test)]
async fn allowed_vars_requires_variable_to_exist() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = { only-env = ["MISSINGVAR"], command = [ "/bin/sh", "-c", "echo $MISSINGVAR >> {result_path}" ] }
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, _output) = stop(gc, dir).await;

    assert_startup_aborted(
        indoc! {r#"
            `run` command failed for process "daemon"
            Unknown environment variable "MISSINGVAR"
        "#},
        result,
    );
}

/// Variables that are not explicitly allowed can still be used in
/// *Ground Control environment expansion syntax.*
#[test_log::test(tokio::test)]
async fn template_expansion_bypasses_allowed_vars() {
    // Create some environment variables, including an override of the
    // PATH variable so that it contains a predictable value.
    std::env::set_var("PATH", "im_the_path");
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [[processes]]
        name = "daemon"
        run = { only-env = [], command = [ "/bin/sh", "-c", "echo $PATH $TESTVAR1 {{TESTVAR2}} >> {result_path}" ] }
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            im_the_path two
        "#},
        output
    );
}

/// Ground Control environment expansion syntax can be mixed with normal
/// (allowed!) environment variables.
#[test_log::test(tokio::test)]
async fn template_expansion_and_allowed_vars() {
    // Create some environment variables, including an override of the
    // PATH variable so that it contains a predictable value.
    std::env::set_var("PATH", "im_the_path");
    std::env::set_var("TESTVAR1", "one");
    std::env::set_var("TESTVAR2", "two");

    let config = r##"
        [[processes]]
        name = "daemon"
        pre = { only-env = [], command = [ "/bin/sh", "-c", "echo pre: $PATH $TESTVAR1 {{TESTVAR2}} >> {result_path}" ] }
        run = { only-env = ["TESTVAR2"], command = [ "/bin/sh", "-c", "echo run: $PATH {{TESTVAR1}} $TESTVAR2 >> {result_path}" ] }
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            pre: im_the_path two
            run: im_the_path one two
        "#},
        output
    );
}

/// Template expansion fails if the environment variable cannot be
/// found.
#[test_log::test(tokio::test)]
async fn template_expansion_requires_variable_to_exist() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = { command = [ "/bin/sh", "-c", "echo {{MISSINGVAR}} >> {result_path}" ] }
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, _output) = stop(gc, dir).await;

    assert_startup_aborted(
        indoc! {r#"
            `run` command failed for process "daemon"
            Environment variable expansion failed for command "/bin/sh"
            Unknown environment variable "MISSINGVAR"
        "#},
        result,
    );
}
