//! Tests that verify different aspects of the startup behavior (where
//! "startup" is defined as the process of getting all long-running
//! processes into their started state).

use groundcontrol::config::Config;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Prepares the test directory and test "daemon" script, performs
/// template replacement in the provided configuration, runs Ground
/// Control, waits for it to stop, and then collects the contents of the
/// result file.
///
/// The following template variables will be replaced in the `config`
/// string:
///
/// - `{test-daemon.sh}` is replaced with the path to the
///   `test-daemon.sh` script that can be used to test long-running
///   daemons. The script takes one argument: the name of the daemon,
///   which will be output to the results file when the daemon starts.
/// - `{result_path}` is replaced with the full path to the result file
///   that will be read at the completion of the test. This can be used
///   to verify that each process was started (and in the case of the
///   daemon, the reason for its exit).
async fn run(config: &str) -> (anyhow::Result<()>, String) {
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
    tokio::fs::write(
        &test_daemon_path,
        format!(
        r##"
            #!/usr/bin/env sh
            set -Eeuo pipefail

            DAEMON_NAME=$1

            echo $DAEMON_NAME >> {result_path}

            exec sleep 5
        "##
        )
        .as_bytes(),
    )
    .await
    .unwrap();

    // Parse the test configuration, replace `result_path` and
    // `test-daemon` in the config.
    let config: Config = toml::from_str(
        &config
            .replace("{result_path}", &result_path)
            .replace("{test-daemon.sh}", &test_daemon_path),
    )
    .unwrap();

    // Run the specification and collect the results.
    let (_tx, rx) = mpsc::unbounded_channel();
    let result = groundcontrol::run(config, rx).await;
    let result_file = match tokio::fs::read_to_string(result_path).await {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => panic!("Unable to read result file: {err}"),
    };

    // Return the result and results file.
    (result, result_file)
}

/// Basic test: execute a single one-shot process.
#[test_log::test(tokio::test)]
async fn one_shot() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]

        [[processes]]
        name = "shutdown"
        run = [ "/bin/sh", "{test-daemon.sh}", "shutdown" ]
        "##;

    let (result, output) = run(config).await;
    assert!(result.is_ok());
    assert_eq!("a-pre\nshutdown\n", output);
}

/// Basic daemon test: starts a single daemon and waits for it shut down
/// on its own.
#[test_log::test(tokio::test)]
async fn single_daemon_success() {
    let config = r##"
        [[processes]]
        name = "daemon"
        run = [ "/bin/sh", "{test-daemon.sh}", "daemon" ]
        "##;

    let (result, output) = run(config).await;
    assert!(result.is_ok());
    assert_eq!("daemon\n", output);
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

    let (result, output) = run(config).await;
    assert!(result.is_err());
    assert_eq!("a-pre\na-post\n", output);
}
