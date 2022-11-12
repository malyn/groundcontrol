//! Starts and stops processes.

use tokio::sync::{mpsc, oneshot};

use crate::{
    command::{self, CommandControl, ExitStatus},
    config::{ProcessConfig, StopMechanism},
    ShutdownReason,
};

/// Errors generated when starting processes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StartProcessError {
    /// Pre-run command failed.
    /// TODO: Rename this to something that indicates that we couldn't even start the process (bad path name or not executable or something?).
    #[error("pre-run command failed")]
    PreRunFailed,

    /// Pre-run command aborted with a non-zero exit code.
    #[error("pre-run command aborted with exit code: {0}")]
    PreRunAborted(i32),

    /// Pre-run command was killed before it could exit.
    #[error("pre-run commadn killed before it could exit")]
    PreRunKilled,

    /// Run command failed.
    #[error("run command failed")]
    RunFailed,
}

/// Errors generated when stopping processes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StopProcessError {
    /// Stop command failed.
    #[error("stop command failed")]
    StopFailed,

    /// Process aborted with a non-zero exit code.
    #[error("process aborted with exit code: {0}")]
    ProcessAborted(i32),

    /// Process was killed before it could be stopped.
    #[error("process killed before it could be stopped")]
    ProcessKilled,

    /// Post-run command failed.
    #[error("post-run command failed")]
    PostRunFailed,
}

/// Process being managed by Ground Control.
#[derive(Debug)]
pub(crate) struct Process {
    config: ProcessConfig,
    handle: ProcessHandle,
}

#[derive(Debug)]
enum ProcessHandle {
    Daemon(CommandControl, oneshot::Receiver<ExitStatus>),
    OneShot,
}

/// Starts the process and returns a handle to the process.
pub(crate) async fn start_process(
    config: ProcessConfig,
    process_stopped: mpsc::UnboundedSender<ShutdownReason>,
) -> Result<Process, StartProcessError> {
    tracing::info!(process_name = %config.name, "Starting process");

    // Perform the pre-run action, if provided.
    if let Some(pre_run) = &config.pre {
        let (_control, monitor) =
            command::run(&config.name, pre_run).map_err(|_| StartProcessError::PreRunFailed)?;

        match monitor.wait().await {
            ExitStatus::Exited(0) => {}
            ExitStatus::Exited(exit_code) => {
                tracing::error!(process_name = %config.name, %exit_code, "pre-run command aborted");
                return Err(StartProcessError::PreRunAborted(exit_code));
            }
            ExitStatus::Killed => {
                tracing::error!(process_name = %config.name, "pre-run command was killed");
                return Err(StartProcessError::PreRunKilled);
            }
        }
    }

    // Run the process itself (if this is a daemon process with a `run`
    // command).
    let handle = if let Some(run) = &config.run {
        let (daemon_sender, daemon_receiver) = oneshot::channel();

        let (control, monitor) =
            command::run(&config.name, run).map_err(|_| StartProcessError::RunFailed)?;

        // Spawn a task to wait for the command to exit, then notify
        // both ourselves (to allow `stop` to return) and the shutdown
        // listener that our daemon process has exited.
        let process_name = config.name.clone();
        tokio::spawn(async move {
            let exit_status = monitor.wait().await;

            if daemon_sender.send(exit_status).is_err() {
                tracing::error!(%process_name, "Daemon receiver dropped before receiving exit signal.");
            }

            let shutdown_reason = match exit_status {
                ExitStatus::Exited(0) => ShutdownReason::DaemonExited,
                ExitStatus::Exited(_) | ExitStatus::Killed => ShutdownReason::DaemonFailed,
            };

            if let Err(err) = process_stopped.send(shutdown_reason) {
                tracing::error!(
                    %process_name,
                    ?err,
                    "Shutdown receiver dropped before all processes have exited."
                );
            }
        });

        ProcessHandle::Daemon(control, daemon_receiver)
    } else {
        ProcessHandle::OneShot
    };

    Ok(Process { config, handle })
}

impl Process {
    /// Stops the process: executes the `stop` command/signal if this is
    /// a daemon process; waits for the process to exit; runs the `post`
    /// command (if present).
    pub(crate) async fn stop_process(self) -> Result<(), StopProcessError> {
        tracing::info!(process_name = %self.config.name, "Stopping process.");

        // Stop the process (which is only required for daemon
        // processes; one-shot processes never "started").
        match self.handle {
            ProcessHandle::Daemon(control, mut daemon_receiver) => {
                // Has the daemon already shut down? If so, we do not
                // need to stop it (we just need to run the `post`
                // command, if any).
                if daemon_receiver.try_recv().is_ok() {
                    tracing::debug!(process_name = %self.config.name, "Daemon already exited; no need to `stop` it.");
                } else {
                    // Stop the daemon.
                    match self.config.stop {
                        StopMechanism::Signal(signal) => {
                            if let Err(err) = control.kill(signal.into()) {
                                tracing::warn!(?err, "Error stopping daemon process.");
                            }
                        }
                        StopMechanism::Command(command) => {
                            let (_pid, exit_receiver) =
                                command::run(&self.config.name, &command)
                                    .map_err(|_| StopProcessError::StopFailed)?;

                            match exit_receiver.wait().await {
                                ExitStatus::Exited(0) => {
                                    tracing::debug!(process_name = %self.config.name, "Daemon process exited cleanly");
                                }
                                ExitStatus::Exited(exit_code) => {
                                    tracing::warn!(process_name = %self.config.name, %exit_code, "Daemon process aborted with non-zero exit code");
                                    return Err(StopProcessError::ProcessAborted(exit_code));
                                }
                                ExitStatus::Killed => {
                                    tracing::warn!(process_name = %self.config.name, "Daemon process was killed before it could stop");
                                    return Err(StopProcessError::ProcessKilled);
                                }
                            }
                        }
                    };

                    // Wait for the daemon to stop.
                    if daemon_receiver.await.is_err() {
                        tracing::error!("Daemon sender dropped before delivering exit signal.");
                    }
                }
            }
            ProcessHandle::OneShot => {}
        };

        // Execute the `post`(-run) command.
        if let Some(post_run) = &self.config.post {
            let (_control, monitor) = command::run(&self.config.name, post_run)
                .map_err(|_| StopProcessError::PostRunFailed)?;

            let exit_status = monitor.wait().await;
            if !matches!(exit_status, ExitStatus::Exited(0)) {
                tracing::error!(?exit_status, "post-run command failed.");
            }
        }

        // The process has been stopped.
        Ok(())
    }
}
