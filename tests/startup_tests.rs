//! Tests that verify different aspects of the startup behavior (where
//! "startup" is defined as the process of getting all long-running
//! processes into their started state).

use groundcontrol::config::{CommandConfig, Config, ProcessConfig};
use tokio::sync::mpsc;

/// Verifies that a failed `pre` execution aborts all subsequent command
/// executions.
#[tokio::test]
async fn failed_pre_aborts_startup() {
    let config = Config {
        processes: vec![
            ProcessConfig {
                name: String::from("a"),
                pre: Some(CommandConfig {
                    user: None,
                    env_vars: Default::default(),
                    program: String::from("/bin/echo"),
                    args: vec![String::from("a-pre")],
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
                    program: String::from("/bin/echo"),
                    args: vec![String::from("c-pre")],
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
}
