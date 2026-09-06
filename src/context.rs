use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RequestId(Uuid);

impl RequestId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClientIdentity {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AgentIdentity {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
}

/// Identity and tracing metadata associated with one MCP tool request.
///
/// Fields are introduced incrementally as the request identity model evolves.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RequestContext {
    pub(crate) request_id: RequestId,
    pub(crate) client: ClientIdentity,
    pub(crate) agent: AgentIdentity,
}

impl RequestContext {
    pub(crate) fn new() -> Self {
        Self {
            request_id: RequestId::new(),
            client: ClientIdentity::default(),
            agent: AgentIdentity::default(),
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
}
