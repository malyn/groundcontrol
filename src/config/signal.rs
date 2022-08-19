//! Signal configuration.

use serde::Deserialize;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
pub enum SignalConfig {
    SIGINT,
    SIGQUIT,
    SIGTERM,
}

impl From<SignalConfig> for nix::sys::signal::Signal {
    fn from(signal: SignalConfig) -> Self {
        match signal {
            SignalConfig::SIGINT => Self::SIGINT,
            SignalConfig::SIGQUIT => Self::SIGQUIT,
            SignalConfig::SIGTERM => Self::SIGTERM,
        }
    }
}

impl From<&SignalConfig> for nix::sys::signal::Signal {
    fn from(signal: &SignalConfig) -> Self {
        match signal {
            SignalConfig::SIGINT => Self::SIGINT,
            SignalConfig::SIGQUIT => Self::SIGQUIT,
            SignalConfig::SIGTERM => Self::SIGTERM,
        }
    }
}
