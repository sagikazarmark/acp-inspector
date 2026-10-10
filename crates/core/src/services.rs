//! What this client claims in `initialize`, chosen at launch
//! (`docs/client-services.md`): which client services it answers, how the
//! agent's other requests are answered, and the protocol version it asks for.
//!
//! Claimed means answered, never performed: a claimed `fs/*` or `terminal/*`
//! call is answered by the reader or by a rules file; nothing reads a file,
//! writes one or runs a command.

use agent_client_protocol_schema::{ProtocolVersion, v1};

use crate::script::ServiceScript;

/// The launch-time choices behind the `initialize` request.
///
/// The default is the claim this tool always made: no filesystem, no terminal,
/// unserviced requests answered method-not-found, and protocol version 1.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientServices {
    /// Claim `fs.readTextFile`, and answer `fs/read_text_file`.
    pub read_text_file: bool,
    /// Claim `fs.writeTextFile`, and answer `fs/write_text_file`.
    pub write_text_file: bool,
    /// Claim `terminal`, and answer every `terminal/*` method.
    pub terminal: bool,
    /// Answer by hand any other request this client does not service, instead
    /// of method-not-found. Claims nothing.
    pub answer_unserviced: bool,
    /// Canned answers, tried before anything is shown to the reader.
    pub script: Option<ServiceScript>,
    /// The `protocolVersion` sent in `initialize`.
    pub protocol_version: u16,
}

impl Default for ClientServices {
    fn default() -> Self {
        Self {
            read_text_file: false,
            write_text_file: false,
            terminal: false,
            answer_unserviced: false,
            script: None,
            protocol_version: ProtocolVersion::V1.as_u16(),
        }
    }
}

/// The `terminal/*` methods one `terminal` claim invites.
pub(crate) fn is_terminal(method: &str) -> bool {
    let names = &v1::CLIENT_METHOD_NAMES;
    [
        names.terminal_create,
        names.terminal_output,
        names.terminal_release,
        names.terminal_wait_for_exit,
        names.terminal_kill,
    ]
    .contains(&method)
}

impl ClientServices {
    /// What `initialize` claims about this client.
    ///
    /// The fixed part is what this tool renders or performs whatever was
    /// chosen: boolean config options, session compaction and notices (which
    /// the timeline draws), and both elicitation modes (which it answers). The
    /// chosen part is the filesystem and terminal services.
    pub fn capabilities(&self) -> v1::ClientCapabilities {
        v1::ClientCapabilities::new()
            .fs(v1::FileSystemCapabilities::new()
                .read_text_file(self.read_text_file)
                .write_text_file(self.write_text_file))
            .terminal(self.terminal)
            .session(
                v1::ClientSessionCapabilities::new()
                    .compaction(v1::CompactionCapabilities::new())
                    .config_options(
                        v1::SessionConfigOptionsCapabilities::new()
                            .boolean(v1::BooleanConfigOptionCapabilities::new()),
                    )
                    .notices(v1::NoticeCapabilities::new()),
            )
            .elicitation(
                v1::ElicitationCapabilities::new()
                    .form(v1::ElicitationFormCapabilities::new())
                    .url(v1::ElicitationUrlCapabilities::new()),
            )
    }

    /// The protocol version `initialize` asks for.
    pub fn protocol(&self) -> ProtocolVersion {
        ProtocolVersion::from(self.protocol_version)
    }

    /// Whether this client claimed the service a method belongs to.
    pub fn claims(&self, method: &str) -> bool {
        let names = &v1::CLIENT_METHOD_NAMES;
        if method == names.fs_read_text_file {
            self.read_text_file
        } else if method == names.fs_write_text_file {
            self.write_text_file
        } else {
            self.terminal && is_terminal(method)
        }
    }
}

/// What one connection's `initialize` claimed, as it went out.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientClaim {
    pub capabilities: v1::ClientCapabilities,
    pub protocol_version: ProtocolVersion,
}

impl Default for ClientClaim {
    fn default() -> Self {
        let services = ClientServices::default();
        Self {
            capabilities: services.capabilities(),
            protocol_version: services.protocol(),
        }
    }
}
