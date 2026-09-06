use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RequestId(Uuid);

impl RequestId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Variants become runtime inputs when transport identity is wired in S3-06.
pub enum IdentityTrust {
    Trusted,
    Untrusted,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(dead_code)] // Sources are defined before their atomic transport-integration commit.
pub enum IdentitySource {
    ServerConfiguration,
    AuthenticatedTransport,
    McpInitialize,
    RequestMetadata,
    #[default]
    Unavailable,
}

impl IdentitySource {
    #[allow(dead_code)] // Used by authorization once identity propagation is complete.
    pub const fn trust(self) -> IdentityTrust {
        match self {
            Self::ServerConfiguration | Self::AuthenticatedTransport => IdentityTrust::Trusted,
            Self::McpInitialize | Self::RequestMetadata | Self::Unavailable => {
                IdentityTrust::Untrusted
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClientIdentity {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) source: IdentitySource,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AgentIdentity {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) source: IdentitySource,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ActorIdentity {
    pub(crate) subject: Option<String>,
    pub(crate) display_name: Option<String>,
    pub(crate) source: IdentitySource,
}

/// Identity and tracing metadata associated with one MCP tool request.
///
/// Fields are introduced incrementally as the request identity model evolves.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RequestContext {
    pub(crate) request_id: RequestId,
    pub(crate) client: ClientIdentity,
    pub(crate) agent: AgentIdentity,
    pub(crate) actor: ActorIdentity,
}

impl RequestContext {
    pub(crate) fn new() -> Self {
        Self {
            request_id: RequestId::new(),
            client: ClientIdentity::default(),
            agent: AgentIdentity::default(),
            actor: ActorIdentity::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_context_has_a_unique_request_id() {
        let first = RequestContext::new();
        let second = RequestContext::new();

        assert_ne!(first.request_id, second.request_id);
    }

    #[test]
    fn request_context_starts_with_unknown_client_and_agent() {
        let context = RequestContext::new();

        assert_eq!(context.client, ClientIdentity::default());
        assert_eq!(context.agent, AgentIdentity::default());
    }

    #[test]
    fn actor_identity_is_independent_from_agent_identity() {
        let context = RequestContext::new();

        assert_eq!(context.actor, ActorIdentity::default());
        assert_eq!(context.agent, AgentIdentity::default());
    }

    #[test]
    fn identity_sources_have_explicit_trust() {
        assert_eq!(
            IdentitySource::ServerConfiguration.trust(),
            IdentityTrust::Trusted
        );
        assert_eq!(
            IdentitySource::AuthenticatedTransport.trust(),
            IdentityTrust::Trusted
        );
        assert_eq!(
            IdentitySource::McpInitialize.trust(),
            IdentityTrust::Untrusted
        );
        assert_eq!(
            IdentitySource::RequestMetadata.trust(),
            IdentityTrust::Untrusted
        );
        assert_eq!(
            IdentitySource::Unavailable.trust(),
            IdentityTrust::Untrusted
        );
    }
}
