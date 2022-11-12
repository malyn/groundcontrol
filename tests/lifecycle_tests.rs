//! Tests that demonstrate the over Ground Control lifecycle across all
//! process phases (pre, run, stop, post).

use indoc::indoc;
use pretty_assertions::assert_eq;

use crate::common::{spawn_daemon_waiter, start, stop};

mod common;

/// One-shot process (no `run` command), then a daemon process, both
/// which have `post` commands.
#[test_log::test(tokio::test)]
async fn oneshot_then_daemon() {
    let config = r##"
        [[processes]]
        name = "oneshot"
        pre = [ "/bin/sh", "-c", "echo oneshot-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo oneshot-post >> {result_path}" ]

        [[processes]]
        name = "daemon"
        pre = [ "/bin/sh", "-c", "echo daemon-pre >> {result_path}" ]
        run = [ "/bin/sh", "-c", "echo daemon >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo daemon-post >> {result_path}" ]
        "##;

    // Start Ground Control, which will shut down immediately because
    // the "daemon" exited immediately (but with a clean shutdown exit
    // code).
    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            oneshot-pre
            daemon-pre
            daemon
            daemon-post
            oneshot-post
        "#},
        output
    );
}

/// Multiple daemon processes.
#[test_log::test(tokio::test)]
async fn multiple_daemons_graceful_shutdown() {
    let config = r##"
        [[processes]]
        name = "daemon1"
        pre = [ "/bin/sh", "-c", "echo daemon1-pre >> {result_path}" ]
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon1", "{result_path}", "{temp_path}" ]
        post = [ "/bin/sh", "-c", "echo daemon1-post >> {result_path}" ]

        # Ground Control starts daemons (invokes their `run` commands)
        # as fast as it can and has no way to know if a daemon is going
        # to stay running (which is why it begins monitoring the
        # daemons *after* startup has completed); we need to serialize
        # the startup operations in the test though so that we can have
        # predictable output.
        [[processes]]
        name = "wait-daemon1-start"
        pre = [ "/bin/sh", "{wait-daemon-start.sh}", "daemon1", "{temp_path}" ]

        [[processes]]
        name = "daemon2"
        pre = [ "/bin/sh", "-c", "echo daemon2-pre >> {result_path}" ]
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon2", "{result_path}", "{temp_path}" ]
        post = [ "/bin/sh", "-c", "echo daemon2-post >> {result_path}" ]
        "##;

    // Start Ground Control, wait for daemon2 to finish starting, ask
    // Ground Control to shutdown, then wait for Ground Control to stop.
    let (gc, tx, dir) = start(config).await;

    let daemon_waiter = spawn_daemon_waiter(&dir, "daemon2");
    tokio::task::spawn(async move {
        daemon_waiter.await.unwrap();
        tx.send(()).unwrap();
    });

    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            daemon1-pre
            daemon1:started
            daemon2-pre
            daemon2:started
            daemon2:shutdown-requested
            daemon2:stopped
            daemon2-post
            daemon1:shutdown-requested
            daemon1:stopped
            daemon1-post
        "#},
        output
    );
}

/// Multiple daemon processes; in this test, the first one shuts down
/// cleanly (because the test sends a SIGTERM to the daemon), which then
/// shuts down all of the other processes. Note that the first daemon is
/// already stopped at that point and is *not* stopped again (but its
/// `post` command *is* run).
#[test_log::test(tokio::test)]
async fn multiple_daemons_first_one_exits() {
    let config = r##"
        [[processes]]
        name = "daemon1"
        pre = [ "/bin/sh", "-c", "echo daemon1-pre >> {result_path}" ]
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon1", "{result_path}", "{temp_path}" ]
        post = [ "/bin/sh", "-c", "echo daemon1-post >> {result_path}" ]

        # Ground Control starts daemons (invokes their `run` commands)
        # as fast as it can and has no way to know if a daemon is going
        # to stay running (which is why it begins monitoring the
        # daemons *after* startup has completed); we need to serialize
        # the startup operations in the test though so that we can have
        # predictable output.
        [[processes]]
        name = "wait-daemon1-start"
        pre = [ "/bin/sh", "{wait-daemon-start.sh}", "daemon1", "{temp_path}" ]

        [[processes]]
        name = "daemon2"
        pre = [ "/bin/sh", "-c", "echo daemon2-pre >> {result_path}" ]
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon2", "{result_path}", "{temp_path}" ]
        post = [ "/bin/sh", "-c", "echo daemon2-post >> {result_path}" ]
        "##;

    // Start Ground Control, wait for daemon2 to finish starting, then
    // get daemon1's PID, tell it to gracefully shutdown, then wait for
    // Ground Control to stop.
    let (gc, _tx, dir) = start(config).await;

    let daemon1_waiter = spawn_daemon_waiter(&dir, "daemon1");
    let daemon2_waiter = spawn_daemon_waiter(&dir, "daemon2");
    tokio::task::spawn(async move {
        daemon2_waiter.await.unwrap();

        let daemon1_pid = daemon1_waiter.await.unwrap();
        nix::sys::signal::kill(daemon1_pid, nix::sys::signal::Signal::SIGTERM).unwrap();
    });

    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            daemon1-pre
            daemon1:started
            daemon2-pre
            daemon2:started
            daemon1:shutdown-requested
            daemon1:stopped
            daemon2:shutdown-requested
            daemon2:stopped
            daemon2-post
            daemon1-post
        "#},
        output
    );
}
