//! Process configuration.

use std::collections::HashSet;

use serde::Deserialize;

use super::{command::CommandConfig, signal::SignalConfig};

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
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProcessConfig {
    name: Option<String>,

    #[serde(rename = "type", default)]
    pub process_type: ProcessType,

    #[serde(default)]
    pub env_filter: HashSet<String>,

    #[serde(default)]
    pub user: Option<String>,

    #[serde(default)]
    pub pre_start: Option<CommandConfig>,

    pub start: CommandConfig,

    #[serde(default)]
    pub post_start: Option<CommandConfig>,

    #[serde(default)]
    pub stop: StopMechanism,
}

impl ProcessConfig {
    pub fn name(&self) -> &str {
        if let Some(name) = &self.name {
            name
        } else {
            self.start
                .program
                .split('/')
                .last()
                .unwrap_or(&self.start.program)
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(untagged)]
pub enum StopMechanism {
    Signal(SignalConfig),

    Command(CommandConfig),
}

impl Default for StopMechanism {
    fn default() -> Self {
        StopMechanism::Signal(SignalConfig::SIGTERM)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::config::signal::SignalConfig;

    use super::StopMechanism;

    #[derive(Debug, Deserialize, PartialEq)]
    struct StopMechanismTest {
        stop: StopMechanism,
    }

    #[test]
    fn supports_signal_names_in_stop() {
        let toml = r#"stop = "SIGTERM""#;
        let decoded: StopMechanismTest = toml::from_str(toml).expect("Failed to parse test TOML");
        assert_eq!(StopMechanism::Signal(SignalConfig::SIGTERM), decoded.stop);
    }
}
