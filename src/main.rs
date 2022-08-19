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
use futures_util::FutureExt;
use groundcontrol::{config::Config, process::Process};
use tokio::signal::unix::{signal, SignalKind};
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

    // Start every process in the order they were found in the config
    // file.
    let mut processes: Vec<Process> = Default::default();
    for process_config in config.processes.into_iter() {
        // TODO: Need to start shutting down if this fails.
        let process = Process::start(process_config)
            .await
            .with_context(|| "Unable to start process")?;
        processes.push(process);
    }

    // Wait for *any* of the daemon processes to exit or for SIGTERM or
    // SIGINT signals (which then begins the graceful shutdown process).
    let sigint_task = tokio::spawn(async move {
        signal(SignalKind::interrupt())
            .expect("Failed to register SIGINT handler")
            .recv()
            .await;
    })
    .map(|_| ());

    let sigterm_task = tokio::spawn(async move {
        signal(SignalKind::terminate())
            .expect("Failed to register SIGTERM handler")
            .recv()
            .await;
    })
    .map(|_| ());

    tracing::event!(
        Level::INFO,
        process_count = %processes.len(),
        "Startup phase completed; waiting for shutdown signal or process exit"
    );

    let signals = processes
        .iter_mut()
        .map(|p| {
            if p.is_daemon() {
                p.wait().map(|_| ()).boxed_local()
            } else {
                // Oneshot processes exit immediately, and so should not
                // trigger a shutdown. (but we need to put something
                // into the iterator, so we put a future that never
                // completes)
                futures_util::future::pending().boxed_local()
            }
        })
        .chain([sigint_task.boxed_local(), sigterm_task.boxed_local()]);

    let (_result, ready_index, rest) = futures_util::future::select_all(signals).await;
    drop(rest);

    if ready_index < processes.len() {
        processes.remove(ready_index);
    }

    // Either one process exited or we received a stop signal; stop all
    // of the processes in the *reverse* order in which they were
    // started.
    tracing::event!(
        Level::INFO,
        %ready_index,
        "Completion signal triggered; shutting down all processes"
    );

    for process in processes.iter_mut().rev() {
        if let Err(err) = process.stop().await {
            tracing::event!(Level::ERROR, ?err, "Error stopping process");
        }

        if process.is_daemon() {
            process.wait().await;
        }
    }

    tracing::event!(Level::INFO, "All processes have exited.");

    Ok(())
}
