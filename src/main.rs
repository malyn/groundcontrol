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
use clap::Parser;
use groundcontrol::{config::Config, process::Process};
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::mpsc,
};
use tracing::Level;

#[derive(Parser)]
#[clap(about, long_about = None)]
struct Cli {
    /// Check the configuration file for errors, but do not start any
    /// processes.
    #[clap(long)]
    check: bool,

    config_file: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Crash the process on a panic anywhere (including in a background
    // Tokio task, since we want panic to mean "something is very wrong;
    // stop everything").
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Server panicked: {info}");
        std::process::abort();
    }));

    // Set the RUST_LOG, if it hasn't been explicitly defined
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info")
    }

    // Create our tracing subscriber, manually bringing in EnvFilter so
    // that we can specify a custom format *and still get environment
    // variable-based filtering.* See this GitHub issue for the
    // difference between `tracing_subscriber::fmt::init()` and
    // `tracing_subscriber::fmt().init()` (the latter does *not*
    // automatically bring in EnvFilter, for example):
    // <https://github.com/tokio-rs/tracing/issues/1329#issuecomment-808682793>
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stdout)
        .init();

    // Parse the command line arguments.
    let cli = Cli::parse();

    // Read and parse the config file.
    let config_file = tokio::fs::read_to_string(cli.config_file)
        .await
        .expect("Unable to read config file");
    let config: Config = toml::from_str(&config_file).expect("Error parsing config file");

    // We're done if this was only a config file check.
    if cli.check {
        return Ok(());
    }

    // Create the shutdown channel, which will be used to initiate the
    // shutdown process, regardless of if this is a graceful shutdown
    // triggered by a UNIX signal, or an unexpected shutdown caused by
    // the failure of a daemon process.
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

    // Convert signals into shutdown messages.
    let sigint_shutdown_sender = shutdown_sender.clone();
    tokio::spawn(async move {
        signal(SignalKind::interrupt())
            .expect("Failed to register SIGINT handler")
            .recv()
            .await;
        let _ = sigint_shutdown_sender.send(());
    });

    let sigterm_shutdown_sender = shutdown_sender.clone();
    tokio::spawn(async move {
        signal(SignalKind::terminate())
            .expect("Failed to register SIGTERM handler")
            .recv()
            .await;
        let _ = sigterm_shutdown_sender.send(());
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

    tracing::event!(Level::INFO, "All processes have exited.");

    Ok(())
}
