//! Tests that verify different aspects of the startup behavior (where
//! "startup" is defined as the process of getting all long-running
//! processes into their started state).

use std::sync::{Arc, Mutex};

use groundcontrol::{
    config::{CommandConfig, Config, ProcessConfig},
    MockManageProcess, MockStartProcess, StartProcessError,
};
use mockall::Sequence;
use tokio::sync::mpsc;

/// Verifies that a failed `pre` execution aborts all subsequent command
/// executions.
#[tokio::test]
async fn failed_pre_aborts_startup_real_processes() {
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
    let result = groundcontrol::run(config.processes, rx).await;
    assert_eq!(Err(StartProcessError::PreRunFailed), result);
}

/// Verifies that a failed `pre` execution aborts all subsequent command
/// executions.
#[tokio::test]
async fn failed_pre_aborts_startup() {
    // Create three mock processes: the first is a daemon process will
    // be started and stopped, the second is a one-shot process that
    // fails to start, the third is never started.
    let mut seq = Sequence::new();

    let mut process_a: MockStartProcess<MockManageProcess> = MockStartProcess::new();
    process_a
        .expect_start_process()
        .once()
        .in_sequence(&mut seq)
        .returning(|_| {
            // We expect this, but do not need to check for it (hence no
            // `once()`); that validation happens in a different test.
            let mut process_a_manager = MockManageProcess::new();
            process_a_manager.expect_stop_process().return_const(Ok(()));
            Ok(process_a_manager)
        });

    let mut process_b: MockStartProcess<MockManageProcess> = MockStartProcess::new();
    process_b
        .expect_start_process()
        .once()
        .in_sequence(&mut seq)
        .return_once(|_| Err(StartProcessError::PreRunFailed));

    let process_c: MockStartProcess<MockManageProcess> = MockStartProcess::new();

    // Run the specification; only `a-pre` should run.
    let spec = vec![process_a, process_b, process_c];
    let (_tx, rx) = mpsc::unbounded_channel();
    let result = groundcontrol::run(spec, rx).await;
    assert_eq!(Err(StartProcessError::PreRunFailed), result);
}

/// Verifies that a failed `pre` execution shuts down all
/// previously-started long-running processes.
#[tokio::test]
async fn failed_pre_shuts_down_earlier_processes() {
    // Create three mock processes: the first is a daemon process will
    // be started and stopped, the second is a one-shot process that
    // fails to start, the third is never started.
    let mut seq = Sequence::new();

    // This ProcessManager is *last* in the sequence, but is returned by
    // the *first* StartProcess trait in the sequence. We need to pass
    // (a clone) of the manager into the StartProcess closure, but can't
    // initialize the manager until we get to the proper place in the
    // expectation sequence. The solution is to wrap the manager in an
    // Arc-Mutex-Option.
    let process_a_manager: Arc<Mutex<Option<MockManageProcess>>> = Default::default();

    let pam = process_a_manager.clone();
    let mut process_a: MockStartProcess<MockManageProcess> = MockStartProcess::new();
    process_a
        .expect_start_process()
        .once()
        .in_sequence(&mut seq)
        .returning(move |_| Ok(pam.lock().unwrap().take().unwrap()));

    let mut process_b: MockStartProcess<MockManageProcess> = MockStartProcess::new();
    process_b
        .expect_start_process()
        .once()
        .in_sequence(&mut seq)
        .return_once(|_| Err(StartProcessError::PreRunFailed));

    let mut pam = MockManageProcess::new();
    pam.expect_stop_process()
        .once()
        .in_sequence(&mut seq)
        .return_const(Ok(()));
    *process_a_manager.lock().unwrap() = Some(pam);

    let process_c: MockStartProcess<MockManageProcess> = MockStartProcess::new();

    // Run the specification.
    let spec = vec![process_a, process_b, process_c];
    let (_tx, rx) = mpsc::unbounded_channel();
    let result = groundcontrol::run(spec, rx).await;
    assert_eq!(Err(StartProcessError::PreRunFailed), result);
}
