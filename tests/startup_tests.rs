//! Tests that verify different aspects of the startup behavior (where
//! "startup" is defined as the process of getting all long-running
//! processes into their started state).

use std::{future::Future, time::Duration};

use groundcontrol::config::Config;
use nix::unistd::Pid;
use tempfile::TempDir;
use tokio::sync::{
    mpsc::{self, UnboundedSender},
    oneshot,
};

/// Prepares the test directory and test "daemon" script, performs
/// template replacement in the provided configuration, runs Ground
/// Control, and returns the shutdown handle and temp directory, the
/// latter which can be used to detect started daemons, and to stop
/// those same daemons.
///
/// The following template variables will be replaced in the `config`
/// string:
///
/// - `{test-daemon.sh}` is replaced with the path to the
///   `test-daemon.sh` script that can be used to test long-running
///   daemons. The script takes three arguments: the name of the daemon,
///   which will be output to the results file when the daemon starts,
///   stops, or is asked to shutdown; the path to the results file; the
///   path to the directory where the daemon's PID should be stored.
/// - `{result_path}` is replaced with the full path to the result file
///   that will be read at the completion of the test. This can be used
///   to verify that each process was started (and in the case of the
///   daemon, the reason for its exit).
async fn start(
    config: &str,
) -> (
    impl Future<Output = Result<(), groundcontrol::Error>>,
    UnboundedSender<()>,
    TempDir,
) {
    // Create a temp directory into which we can write output from the
    // commands, as a simple way of verifying that the commands are in
    // fact run in the proper order.
    let dir = TempDir::new().unwrap();
    let result_path = dir.path().join("results.txt").to_str().unwrap().to_string();

    // Write the "test-daemon.sh" script into the temp directory.
    let test_daemon_path = dir
        .path()
        .join("test-daemon.sh")
        .to_str()
        .unwrap()
        .to_string();
    tokio::fs::write(&test_daemon_path, include_bytes!("test-daemon.sh"))
        .await
        .unwrap();

    // Parse the test configuration, replacing our template variables
    // before passing the config to the parser.
    let config: Config = toml::from_str(
        &config
            .replace("{result_path}", &result_path)
            .replace("{temp_path}", dir.path().to_str().unwrap())
            .replace("{test-daemon.sh}", &test_daemon_path),
    )
    .unwrap();

    // Start Ground Control and return the handles.
    let (tx, rx) = mpsc::unbounded_channel();
    let gc = groundcontrol::run(config, rx);
    (gc, tx, dir)
}

/// Waits for Ground Control to stop, then collects the contents of the
/// result file.
async fn stop(
    gc: impl Future<Output = Result<(), groundcontrol::Error>>,
    dir: TempDir,
) -> (Result<(), groundcontrol::Error>, String) {
    // Wait for Ground Control to stop.
    let result = gc.await;

    // Collect the results.
    let result_path = dir.path().join("results.txt").to_str().unwrap().to_string();
    let result_file = match tokio::fs::read_to_string(result_path).await {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => panic!("Unable to read result file: {err}"),
    };

    // Return the result and results file.
    (result, result_file)
}

/// Spawns a task that will wait for the daemon with the given name to
/// start, then returns the PID of the daemon.
fn spawn_daemon_waiter(dir: &TempDir, daemon_name: &str) -> oneshot::Receiver<Pid> {
    let (tx, rx) = oneshot::channel();
    let daemon_pid_path = dir
        .path()
        .join(format!("{daemon_name}.pid"))
        .to_str()
        .unwrap()
        .to_string();

    tokio::task::spawn(async move {
        loop {
            match tokio::fs::read_to_string(&daemon_pid_path).await {
                Ok(text) => {
                    let pid = Pid::from_raw(text.trim().parse::<i32>().unwrap());
                    tx.send(pid).unwrap();
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
                Err(err) => panic!("Unable to read PID file: {err}"),
            };
        }
    });

    rx
}

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
        pre = "/user/binary/nope"
        run = [ "/bin/sh", "-c", "echo b-run >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]

        [[processes]]
        name = "c"
        pre = [ "/bin/sh", "-c", "echo c-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo c-post >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;
    assert_eq!(Err(groundcontrol::Error::StartupAborted), result);
    assert_eq!("a-pre\na-post\n", output);
}
