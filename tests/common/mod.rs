//! Helper functions for Ground Control integration tests

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
/// - `{result_path}` is replaced with the full path to the result file
///   that will be read at the completion of the test. This can be used
///   to verify that each process was started (and in the case of the
///   daemon, the reason for its exit).
/// - `{temp_path}` is replaced with the path to the test-specific
///   temporary directory (which can be used to store daemon PID files,
///   for example).
/// - `{test-daemon.sh}` is replaced with the path to the
///   `test-daemon.sh` script that can be used to test long-running
///   daemons. The script takes three arguments: the name of the daemon,
///   which will be output to the results file when the daemon starts,
///   stops, or is asked to shutdown; the path to the results file; the
///   path to the directory where the daemon's PID should be stored.
/// - `{wait-daemon-start.sh}` is replaced with the path to the
///   `wait-daemon-start.sh` script that can be used to wait for a
///   long-running daemon to finish starting and enter its sleep loop.
///   The script takes two arguments: the name of the daemon and the
///   path to the directory where the daemon's PID is stored. This
///   script allows tests to ensure that a daemon has started *and
///   output its startup text to the results file* before proceeding to
///   the next process. (without that, test results would be
///   inconsistent depending on which process got to run first, and for
///   how long).
pub async fn start(
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

    // Write the test scripts into the temp directory.
    let test_daemon_path = dir
        .path()
        .join("test-daemon.sh")
        .to_str()
        .unwrap()
        .to_string();
    tokio::fs::write(&test_daemon_path, include_bytes!("test-daemon.sh"))
        .await
        .unwrap();

    let wait_daemon_start_path = dir
        .path()
        .join("wait-daemon-start.sh")
        .to_str()
        .unwrap()
        .to_string();
    tokio::fs::write(
        &wait_daemon_start_path,
        include_bytes!("wait-daemon-start.sh"),
    )
    .await
    .unwrap();

    // Parse the test configuration, replacing our template variables
    // before passing the config to the parser.
    let config: Config = toml::from_str(
        &config
            .replace("{result_path}", &result_path)
            .replace("{temp_path}", dir.path().to_str().unwrap())
            .replace("{test-daemon.sh}", &test_daemon_path)
            .replace("{wait-daemon-start.sh}", &wait_daemon_start_path),
    )
    .unwrap();

    // Start Ground Control and return the handles.
    let (tx, rx) = mpsc::unbounded_channel();
    let gc = groundcontrol::run(config, rx);
    (gc, tx, dir)
}

/// Waits for Ground Control to stop, then collects the contents of the
/// result file.
pub async fn stop(
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
#[allow(dead_code)]
pub fn spawn_daemon_waiter(dir: &TempDir, daemon_name: &str) -> oneshot::Receiver<Pid> {
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

/// Asserts that the Ground Control result is the `StartupAborted` error
/// and that the error report matches the expected text.
#[allow(dead_code)]
pub fn assert_startup_aborted(expected: &str, result: Result<(), groundcontrol::Error>) {
    match result {
        Err(groundcontrol::Error::StartupAborted(report)) => {
            let report_text: String = report.chain().map(|r| format!("{r}\n")).collect();
            assert_eq!(expected, report_text,);
        }
        Ok(_) | Err(_) => panic!("Expected StartupAborted error."),
    };
}
