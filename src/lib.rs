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

use std::collections::HashSet;

use serde::Deserialize;

pub mod command;
pub mod process;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessType {
    Daemon,
    Oneshot,
}

impl Default for ProcessType {
    fn default() -> Self {
        Self::Daemon
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessConfig {
    name: Option<String>,

    #[serde(rename = "type", default)]
    pub process_type: ProcessType,

    #[serde(default)]
    pub pass_environment: HashSet<String>,

    #[serde(default)]
    pub user: Option<String>,

    #[serde(default)]
    pub exec_start_pre: Option<Vec<String>>,

    pub exec_start: Vec<String>,

    #[serde(default)]
    pub exec_start_post: Option<Vec<String>>,

    #[serde(default)]
    pub exec_stop: Option<Vec<String>>,
}

impl ProcessConfig {
    pub fn name(&self) -> &str {
        if let Some(name) = &self.name {
            name
        } else {
            self.exec_start[0]
                .split('/')
                .last()
                .unwrap_or_else(|| &self.exec_start[0])
        }
    }
}
