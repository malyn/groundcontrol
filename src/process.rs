//! Starts and stops processes.

use color_eyre::eyre::{self, eyre, WrapErr};
use tokio::sync::{mpsc, oneshot};

use crate::{
    command::{self, CommandControl, ExitStatus},
    config::{ProcessConfig, StopMechanism},
    ShutdownReason,
};

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
) -> eyre::Result<Process> {
    tracing::info!(process_name = %config.name, "Starting process");

    // Perform the pre-run action, if provided.
    if let Some(pre_run) = &config.pre {
        let (_control, monitor) = command::run(&config.name, pre_run)
            .wrap_err_with(|| format!("`pre` command failed for process \"{}\"", config.name))?;

        match monitor.wait().await {
            ExitStatus::Exited(0) => {}
            ExitStatus::Exited(exit_code) => {
                tracing::error!(process_name = %config.name, %exit_code, "pre-run command aborted");
                return Err(eyre!(
                    "`pre` command failed for process \"{}\" (exit code {exit_code})",
                    config.name
                ));
            }
            ExitStatus::Killed => {
                tracing::error!(process_name = %config.name, "pre-run command was killed");
                return Err(eyre!(
                    "`pre` command was killed for process \"{}\"",
                    config.name
                ));
            }
        }
    }

    // Run the process itself (if this is a daemon process with a `run`
    // command).
    let handle = if let Some(run) = &config.run {
        let (daemon_sender, daemon_receiver) = oneshot::channel();

        let (control, monitor) = command::run(&config.name, run)
            .wrap_err_with(|| format!("`run` command failed for process \"{}\"", config.name))?;

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
    pub(crate) async fn stop_process(self) -> eyre::Result<()> {
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
                            let (_pid, exit_receiver) = command::run(&self.config.name, &command)
                                .wrap_err_with(|| {
                                format!(
                                    "`stop` command failed for process \"{}\"",
                                    self.config.name
                                )
                            })?;

                            match exit_receiver.wait().await {
                                ExitStatus::Exited(0) => {}
                                ExitStatus::Exited(exit_code) => {
                                    tracing::error!(process_name = %self.config.name, %exit_code, "stop command aborted");
                                    return Err(eyre!(
                                        "`stop` command failed for process \"{}\" (exit code {exit_code})",
                                        self.config.name
                                    ));
                                }
                                ExitStatus::Killed => {
                                    tracing::error!(process_name = %self.config.name, "stop command was killed");
                                    return Err(eyre!(
                                        "`stop` command was killed for process \"{}\"",
                                        self.config.name
                                    ));
                                }
                            }
                        }
                    };

                    // Wait for the daemon to stop.
                    match daemon_receiver.await {
                        Ok(ExitStatus::Exited(0)) => {
                            tracing::debug!(process_name = %self.config.name, "Daemon exited cleanly");
                        }
                        Ok(ExitStatus::Exited(exit_code)) => {
                            tracing::warn!(process_name = %self.config.name, %exit_code, "Daemon exited with non-zero exit code");
                        }
                        Ok(ExitStatus::Killed) => {
                            tracing::warn!(process_name = %self.config.name, "Daemon was killed");
                        }
                        Err(_) => {
                            tracing::error!("Daemon sender dropped before delivering exit signal.")
                        }
                    }
                }
            }
            ProcessHandle::OneShot => {}
        };

        // Execute the `post`(-run) command.
        if let Some(post_run) = &self.config.post {
            let (_control, monitor) =
                command::run(&self.config.name, post_run).wrap_err_with(|| {
                    format!("`post` command failed for process \"{}\"", self.config.name)
                })?;

            match monitor.wait().await {
                ExitStatus::Exited(0) => {}
                ExitStatus::Exited(exit_code) => {
                    tracing::error!(process_name = %self.config.name, %exit_code, "post-run command aborted");
                    return Err(eyre!(
                        "`post` command failed for process \"{}\" (exit code {exit_code})",
                        self.config.name
                    ));
                }
                ExitStatus::Killed => {
                    tracing::error!(process_name = %self.config.name, "post-run command was killed");
                    return Err(eyre!(
                        "`post` command was killed for process \"{}\"",
                        self.config.name
                    ));
                }
            }
        }

        // The process has been stopped.
        Ok(())
    }
}
