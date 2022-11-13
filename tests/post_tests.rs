//! Tests that verify different aspects of the `post` scripts that run
//! as part of shutting down processes (regardless of if they are
//! one-shot or daemon processes).

use indoc::indoc;

use crate::common::{start, stop};

mod common;

/// The `post` command runs as Ground Control is shutting down
#[test_log::test(tokio::test)]
async fn post_during_shutdown() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = [ "/bin/sh", "-c", "echo b-pre >> {result_path}" ]
        run = [ "/bin/sh", "-c", "echo b >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            a-pre
            b-pre
            b
            b-post
            a-post
        "#},
        output
    );
}

/// Verifies that a failed `post` command does *not* block the shutdown
/// process.
#[test_log::test(tokio::test)]
async fn failed_post_continues_shutdown() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = [ "/bin/sh", "-c", "echo b-pre >> {result_path}" ]
        run = [ "/bin/sh", "-c", "echo b >> {result_path}" ]
        post = [ "/bin/sh", "-c", "exit 1" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            a-pre
            b-pre
            b
            a-post
        "#},
        output
    );
}

/// Verifies that a killed `post` command does *not* block the shutdown
/// process.
#[test_log::test(tokio::test)]
async fn killed_post_continues_shutdown() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = [ "/bin/sh", "-c", "echo b-pre >> {result_path}" ]
        run = [ "/bin/sh", "-c", "echo b >> {result_path}" ]
        post = [ "/bin/sh", "-c", "kill -9 $$" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            a-pre
            b-pre
            b
            a-post
        "#},
        output
    );
}

/// Verifies that a not-found `post` command does *not* block the
/// shutdown process.
#[test_log::test(tokio::test)]
async fn not_found_post_continues_shutdown() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = [ "/bin/sh", "-c", "echo b-pre >> {result_path}" ]
        run = [ "/bin/sh", "-c", "echo b >> {result_path}" ]
        post = "/user/binary/nope"
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            a-pre
            b-pre
            b
            a-post
        "#},
        output
    );
}
