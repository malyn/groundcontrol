use anyhow::{bail, Context};
use tracing::Level;

use crate::{
    command::{Command, ExitStatus},
    config::process::{ProcessConfig, ProcessType, StopMechanism},
};

#[derive(Debug)]
pub struct Process {
    config: ProcessConfig,
    daemon: Option<Command>,
}

impl Process {
    pub async fn start(config: ProcessConfig) -> anyhow::Result<Self> {
        tracing::event!(Level::INFO, name = %config.name(), "Starting process");

        // Perform the pre-start action, if provided.
        if let Some(exec_start_pre) = &config.exec_start_pre {
            let mut cmd = Command::run(
                config.name(),
                None,
                &exec_start_pre[0],
                &exec_start_pre[1..],
                &config.pass_environment,
            )
            .with_context(|| "Error starting exec_start_pre command")?;

            let exit_status = cmd.wait().await;
            if !matches!(exit_status, ExitStatus::Exited(0)) {
                // TODO: Start shutting everything down if we get this.
                panic!("exec_start_pre command failed");
            }
        }

        // Start the process itself.
        let uid_gid: Option<(u32, u32)> = if let Some(username) = &config.user {
            let user = users::get_user_by_name(username).with_context(|| "Unknown username")?;
            Some((user.uid(), user.primary_group_id()))
        } else {
            None
        };

        let mut cmd = Command::run(
            config.name(),
            uid_gid,
            &config.exec_start[0],
            &config.exec_start[1..],
            &config.pass_environment,
        )
        .with_context(|| "Error starting exec_start command")?;

        // What we do next depends on the process type: oneshot processes
        // are awaited right here, whereas daemons will be then awaited en
        // masse after all of the processes have been started.
        let daemon = match config.process_type {
            ProcessType::Oneshot => {
                let exit_status = cmd.wait().await;
                if !matches!(exit_status, ExitStatus::Exited(0)) {
                    // TODO: Start shutting everything down if we get this.
                    panic!("exec_start command failed");
                }

                None
            }
            ProcessType::Daemon => Some(cmd),
        };

        // Perform the post-start action, if provided.
        if let Some(exec_start_post) = &config.exec_start_post {
            let mut cmd = Command::run(
                config.name(),
                None,
                &exec_start_post[0],
                &exec_start_post[1..],
                &config.pass_environment,
            )
            .with_context(|| "Error starting exec_start_post command")?;

            let exit_status = cmd.wait().await;
            if !matches!(exit_status, ExitStatus::Exited(0)) {
                // TODO: Start shutting everything down if we get this.
                panic!("exec_start_post command failed");
            }
        }

        // Return the process (if this is a long-running daemon, otherwise
        // `None`, since there is nothing to monitor).
        Ok(Self { config, daemon })
    }

    pub fn is_daemon(&self) -> bool {
        self.daemon.is_some()
    }

    /// Wait for the process to exit (if it is a daemon); returns
    /// immediately if this is was a one-shot process.
    pub async fn wait(&mut self) -> ExitStatus {
        self.daemon
            .as_mut()
            .expect("Cannot wait on oneshot process")
            .wait()
            .await
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        tracing::event!(Level::INFO, name = %self.config.name(), "Stopping process");

        match &self.config.exec_stop {
            StopMechanism::Signal(signal) => {
                if let Some(command) = &self.daemon {
                    command
                        .kill(signal.into())
                        .await
                        .with_context(|| "Error sending stop signal to daemon")?;
                }
            }
            StopMechanism::Command(exec_stop) => {
                let mut cmd = Command::run(
                    self.config.name(),
                    None,
                    &exec_stop.program,
                    &exec_stop.args,
                    &self.config.pass_environment,
                )
                .with_context(|| "Error starting exec_stop command")?;

                match cmd.wait().await {
                    ExitStatus::Exited(0) => {}
                    ExitStatus::Exited(exit_code) => {
                        bail!("exec_stop process failed: {exit_code}");
                    }
                    ExitStatus::Killed => {
                        bail!("exec_stop process was killed");
                    }
                }
            }
        };

        Ok(())
    }
}
