use serde::Deserialize;

use self::process::ProcessConfig;

pub mod command;
pub mod process;
pub mod signal;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub processes: Vec<ProcessConfig>,
}
