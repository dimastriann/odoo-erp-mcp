/// Identity and tracing metadata associated with one MCP tool request.
///
/// Fields are introduced incrementally as the request identity model evolves.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RequestContext {
    _private: (),
}

impl RequestContext {
    pub(crate) const fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_context_is_a_typed_request_boundary() {
        let context = RequestContext::new();

        assert_eq!(context, context.clone());
    }
}
