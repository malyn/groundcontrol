//! Runs commands and monitors their completion.

use std::{env, process::Stdio};

use anyhow::Context;
use command_group::{AsyncCommandGroup, AsyncGroupChild};
use nix::unistd::Pid;
use regex::{Captures, Regex};
use tokio::sync::watch;

use crate::config::command::CommandConfig;

/// Exit status returned by a command.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ExitStatus {
    /// Command exited with the given exit code.
    Exited(i32),

    /// Command was killed before it could exit.
    Killed,
}

#[derive(Debug)]
pub struct Command {
    exited: watch::Receiver<Option<ExitStatus>>,
    pid: Pid,
}

impl Command {
    pub fn run(name: &str, config: &CommandConfig) -> anyhow::Result<Self> {
        tracing::debug!(%name, ?config, "Running command");

        // Initialize the command.
        let mut command = tokio::process::Command::new(&config.program);

        // Add the arguments, and perform environment variable
        // substitution.
        command.args(
            config
                .args
                .iter()
                .map(substitute_env_var)
                .collect::<Vec<String>>(),
        );

        // Clear the environment, add back in `PATH`, then add any other
        // allowed environment variables.
        command.env_clear();

        if let Ok(path) = env::var("PATH") {
            command.env("PATH", path);
        }

        for key in &config.env_vars {
            command.env(
                &key,
                env::var(&key).with_context(|| "Missing environment variable")?,
            );
        }

        // Set the uid and gid if provided.
        if let Some(username) = &config.user {
            let user = users::get_user_by_name(username).with_context(|| "Unknown username")?;
            command.uid(user.uid()).gid(user.primary_group_id());
        };

        // Disable stdin, and map stdout and stderr to our own stdout
        // and stderr.
        command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Run the command.
        let child = command
            .group_spawn()
            .with_context(|| "Error running command")?;
        let pid = nix::unistd::Pid::from_raw(
            child
                .id()
                .with_context(|| "Unable to get PID of just-started process")? as i32,
        );

        tracing::debug!(%name, %pid, "Command running");

        // Listen for the command to complete.
        let (sender, receiver) = watch::channel(None);
        monitor_process(name.to_owned(), pid, child, sender);

        // Return the handle to our Command.
        Ok(Self {
            exited: receiver,
            pid,
        })
    }

    /// Sends a signal to the process.
    pub async fn kill(&self, signal: nix::sys::signal::Signal) -> anyhow::Result<()> {
        nix::sys::signal::kill(self.pid, signal)?;
        Ok(())
    }

    /// Waits for the process to exit.
    pub async fn wait(&mut self) -> ExitStatus {
        loop {
            // Wait for a new value.
            self.exited
                .changed()
                .await
                .unwrap_or_else(|_| panic!("Sender dropped for PID {}", self.pid));

            // Return the value immediately if we have one, otherwise
            // ignore the initial `None` value that will still be
            // present if the process has not yet stopped.
            if let Some(exit_status) = *self.exited.borrow_and_update() {
                tracing::debug!(pid = %self.pid, "Command exited");
                return exit_status;
            }
        }
    }
}

fn substitute_env_var(s: impl AsRef<str>) -> String {
    Regex::new(r"\{\{([A-Za-z0-9_]+)\}\}")
        .expect("Failed to compile regular expression")
        .replace_all(s.as_ref(), |caps: &Captures| {
            std::env::var(&caps[1]).expect("Unable to find environment variable")
        })
        .into_owned()
}

fn monitor_process(
    name: String,
    pid: Pid,
    mut child: AsyncGroupChild,
    sender: watch::Sender<Option<ExitStatus>>,
) {
    tokio::spawn(async move {
        match child.wait().await {
            Err(err) => {
                tracing::error!(%name, ?err, "Error waiting for command to exit");
                let _ = sender.send(Some(ExitStatus::Killed));
            }
            Ok(exit_status) => match exit_status.code() {
                Some(exit_code) => {
                    if exit_code == 0 {
                        tracing::debug!(%name, %pid, "Command exited cleanly");
                    } else {
                        tracing::error!(%name, %pid, %exit_code, "Command exited with non-zero exit code");
                    }

                    let _ = sender.send(Some(ExitStatus::Exited(exit_code)));
                }
                None => {
                    tracing::debug!(%name, %pid, "Command was killed");
                    let _ = sender.send(Some(ExitStatus::Killed));
                }
            },
        }
    });
}
