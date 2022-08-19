//! Process manager designed for container-*like* environments that need
//! to run multiple processes, with basic dependency relationships and
//! pre/post execution commands.

#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    // missing_docs,
    unreachable_pub,
    missing_copy_implementations,
    unused_qualifications,
    clippy::unwrap_in_result,
    clippy::unwrap_used
)]

pub mod command;
pub mod config;
pub mod process;
