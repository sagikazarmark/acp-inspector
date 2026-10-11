//! Inspector-supplied MCP definitions for subsequent Session-opening calls.
//! The Agent owns execution; these are neither a catalog nor a connectivity report.
use crate::{CallError, v1};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct McpDraft {
    pub servers: Vec<McpServerDraft>,
}

/// Editor identities for a server and its rows. Never sent on the wire.
///
/// `0` means "not yet identified"; [`McpDraft::identify`] fills every gap with
/// a process-unique id and keeps the ones already given, so an editor can key
/// rows on something that survives reordering and removal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct McpServerDraft {
    pub id: u64,
    pub name: String,
    pub transport: McpTransport,
    pub command: String,
    pub args: Vec<String>,
    /// Identities for `args`, index for index.
    pub arg_ids: Vec<u64>,
    pub env: Vec<McpEnv>,
    /// Identities for `env`, index for index.
    pub env_ids: Vec<u64>,
    pub url: String,
    pub headers: Vec<McpEnv>,
    /// Identities for `headers`, index for index.
    pub header_ids: Vec<u64>,
}

static NEXT_ROW: AtomicU64 = AtomicU64::new(1);

fn fresh() -> u64 {
    NEXT_ROW.fetch_add(1, Ordering::Relaxed)
}

/// Makes `ids` exactly `len` long, keeping the identities it already holds.
fn fit(ids: &mut Vec<u64>, len: usize) {
    ids.truncate(len);
    for id in ids.iter_mut().filter(|id| **id == 0) {
        *id = fresh();
    }
    while ids.len() < len {
        ids.push(fresh());
    }
}

