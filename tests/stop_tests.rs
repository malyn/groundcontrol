//! Tests the verify different aspects of the `stop` configurations that
//! stop long-running daemons.

use indoc::indoc;

use crate::common::{assert_startup_aborted, spawn_daemon_waiter, start, stop};

mod common;

/// The default `stop` operation is to send SIGTERM to the daemon (which
/// our `test-daemon.sh` script traps and uses to initiate a graceful
/// shutdown).
#[test_log::test(tokio::test)]
async fn stop_defaults_to_sigterm() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon", "{result_path}", "{temp_path}" ]
        "##;

    // Start Ground Control, wait for daemon to finish starting, ask
    // Ground Control to shutdown, then wait for Ground Control to stop.
    let (gc, tx, dir) = start(config).await;

    let daemon_waiter = spawn_daemon_waiter(&dir, "daemon");
    tokio::task::spawn(async move {
        daemon_waiter.await.unwrap();
        tx.send(()).unwrap();
    });

    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            daemon:started
            daemon:shutdown-requested
            daemon:stopped
        "#},
        output
    );
}

/// `stop` can be set to a different signal name if the process uses a
/// different signal to stop the daemon. Here, we switch to SIGINT,
/// which our `test-daemon.sh` script traps, but does not treat as a
/// graceful shutdown (and so it does not log a shutdown message).
#[test_log::test(tokio::test)]
async fn stop_supports_other_signals() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon", "{result_path}", "{temp_path}" ]
        stop = "SIGINT"
        "##;

    // Start Ground Control, wait for daemon to finish starting, ask
    // Ground Control to shutdown, then wait for Ground Control to stop.
    let (gc, tx, dir) = start(config).await;

    let daemon_waiter = spawn_daemon_waiter(&dir, "daemon");
    tokio::task::spawn(async move {
        daemon_waiter.await.unwrap();
        tx.send(()).unwrap();
    });

    let (result, output) = stop(gc, dir).await;

    assert!(result.is_ok());

    assert_eq!(
        indoc! {r#"
            daemon:started
        "#},
        output
    );
}
