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

use anyhow::Context;
use config::Config;
use tokio::sync::mpsc;

use crate::process::Process;

pub mod command;
pub mod config;
pub mod process;

/// Runs a Ground Control specification, returning only when all of the
/// processes have stopped (either because one process triggered a
/// shutdown, or because the provide shutdown signal was triggered).
pub async fn run(config: Config, mut shutdown: mpsc::UnboundedReceiver<()>) -> anyhow::Result<()> {
    // Create the shutdown channel, which will be used to initiate the
    // shutdown process, regardless of if this is a graceful shutdown
    // triggered by a shutdown signal, or an unexpected shutdown caused
    // by the failure of a daemon process.
    let (shutdown_sender, mut shutdown_receiver) = mpsc::unbounded_channel();

    // Start every process in the order they were found in the config
    // file.
    let mut processes: Vec<Process> = Default::default();
    for process_config in config.processes.into_iter() {
        // TODO: Need to start shutting down if this fails.
        let process = Process::start(process_config, shutdown_sender.clone())
            .await
            .with_context(|| "Unable to start process")?;
        processes.push(process);
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
        process_count = %processes.len(),
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

    while let Some(process) = processes.pop() {
        // TODO: We could do some sort of thing here where we check to
        // see if this is the process that triggered the shutdown and,
        // *still* `stop` it (since we may need to run `post`), but not
        // actually kill it, since it has already stopped. Basically,
        // just some extra tracking to avoid the WARN log that happens
        // when trying to kill a process that has already exited.
        if let Err(err) = process.stop().await {
            tracing::error!(?err, "Error stopping process");
        }
    }

    tracing::info!("All processes have exited.");

    Ok(())
}
