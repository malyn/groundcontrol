//! Tests that verify different aspects of the startup behavior (where
//! "startup" is defined as the process of getting all long-running
//! processes into their started state).

use groundcontrol::config::Config;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Verifies that a failed `pre` execution aborts all subsequent command
/// executions *and* runs stop/post commands for anything that was
/// started.
#[test_log::test(tokio::test)]
async fn failed_pre_aborts_startup() {
    // Create a temp directory into which we can write output from the
    // commands, as a simple way of verifying that the commands are in
    // fact run in the proper order.
    let dir = TempDir::new().unwrap();
    let result_path = dir.path().join("results.txt").to_str().unwrap().to_string();

    // Create and parse the test configuration.
    let config: Config = toml::from_str(&format!(
        r##"
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
        "##
    ))
    .unwrap();

    // Run the specification; only `a-pre` and `a-post` should run.
    let (_tx, rx) = mpsc::unbounded_channel();
    let result = groundcontrol::run(config, rx).await;
    assert!(result.is_err());

    let result = tokio::fs::read_to_string(result_path).await.unwrap();
    assert_eq!("a-pre\na-post\n", result);
}
