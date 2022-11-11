//! Tests that verify different aspects of the startup behavior (where
//! "startup" is defined as the process of getting all long-running
//! processes into their started state).

use crate::common::{spawn_daemon_waiter, start, stop};

mod common;

/// Basic daemon test: starts a single "daemon" (actually just a script
/// that exits immediately) and waits for it shut down on its own
/// (which, again, happens immediately).
///
/// This demonstrates that a "daemon" is really just a process that
/// Ground Control does *not* wait to exit before starting other
/// processes. In other words, all daemon processes (`run` commands) are
/// started in sequence and we only monitor their exit *after* they have
/// all started.
///
/// This is in contrast with `pre` and `post` commands which are
/// one-shot commands that block startup/shutdown until they complete.
#[test_log::test(tokio::test)]
async fn single_daemon_success() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "-c", "echo daemon >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;
    assert!(result.is_ok());
    assert_eq!("daemon\n", output);
}

/// Basic daemon test: starts a real daemon, waits for it to start, then
/// requests that Ground Control perform a controlled shutdown.
#[test_log::test(tokio::test)]
async fn single_daemon_graceful_shutdown() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon", "{result_path}", "{temp_path}" ]
        "##;

    // Start Ground Control, wait for the daemon to finish starting, ask
    // Ground Control to shutdown, then wait for Ground Control to stop.
    let (gc, tx, dir) = start(config).await;

    let daemon_waiter = spawn_daemon_waiter(&dir, "daemon");
    tokio::task::spawn(async move {
        daemon_waiter.await.unwrap();
        tx.send(()).unwrap();
    });

    let (result, output) = stop(gc, dir).await;

    // This should result in a controlled shutdown.
    assert!(result.is_ok());
    assert_eq!(
        "daemon:started\ndaemon:shutdown-requested\ndaemon:stopped\n",
        output
    );
}

/// Basic daemon failure test: starts a single daemon and expects it to
/// fail during startup (which happens because we do *not* provide any
/// arguments to the `test-daemon.sh` script).
///
/// Note that this is technically a delayed failure that is detected
/// *after* Ground Control reaches the startup phase; immediate startup
/// failures -- ones that block the remainder of the startup process --
/// will be only be triggered if the process cannot even be started: the
/// binary cannot be found or is not executable, a required environment
/// variable is missing, etc.
#[test_log::test(tokio::test)]
async fn single_daemon_failure() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;
    assert_eq!(Err(groundcontrol::Error::AbnormalShutdown), result);
    assert_eq!("", output);
}
