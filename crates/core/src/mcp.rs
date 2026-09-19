//! Inspector-supplied stdio MCP definitions for subsequent Session-opening calls.
//! The Agent owns execution; these are neither a catalog nor a connectivity report.
use crate::{CallError, v1};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct McpDraft {
    pub servers: Vec<StdioMcp>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StdioMcp {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<McpEnv>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct McpEnv {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpFinding {
    pub server: usize,
    pub field: &'static str,
    pub message: &'static str,
}

impl McpDraft {
    pub fn findings(&self) -> Vec<McpFinding> {
        let mut findings = Vec::new();
        for (server, definition) in self.servers.iter().enumerate() {
            if definition.name.trim().is_empty() {
                findings.push(McpFinding {
                    server,
                    field: "name",
                    message: "Enter a nonblank server name.",
                });
            }
            if !Path::new(&definition.command).is_absolute() {
                findings.push(McpFinding {
                    server,
                    field: "command",
                    message: "Enter an absolute executable path.",
                });
            }
        }
        findings
    }

    pub fn definitions(&self) -> Result<Vec<v1::McpServer>, CallError> {
        if let Some(finding) = self.findings().first() {
            return Err(CallError::InvalidMcp(format!(
                "MCP server {} {}: {}",
                finding.server + 1,
                finding.field,
                finding.message
            )));
        }
        Ok(self
            .servers
            .iter()
            .map(|server| {
                v1::McpServer::Stdio(
                    v1::McpServerStdio::new(server.name.clone(), server.command.clone())
                        .args(server.args.clone())
                        .env(
                            server
                                .env
                                .iter()
                                .map(|pair| {
                                    v1::EnvVariable::new(pair.name.clone(), pair.value.clone())
                                })
                                .collect(),
                        ),
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definitions_preserve_order_empty_arguments_and_duplicate_environment_names() {
        let command = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let draft = McpDraft {
            servers: vec![StdioMcp {
                name: " name ".into(),
                command: command.clone(),
                args: vec!["".into(), "  two words  ".into()],
                env: vec![
                    McpEnv {
                        name: "X".into(),
                        value: "".into(),
                    },
                    McpEnv {
                        name: "X".into(),
                        value: " a=b ".into(),
                    },
                ],
            }],
        };
        let definitions = draft.definitions().unwrap();
        let v1::McpServer::Stdio(server) = &definitions[0] else {
            panic!("stdio")
        };
        assert_eq!(server.name, " name ");
        assert_eq!(server.command, std::path::PathBuf::from(command));
        assert_eq!(server.args, vec!["", "  two words  "]);
        assert_eq!(
            server
                .env
                .iter()
                .map(|v| (v.name.as_str(), v.value.as_str()))
                .collect::<Vec<_>>(),
            vec![("X", ""), ("X", " a=b ")]
        );
    }
    #[test]
    fn validation_names_fields_but_never_checks_the_agents_filesystem() {
        let mut draft = McpDraft {
            servers: vec![StdioMcp::default()],
        };
        assert_eq!(draft.findings().len(), 2);
        assert!(draft.definitions().is_err());
        draft.servers[0].name = "tools".into();
        draft.servers[0].command = std::env::temp_dir()
            .join("missing-mcp-executable")
            .to_string_lossy()
            .into_owned();
        assert!(draft.definitions().is_ok());
        draft.servers[0].command = "relative".into();
        assert_eq!(draft.findings()[0].field, "command");
        assert_eq!(McpDraft::default().definitions().unwrap(), vec![]);
    }
}
