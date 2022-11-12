//! Tests that verify different aspects of the `pre` scripts that run as
//! part of starting daemons and, in the case of "one-shot" processes,
//! are the only thing that does run during the startup phase.

use indoc::indoc;

use crate::common::{assert_startup_aborted, start, stop};

mod common;

/// The `pre` command runs before the (daemon's) `run` command
#[test_log::test(tokio::test)]
async fn pre_before_run() {
    let config = r##"
        [[processes]]
        name = "daemon"
        pre = [ "/bin/sh", "-c", "echo pre >> {result_path}" ]
        run = [ "/bin/sh", "-c", "echo daemon >> {result_path}" ]
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            pre
            daemon
        "#},
        output
    );
}

/// Verifies that a failed `pre` execution aborts all subsequent command
/// executions *and* runs stop/post commands for anything that was
/// started.
#[test_log::test(tokio::test)]
async fn failed_pre_aborts_startup() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = [ "/bin/sh", "-c", "exit 1" ]
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]

        [[processes]]
        name = "c"
        pre = [ "/bin/sh", "-c", "echo c-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo c-post >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert_startup_aborted(
        indoc! {r#"
            `pre` command failed for process "b" (exit code 1)
        "#},
        result,
    );

    assert_eq!(
        indoc! {r#"
            a-pre
            a-post
        "#},
        output
    );
}

/// Verifies that a killed `pre` execution aborts all subsequent command
/// executions *and* runs stop/post commands for anything that was
/// started.
#[test_log::test(tokio::test)]
async fn killed_pre_aborts_startup() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = [ "/bin/sh", "-c", "kill -9 $$" ]
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]

        [[processes]]
        name = "c"
        pre = [ "/bin/sh", "-c", "echo c-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo c-post >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert_startup_aborted(
        indoc! {r#"
            `pre` command was killed for process "b"
        "#},
        result,
    );

    assert_eq!(
        indoc! {r#"
            a-pre
            a-post
        "#},
        output
    );
}

/// Verifies that a not-found `pre` command aborts all subsequent
/// command executions *and* runs stop/post commands for anything that
/// was started.
#[test_log::test(tokio::test)]
async fn not_found_pre_aborts_startup() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = "/user/binary/nope"
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]

        [[processes]]
        name = "c"
        pre = [ "/bin/sh", "-c", "echo c-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo c-post >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert_startup_aborted(
        indoc! {r#"
            `pre` command failed for process "b"
            Error starting command "/user/binary/nope"
            No such file or directory (os error 2)
        "#},
        result,
    );

    assert_eq!(
        indoc! {r#"
            a-pre
            a-post
        "#},
        output
    );
}

/// Verifies that a failed `pre` execution shuts down all
/// previously-started long-running processes.
#[test_log::test(tokio::test)]
async fn failed_pre_shuts_down_earlier_processes() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        run = [ "/bin/sh", "{test-daemon.sh}", "a-daemon", "{result_path}", "{temp_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        # Ground Control starts daemons (invokes their `run` commands)
        # as fast as it can and has no way to know if a daemon is going
        # to stay running (which is why it begins monitoring the
        # daemons *after* startup has completed); we need to serialize
        # the startup operations in the test though so that we can have
        # predictable output.
        [[processes]]
        name = "wait-daemon-start"
        pre = [ "/bin/sh", "{wait-daemon-start.sh}", "a-daemon", "{temp_path}" ]

        [[processes]]
        name = "b"
        pre = [ "/bin/sh", "-c", "exit 1" ]
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]

        [[processes]]
        name = "c"
        pre = [ "/bin/sh", "-c", "echo c-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo c-post >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert_startup_aborted(
        indoc! {r#"
            `pre` command failed for process "b" (exit code 1)
        "#},
        result,
    );

    assert_eq!(
        indoc! {r#"
            a-pre
            a-daemon:started
            a-daemon:shutdown-requested
            a-daemon:stopped
            a-post
        "#},
        output
    );
}
