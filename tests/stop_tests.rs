//! Tests the verify different aspects of the `stop` configurations that
//! stop long-running daemons.

use indoc::indoc;

use crate::common::{spawn_daemon_waiter, start, stop};

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
        post = [ "/bin/sh", "-c", "echo daemon-post >> {result_path}" ]
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
            daemon-post
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
        post = [ "/bin/sh", "-c", "echo daemon-post >> {result_path}" ]
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
            daemon-post
        "#},
        output
    );
}

/// `stop` can be a command that knows how to shut down the daemon, in
/// this case, a `kill` command that sends a SIGTERM.
#[test_log::test(tokio::test)]
async fn stop_command() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon", "{result_path}", "{temp_path}" ]
        stop = [ "/bin/sh", "-c", "kill -TERM `cat {temp_path}/daemon.pid`" ]
        post = [ "/bin/sh", "-c", "echo daemon-post >> {result_path}" ]
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
            daemon-post
        "#},
        output
    );
}

/// `stop` commands that fail do *not* stop the shutdown process, but
/// instead proceed to the next daemon to stop. Note that this will
/// almost certainly leave the original daemon running, which may block
/// the shutdown of the container or VM (and is outside the scope of
/// Ground Control).
///
/// However, we don't want to block the test, so we configure
/// *daemon1's* `post` command to kill the daemon2 process. (this is not
/// a normal thing to do and should not be replicated outside of testing
/// scenarios)
///
/// Note that Ground Control *could* be changed to kill every processes
/// in the daemon's process group as an attempt to clean up after this
/// situation. *But,* it is also worth noting that this is issue is
/// effectively a bug in the Ground Control specification, since `stop`
/// commands should not fail in their *attempt* to shut down their
/// target daemon.
#[test_log::test(tokio::test)]
async fn failed_stop_command_continues_shutdown() {
    let config = r##"
        [[processes]]
        name = "daemon1"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon1", "{result_path}", "{temp_path}" ]
        # Shut down daemon2 (which never got the `stop` signal) so that
        # the test will not hang. (don't do this in Prod)
        post = [ "/bin/sh", "-c", "kill -TERM `cat {temp_path}/daemon2.pid`" ]

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
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon2", "{result_path}", "{temp_path}" ]
        stop = [ "/bin/sh", "-c", "exit 1" ]
        # Note that `post` will be run even though `stop` failed! This
        # is the same behavior you get if the signal-based `stop` fails.
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

    // Note that the daemon2's graceful shutdown (including it's `post`
    // command running) are only possible because of daemon1's `post`
    // command. Normally daemon2 would continue running, even after
    // Ground Control has tried to exit.
    assert_eq!(
        indoc! {r#"
            daemon1:started
            daemon2:started
            daemon2-post
            daemon1:shutdown-requested
            daemon1:stopped
            daemon2:shutdown-requested
            daemon2:stopped
        "#},
        output
    );
}

/// `stop` commands that fail do *not* stop the shutdown process, but
/// instead proceed to the next daemon to stop. Note that this will
/// almost certainly leave the original daemon running, which may block
/// the shutdown of the container or VM (and is outside the scope of
/// Ground Control).
///
/// However, we don't want to block the test, so we configure
/// *daemon1's* `post` command to kill the daemon2 process. (this is not
/// a normal thing to do and should not be replicated outside of testing
/// scenarios)
///
/// Note that Ground Control *could* be changed to kill every processes
/// in the daemon's process group as an attempt to clean up after this
/// situation. *But,* it is also worth noting that this is issue is
/// effectively a bug in the Ground Control specification, since `stop`
/// commands should not fail in their *attempt* to shut down their
/// target daemon.
#[test_log::test(tokio::test)]
async fn killed_stop_command_continues_shutdown() {
    let config = r##"
        [[processes]]
        name = "daemon1"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon1", "{result_path}", "{temp_path}" ]
        # Shut down daemon2 (which never got the `stop` signal) so that
        # the test will not hang. (don't do this in Prod)
        post = [ "/bin/sh", "-c", "kill -TERM `cat {temp_path}/daemon2.pid`" ]

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
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon2", "{result_path}", "{temp_path}" ]
        stop = [ "/bin/sh", "-c", "kill -9 $$" ]
        # Note that `post` will be run even though `stop` failed! This
        # is the same behavior you get if the signal-based `stop` fails.
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

    // Note that the daemon2's graceful shutdown (including it's `post`
    // command running) are only possible because of daemon1's `post`
    // command. Normally daemon2 would continue running, even after
    // Ground Control has tried to exit.
    assert_eq!(
        indoc! {r#"
            daemon1:started
            daemon2:started
            daemon2-post
            daemon1:shutdown-requested
            daemon1:stopped
            daemon2:shutdown-requested
            daemon2:stopped
        "#},
        output
    );
}

/// `stop` commands that do not exist do *not* stop the shutdown
/// process, but instead proceed to the next daemon to stop. Note that
/// this will almost certainly leave the original daemon running, which
/// may block the shutdown of the container or VM (and is outside the
/// scope of Ground Control).
///
/// However, we don't want to block the test, so we configure
/// *daemon1's* `post` command to kill the daemon2 process. (this is not
/// a normal thing to do and should not be replicated outside of testing
/// scenarios)
///
/// Note that Ground Control *could* be changed to kill every processes
/// in the daemon's process group as an attempt to clean up after this
/// situation. *But,* it is also worth noting that this is issue is
/// effectively a bug in the Ground Control specification, since `stop`
/// commands should not fail in their *attempt* to shut down their
/// target daemon.
#[test_log::test(tokio::test)]
async fn not_found_stop_command_continues_shutdown() {
    let config = r##"
        [[processes]]
        name = "daemon1"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon1", "{result_path}", "{temp_path}" ]
        # Shut down daemon2 (which never got the `stop` signal) so that
        # the test will not hang. (don't do this in Prod)
        post = [ "/bin/sh", "-c", "kill -TERM `cat {temp_path}/daemon2.pid`" ]

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
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon2", "{result_path}", "{temp_path}" ]
        stop = "/user/binary/nope"
        # Note that `post` will be run even though `stop` failed! This
        # is the same behavior you get if the signal-based `stop` fails.
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

    // Note that the daemon2's graceful shutdown (including it's `post`
    // command running) are only possible because of daemon1's `post`
    // command. Normally daemon2 would continue running, even after
    // Ground Control has tried to exit.
    assert_eq!(
        indoc! {r#"
            daemon1:started
            daemon2:started
            daemon2-post
            daemon1:shutdown-requested
            daemon1:stopped
            daemon2:shutdown-requested
            daemon2:stopped
        "#},
        output
    );
}
