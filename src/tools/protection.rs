#[derive(Clone, Copy, Debug)]
pub(crate) struct QueryLimits {
    pub(crate) max_query_limit: u64,
    pub(crate) max_requested_fields: usize,
    pub(crate) max_read_ids: usize,
    pub(crate) max_domain_depth: usize,
    pub(crate) max_domain_terms: usize,
}
