//! Tests that verify different aspects of the startup behavior (where
//! "startup" is defined as the process of getting all long-running
//! processes into their started state).

use groundcontrol::config::{CommandConfig, Config, ProcessConfig};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Verifies that a failed `pre` execution aborts all subsequent command
/// executions.
#[tokio::test]
async fn failed_pre_aborts_startup() {
    // Create a temp directory into which we can write output from the
    // commands, as a simple way of verifying that the commands are in
    // fact run in the proper order.
    let dir = TempDir::new().unwrap();
    let result_path = dir.path().join("results.txt").to_str().unwrap().to_string();

    // Create the test configuration.
    let config = Config {
        processes: vec![
            ProcessConfig {
                name: String::from("a"),
                pre: Some(CommandConfig {
                    user: None,
                    env_vars: Default::default(),
                    program: String::from("/bin/sh"),
                    args: vec![String::from("-c"), format!("echo a-pre >> {result_path}")],
                }),
                run: None,
                stop: Default::default(),
                post: None,
            },
            ProcessConfig {
                name: String::from("b"),
                pre: Some(CommandConfig {
                    user: None,
                    env_vars: Default::default(),
                    program: String::from("/user/binary/nope"),
                    args: vec![],
                }),
                run: None,
                stop: Default::default(),
                post: None,
            },
            ProcessConfig {
                name: String::from("c"),
                pre: Some(CommandConfig {
                    user: None,
                    env_vars: Default::default(),
                    program: String::from("/bin/sh"),
                    args: vec![String::from("-c"), format!("echo c-pre >> {result_path}")],
                }),
                run: None,
                stop: Default::default(),
                post: None,
            },
        ],
    };

    // Run the specification; only `a-pre` should run.
    let (_tx, rx) = mpsc::unbounded_channel();
    let result = groundcontrol::run(config, rx).await;
    assert!(result.is_err());

    let result = tokio::fs::read_to_string(result_path).await.unwrap();
    assert_eq!("a-pre\n", result);
}
