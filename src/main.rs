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
use groundcontrol::config::Config;
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::mpsc,
};

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
        eprintln!("Process panicked: {info}");
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
    // TODO: We don't actually need this; this was only required back when we supported text *or* JSON.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stdout)
        .init();

    // Parse the command line arguments.
    let cli = Cli::parse();

    // Read and parse the config file.
    let config_file = tokio::fs::read_to_string(cli.config_file)
        .await
        .with_context(|| "Unable to read config file")?;
    let config: Config =
        toml::from_str(&config_file).with_context(|| "Error parsing config file")?;

    // We're done if this was only a config file check.
    if cli.check {
        return Ok(());
    }

    // Create the external shutdown signal (used to shut down Ground
    // Control on UNIX signals).
    let (shutdown_sender, mut shutdown_receiver) = mpsc::unbounded_channel();

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

    // Run the Ground Control specification, *unless* we are in
    // break-glass mode, in which case we freeze startup and just wait
    // for the shutdown signal. (this gives the admin a chance to SSH
    // into a machine that is in a startup-crash loop, perhaps due to an
    // issue on an attached, persistent storage volume)
    if std::env::var_os("BREAK_GLASS").is_none() {
        groundcontrol::run(config, shutdown_receiver).await
    } else {
        tracing::info!("BREAK GLASS MODE: no processes will be started");

        shutdown_receiver
            .recv()
            .await
            .expect("All shutdown senders closed without sending a shutdown signal.");

        tracing::info!(
            "Shutdown signal triggered (make sure to clear the `BREAK_GLASS` environment variable)"
        );

        Ok(())
    }
}
