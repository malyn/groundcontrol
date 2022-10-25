//! Process manager designed for container-*like* environments that need
//! to run multiple processes, with basic dependency relationships and
//! pre/post execution commands.

#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    missing_docs,
    unreachable_pub,
    missing_copy_implementations,
    unused_qualifications,
    clippy::unwrap_in_result,
    clippy::unwrap_used
)]

use async_trait::async_trait;
use tokio::sync::mpsc;

pub mod command;
pub mod config;
pub mod process;

/// Errors generated when starting processes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StartProcessError {
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

/// Starts processes.
#[cfg_attr(feature = "_mocks", mockall::automock)]
#[async_trait]
pub trait StartProcess<MP>: Send + Sync
where
    MP: ManageProcess,
{
    /// Starts the process and returns a handle to the process.
    async fn start_process(
        self,
        process_stopped: mpsc::UnboundedSender<()>,
    ) -> Result<MP, StartProcessError>;
}

/// Errors generated when stopping processes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StopProcessError {
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

/// Manages started processes.
#[cfg_attr(feature = "_mocks", mockall::automock)]
#[async_trait]
pub trait ManageProcess: Send + Sync {
    /// Stops the process: executes the `stop` command/signal if this is
    /// a daemon process; waits for the process to exit; runs the `post`
    /// command (if present).
    async fn stop_process(self) -> Result<(), StopProcessError>;
}

/// Runs a Ground Control specification, returning only when all of the
/// processes have stopped (either because one process triggered a
/// shutdown, or because the provide shutdown signal was triggered).
pub async fn run<SP, MP>(
    spec: Vec<SP>,
    mut shutdown: mpsc::UnboundedReceiver<()>,
) -> Result<(), StartProcessError>
where
    SP: StartProcess<MP>,
    MP: ManageProcess,
{
    // Create the shutdown channel, which will be used to initiate the
    // shutdown process, regardless of if this is a graceful shutdown
    // triggered by a shutdown signal, or an unexpected shutdown caused
    // by the failure of a daemon process.
    let (shutdown_sender, mut shutdown_receiver) = mpsc::unbounded_channel();

    // Start every process in the order they were found in the config
    // file.
    let mut running: Vec<MP> = Vec::with_capacity(spec.len());
    for sp in spec.into_iter() {
        let process = match sp.start_process(shutdown_sender.clone()).await {
            Ok(process) => process,
            Err(err) => {
                tracing::error!(?err, "Failed to start process; aborting startup procedure");

                // TODO: Need to start shutting down if this fails.
                // Right now we just exit, but we may have already
                // started processes and we need to shut down those
                // processes (or they will block Ground Control from
                // exiting and thus the container from shutting down).
                return Err(err);
            }
        };

        running.push(process);
    }

    // Convert an external shutdown signal into a shutdown message.
    let external_shutdown_sender = shutdown_sender.clone();
    tokio::spawn(async move {
        // Both sending the shutdown signal, *and dropping the sender,*
        // trigger a shutdown.
        let _ = shutdown.recv().await;
        let _ = external_shutdown_sender.send(());
    });

    tracing::info!(
        process_count = %running.len(),
        "Startup phase completed; waiting for shutdown signal or any process to exit."
    );

    shutdown_receiver
        .recv()
        .await
        .expect("All shutdown senders closed without sending a shutdown signal.");

    // Either one process exited or we received a stop signal; stop all
    // of the processes in the *reverse* order in which they were
    // started.
    tracing::info!("Completion signal triggered; shutting down all processes");

    while let Some(process) = running.pop() {
        // TODO: We could do some sort of thing here where we check to
        // see if this is the process that triggered the shutdown and,
        // *still* `stop` it (since we may need to run `post`), but not
        // actually kill it, since it has already stopped. Basically,
        // just some extra tracking to avoid the WARN log that happens
        // when trying to kill a process that has already exited.
        if let Err(err) = process.stop_process().await {
            tracing::error!(?err, "Error stopping process");
        }
    }

    tracing::info!("All processes have exited.");

    Ok(())
}
