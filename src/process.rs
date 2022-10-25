//! Starts and stops processes.

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use crate::{
    command::{self, CommandControl, ExitStatus},
    config::{ProcessConfig, StopMechanism},
    ManageProcess, StartProcess, StartProcessError, StopProcessError,
};

/// Process being managed by Ground Control.
#[derive(Debug)]
pub struct Process {
    config: ProcessConfig,
    handle: ProcessHandle,
}

#[derive(Debug)]
enum ProcessHandle {
    Daemon(CommandControl, oneshot::Receiver<ExitStatus>),
    OneShot,
}

#[async_trait]
impl StartProcess<Process> for ProcessConfig {
    async fn start_process(
        self,
        process_stopped: mpsc::UnboundedSender<()>,
    ) -> Result<Process, StartProcessError> {
        tracing::info!(process_name = %self.name, "Starting process");

        // Perform the pre-run action, if provided.
        if let Some(pre_run) = &self.pre {
            let (_control, monitor) =
                command::run(&self.name, pre_run).map_err(|_| StartProcessError::PreRunFailed)?;

            match monitor.wait().await {
                ExitStatus::Exited(0) => {}
                ExitStatus::Exited(exit_code) => {
                    tracing::error!(process_name = %self.name, %exit_code, "pre-run command aborted");
                    return Err(StartProcessError::PreRunAborted(exit_code));
                }
                ExitStatus::Killed => {
                    tracing::error!(process_name = %self.name, "pre-run command was killed");
                    return Err(StartProcessError::PreRunKilled);
                }
            }
        }

        // Run the process itself (if this is a daemon process with a
        // `run` command).
        let handle = if let Some(run) = &self.run {
            let (daemon_sender, daemon_receiver) = oneshot::channel();

            let (control, monitor) =
                command::run(&self.name, run).map_err(|_| StartProcessError::RunFailed)?;

            // Spawn a task to wait for the command to exit, then notify
            // both ourselves (to allow `stop` to return) and the
            // shutdown listener that our daemon process has exited.
            let process_name = self.name.clone();
            tokio::spawn(async move {
                let exit_status = monitor.wait().await;

                if daemon_sender.send(exit_status).is_err() {
                    tracing::error!(%process_name, "Daemon receiver dropped before receiving exit signal.");
                }

                if let Err(err) = process_stopped.send(()) {
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

        Ok(Process {
            config: self,
            handle,
        })
    }
}

#[async_trait]
impl ManageProcess for Process {
    async fn stop_process(self) -> Result<(), StopProcessError> {
        tracing::info!(process_name = %self.config.name, "Stopping process.");

        // Stop the process (which is only required for daemon
        // processes; one-shot processes never "started").
        match self.handle {
            ProcessHandle::Daemon(control, daemon_receiver) => {
                // Stop the daemon.
                match self.config.stop {
                    StopMechanism::Signal(signal) => {
                        if let Err(err) = control.kill(signal.into()) {
                            tracing::warn!(?err, "Error stopping daemon process.");
                        }
                    }
                    StopMechanism::Command(command) => {
                        let (_pid, exit_receiver) = command::run(&self.config.name, &command)
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
