//! Tests that verify the environment variable filtering and replacement
//! functionality in Ground Control.

use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::common::{assert_startup_aborted, start, stop};

mod common;

/// By default, only the PATH environment variable is made available to
/// commands.
#[test_log::test(tokio::test)]
async fn only_path_by_default() {
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
        [[processes]]
        name = "daemon"
        pre = [ "/bin/sh", "-c", "echo pre: $PATH $TESTVAR1 $TESTVAR2 >> {result_path}" ]
        run = { env-vars = ["TESTVAR2"], command = [ "/bin/sh", "-c", "echo run: $PATH $TESTVAR1 $TESTVAR2 >> {result_path}" ] }
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
            run: im_the_path two
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
        run = { env-vars = ["MISSINGVAR"], command = [ "/bin/sh", "-c", "echo $MISSINGVAR >> {result_path}" ] }
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
        run = { command = [ "/bin/sh", "-c", "echo $PATH $TESTVAR1 {{TESTVAR2}} >> {result_path}" ] }
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
        pre = [ "/bin/sh", "-c", "echo pre: $PATH $TESTVAR1 {{TESTVAR2}} >> {result_path}" ]
        run = { env-vars = ["TESTVAR2"], command = [ "/bin/sh", "-c", "echo run: $PATH {{TESTVAR1}} $TESTVAR2 >> {result_path}" ] }
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
