use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RequestId(Uuid);

impl RequestId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Identity and tracing metadata associated with one MCP tool request.
///
/// Fields are introduced incrementally as the request identity model evolves.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RequestContext {
    pub(crate) request_id: RequestId,
}

impl RequestContext {
    pub(crate) fn new() -> Self {
        Self {
            request_id: RequestId::new(),
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
}
