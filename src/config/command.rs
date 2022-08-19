//! Command configuration

use serde::Deserialize;

/// Configuration for a command, its arguments, and any execution
/// properties (such as the user under which to run the command).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(from = "SimpleOrDetailedCommandConfig")]
pub struct CommandConfig {
    /// User to run this command as, otherwise run the command as the
    /// user that executed Ground Control (most likely `root`).
    pub user: Option<String>,

    /// Program to execute.
    pub program: String,

    /// Arguments to pass to the program.
    pub args: Vec<String>,
}

#[derive(Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(untagged)]
enum SimpleOrDetailedCommandConfig {
    SimpleCommand(SimpleCommandConfig),

    DetailedCommand(DetailedCommandConfig),
}

impl From<SimpleOrDetailedCommandConfig> for CommandConfig {
    fn from(config: SimpleOrDetailedCommandConfig) -> Self {
        match config {
            SimpleOrDetailedCommandConfig::SimpleCommand(config) => config.into(),
            SimpleOrDetailedCommandConfig::DetailedCommand(config) => Self {
                user: config.user,
                ..config.command.into()
            },
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(untagged)]
enum SimpleCommandConfig {
    CommandString(String),

    CommandVector(Vec<String>),
}

#[derive(Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
struct DetailedCommandConfig {
    #[serde(default)]
    user: Option<String>,

    command: SimpleCommandConfig,
}

impl From<SimpleCommandConfig> for CommandConfig {
    fn from(config: SimpleCommandConfig) -> Self {
        let command_vec = match config {
            SimpleCommandConfig::CommandString(line) => line
                .split(' ')
                .map(|s| s.to_owned())
                .collect::<Vec<String>>(),
            SimpleCommandConfig::CommandVector(v) => v,
        };

        Self {
            user: None,
            program: command_vec[0].clone(),
            args: command_vec[1..].to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::config::command::CommandConfig;

    #[derive(Debug, Deserialize, PartialEq)]
    struct CommandConfigTest {
        run: CommandConfig,
    }

    #[test]
    fn supports_whitespace_separated_command_lines() {
        let toml = r#"run = "/app/run-me.sh using these args""#;
        let decoded: CommandConfigTest = toml::from_str(toml).expect("Failed to parse test TOML");
        assert_eq!(
            CommandConfig {
                user: None,
                program: String::from("/app/run-me.sh"),
                args: vec![
                    String::from("using"),
                    String::from("these"),
                    String::from("args"),
                ]
            },
            decoded.run
        );
    }

    #[test]
    fn supports_command_vectors() {
        let toml = r#"run = ["/app/run-me.sh", "using", "these", "args"]"#;
        let decoded: CommandConfigTest = toml::from_str(toml).expect("Failed to parse test TOML");
        assert_eq!(
            CommandConfig {
                user: None,
                program: String::from("/app/run-me.sh"),
                args: vec![
                    String::from("using"),
                    String::from("these"),
                    String::from("args"),
                ]
            },
            decoded.run
        );
    }

    #[test]
    fn supports_detailed_whitespace_separated_command_lines() {
        let toml = r#"run = { command = "/app/run-me.sh using these args" }"#;
        let decoded: CommandConfigTest = toml::from_str(toml).expect("Failed to parse test TOML");
        assert_eq!(
            CommandConfig {
                user: None,
                program: String::from("/app/run-me.sh"),
                args: vec![
                    String::from("using"),
                    String::from("these"),
                    String::from("args"),
                ]
            },
            decoded.run
        );

        let toml = r#"run = { user = "app", command = "/app/run-me.sh using these args" }"#;
        let decoded: CommandConfigTest = toml::from_str(toml).expect("Failed to parse test TOML");
        assert_eq!(
            CommandConfig {
                user: Some(String::from("app")),
                program: String::from("/app/run-me.sh"),
                args: vec![
                    String::from("using"),
                    String::from("these"),
                    String::from("args"),
                ]
            },
            decoded.run
        );
    }

    #[test]
    fn supports_detailed_command_vectors() {
        let toml = r#"run = { command = ["/app/run-me.sh", "using", "these", "args"] }"#;
        let decoded: CommandConfigTest = toml::from_str(toml).expect("Failed to parse test TOML");
        assert_eq!(
            CommandConfig {
                user: None,
                program: String::from("/app/run-me.sh"),
                args: vec![
                    String::from("using"),
                    String::from("these"),
                    String::from("args"),
                ]
            },
            decoded.run
        );

        let toml =
            r#"run = { user = "app", command = ["/app/run-me.sh", "using", "these", "args"] }"#;
        let decoded: CommandConfigTest = toml::from_str(toml).expect("Failed to parse test TOML");
        assert_eq!(
            CommandConfig {
                user: Some(String::from("app")),
                program: String::from("/app/run-me.sh"),
                args: vec![
                    String::from("using"),
                    String::from("these"),
                    String::from("args"),
                ]
            },
            decoded.run
        );
    }

    #[test]
    fn requires_command_in_detailed_command() {
        let toml = r#"run = { }"#;
        let error = toml::from_str::<CommandConfigTest>(toml).unwrap_err();
        assert_eq!("data did not match any variant of untagged enum SimpleOrDetailedCommandConfig for key `run` at line 1 column 1", error.to_string(),);

        let toml = r#"run = { user = "app" }"#;
        let error = toml::from_str::<CommandConfigTest>(toml).unwrap_err();
        assert_eq!("data did not match any variant of untagged enum SimpleOrDetailedCommandConfig for key `run` at line 1 column 1", error.to_string(),);
    }
}