impl McpServerDraft {
    /// Gives the server and each of its rows an identity, keeping existing ones.
    pub fn identify(&mut self) {
        if self.id == 0 {
            self.id = fresh();
        }
        fit(&mut self.arg_ids, self.args.len());
        fit(&mut self.env_ids, self.env.len());
        fit(&mut self.header_ids, self.headers.len());
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

impl McpTransport {
    pub fn token(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
    pub fn advertised(self, capabilities: &v1::McpCapabilities) -> bool {
        match self {
            Self::Stdio => true,
            Self::Http => capabilities.http,
            Self::Sse => capabilities.sse,
        }
    }
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
    /// Gives every server and row an editor identity, keeping existing ones.
    pub fn identify(&mut self) {
        for server in &mut self.servers {
            server.identify();
        }
    }

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
            if definition.transport == McpTransport::Stdio
                && !Path::new(&definition.command).is_absolute()
            {
                findings.push(McpFinding {
                    server,
                    field: "command",
                    message: "Enter an absolute executable path.",
                });
            }
            if definition.transport != McpTransport::Stdio && !valid_url(&definition.url) {
                findings.push(McpFinding {
                    server,
                    field: "url",
                    message: "Enter an absolute HTTP or HTTPS URL with a host.",
                });
            }
        }
        findings
    }

    /// None means initialize has not answered yet, so drafts may name any transport.
    pub fn findings_for(&self, capabilities: Option<&v1::McpCapabilities>) -> Vec<McpFinding> {
        let mut findings = self.findings();
        if let Some(capabilities) = capabilities {
            for (server, definition) in self.servers.iter().enumerate() {
                if !definition.transport.advertised(capabilities) {
                    findings.push(McpFinding {
                        server,
                        field: "transport",
                        message: match definition.transport {
                            McpTransport::Http => {
                                "The Agent did not advertise HTTP MCP definitions."
                            }
                            McpTransport::Sse => "The Agent did not advertise SSE MCP definitions.",
                            McpTransport::Stdio => unreachable!(),
                        },
                    });
                }
            }
        }
        findings
    }

    pub fn definitions_for(
        &self,
        capabilities: &v1::McpCapabilities,
    ) -> Result<Vec<v1::McpServer>, CallError> {
        reject_first(&self.findings_for(Some(capabilities)))?;
        self.definitions()
    }

    pub fn definitions(&self) -> Result<Vec<v1::McpServer>, CallError> {
        reject_first(&self.findings())?;
        Ok(self
            .servers
            .iter()
            .map(|server| {
                let headers = || {
                    server
                        .headers
                        .iter()
                        .map(|pair| v1::HttpHeader::new(pair.name.clone(), pair.value.clone()))
                        .collect()
                };
                match server.transport {
                    McpTransport::Http => v1::McpServer::Http(
                        v1::McpServerHttp::new(server.name.clone(), server.url.clone())
                            .headers(headers()),
                    ),
                    McpTransport::Sse => v1::McpServer::Sse(
                        v1::McpServerSse::new(server.name.clone(), server.url.clone())
                            .headers(headers()),
                    ),
                    McpTransport::Stdio => v1::McpServer::Stdio(
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
                    ),
                }
            })
            .collect())
    }
}

fn reject_first(findings: &[McpFinding]) -> Result<(), CallError> {
    if let Some(finding) = findings.first() {
        return Err(CallError::InvalidMcp(format!(
            "MCP server {} {}: {}",
            finding.server + 1,
            finding.field,
            finding.message
        )));
    }
    Ok(())
}

fn valid_url(text: &str) -> bool {
    // Validate without normalizing the user's wire value or resolving its host.
    !text.chars().any(|c| c.is_whitespace() || c.is_control())
        && (text.to_ascii_lowercase().starts_with("http://")
            || text.to_ascii_lowercase().starts_with("https://"))
        && !text.contains('\\')
        && url::Url::parse(text)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_definitions_preserve_headers_and_validate_each_advertisement() {
        let mut draft = McpDraft {
            servers: vec![McpServerDraft {
                name: "remote".into(),
                transport: McpTransport::Http,
                url: "https://example.test/MCP?key=a%2Fb".into(),
                headers: vec![
                    McpEnv {
                        name: "X-Test".into(),
                        value: "".into(),
                    },
                    McpEnv {
                        name: "X-Test".into(),
                        value: " two ".into(),
                    },
                ],
                ..Default::default()
            }],
        };
        assert!(draft.findings_for(None).is_empty());
        assert_eq!(
            draft.findings_for(Some(&v1::McpCapabilities::default()))[0].field,
            "transport"
        );
        let caps = v1::McpCapabilities::new().http(true);
        let definitions = draft.definitions_for(&caps).unwrap();
        let v1::McpServer::Http(server) = &definitions[0] else {
            panic!("http")
        };
        assert_eq!(server.url, "https://example.test/MCP?key=a%2Fb");
        assert_eq!(
            server
                .headers
                .iter()
                .map(|h| (&*h.name, &*h.value))
                .collect::<Vec<_>>(),
            vec![("X-Test", ""), ("X-Test", " two ")]
        );
        draft.servers[0].transport = McpTransport::Sse;
        assert!(draft.definitions_for(&caps).is_err());
        assert!(
            draft
                .definitions_for(&v1::McpCapabilities::new().sse(true))
                .is_ok()
        );
        for url in [
            "relative",
            "file:///tmp/mcp",
            "https:///",
            "https://a.test/ bad",
            "https://a.test\\path",
        ] {
            draft.servers[0].url = url.into();
            assert!(draft.definitions().is_err(), "{url}");
        }
    }
    #[test]
    fn definitions_preserve_order_empty_arguments_and_duplicate_environment_names() {
        let command = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let draft = McpDraft {
            servers: vec![McpServerDraft {
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
                ..Default::default()
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
    fn identities_fill_gaps_and_survive_edits_without_reaching_the_wire() {
        let mut draft = McpDraft {
            servers: vec![McpServerDraft {
                name: "tools".into(),
                command: "/bin/tools".into(),
                args: vec!["a".into(), "b".into()],
                ..Default::default()
            }],
        };
        let before = draft.definitions().unwrap();
        draft.identify();
        let server = &draft.servers[0];
        assert_ne!(server.id, 0);
        assert_eq!(server.arg_ids.len(), 2);
        assert_ne!(server.arg_ids[0], server.arg_ids[1]);
        let kept = server.clone();

        // Identifying again keeps what was given and fills only what is new.
        draft.servers[0].args.push("c".into());
        draft.identify();
        assert_eq!(draft.servers[0].id, kept.id);
        assert_eq!(draft.servers[0].arg_ids[..2], kept.arg_ids[..]);
        assert_eq!(draft.servers[0].arg_ids.len(), 3);

        // A row removed takes its identity with it.
        draft.servers[0].args.truncate(1);
        draft.identify();
        assert_eq!(draft.servers[0].arg_ids, kept.arg_ids[..1]);

        // And none of it is part of the definition sent to the agent.
        draft.servers[0].args = vec!["a".into(), "b".into()];
        draft.identify();
        assert_eq!(draft.definitions().unwrap(), before);
    }

    #[test]
    fn validation_names_fields_but_never_checks_the_agents_filesystem() {
        let mut draft = McpDraft {
            servers: vec![McpServerDraft::default()],
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
