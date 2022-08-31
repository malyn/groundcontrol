//! Starts and stops processes.

use anyhow::{bail, Context};
use tokio::sync::{mpsc, oneshot};

use crate::{
    command::{self, CommandControl, ExitStatus},
    config::{ProcessConfig, StopMechanism},
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

impl Process {
    /// Starts the process and returns a handle to the process.
    pub async fn start(
        config: ProcessConfig,
        shutdown_sender: mpsc::UnboundedSender<()>,
    ) -> anyhow::Result<Self> {
        tracing::info!(name = %config.name, "Starting process");

        // Perform the pre-run action, if provided.
        if let Some(pre_run) = &config.pre {
            let (_control, monitor) = command::run(&config.name, pre_run)
                .with_context(|| "Error executing pre-run command")?;

            let exit_status = monitor.wait().await;
            if !matches!(exit_status, ExitStatus::Exited(0)) {
                // TODO: Start shutting everything down if we get this.
                panic!("pre-run command failed");
            }
        }

        // Run the process itself (if this is a daemon process with a
        // `run` command).
        let handle = if let Some(run) = &config.run {
            let (daemon_sender, daemon_receiver) = oneshot::channel();

            let (control, monitor) =
                command::run(&config.name, run).with_context(|| "Error starting run command")?;

            // Spawn a task to wait for the command to exit, then notify
            // both ourselves (to allow `stop` to return) and the
            // shutdown listener that our daemon process has exited.
            tokio::spawn(async move {
                let exit_status = monitor.wait().await;

                if daemon_sender.send(exit_status).is_err() {
                    tracing::error!("Daemon receiver dropped before receiving exit signal.");
                }

                if let Err(err) = shutdown_sender.send(()) {
                    tracing::error!(
                        ?err,
                        "Shutdown receiver dropped before all processes have exited."
                    );
                }
            });

            ProcessHandle::Daemon(control, daemon_receiver)
        } else {
            ProcessHandle::OneShot
        };

        Ok(Self { config, handle })
    }

    /// Stops the process: executes the `stop` command/signal if this is
    /// a daemon process; waits for the process to exit; runs the `post`
    /// command (if present).
    pub async fn stop(self) -> anyhow::Result<()> {
        tracing::info!(name = %self.config.name, "Stopping process.");

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
                            .with_context(|| "Error executing stop command")?;

                        match exit_receiver.wait().await {
                            ExitStatus::Exited(0) => {}
                            ExitStatus::Exited(exit_code) => {
                                bail!("stop process failed: {exit_code}");
                            }
                            ExitStatus::Killed => {
                                bail!("stop process was killed");
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
                .with_context(|| "Error executing post-run command")?;

            let exit_status = monitor.wait().await;
            if !matches!(exit_status, ExitStatus::Exited(0)) {
                tracing::error!(?exit_status, "post-run command failed.");
            }
        }

        // The process has been stopped.
        Ok(())
    }
}
