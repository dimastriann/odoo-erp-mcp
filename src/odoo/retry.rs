use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationClass {
    Authentication,
    ReadOnly,
    Mutation,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetryBackoff {
    base_delay: Duration,
    max_delay: Duration,
}

#[allow(dead_code)]
impl RetryBackoff {
    pub(crate) const fn new(base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            base_delay,
            max_delay,
        }
    }

    pub(crate) fn delay_for(self, retry_index: u32, entropy: u64) -> Duration {
        let multiplier = 1_u32.checked_shl(retry_index.min(31)).unwrap_or(u32::MAX);
        let capped = self
            .base_delay
            .saturating_mul(multiplier)
            .min(self.max_delay);
        let capped_millis = u64::try_from(capped.as_millis()).unwrap_or(u64::MAX);
        let lower_bound = capped_millis / 2;
        let jitter_window = capped_millis.saturating_sub(lower_bound);
        let mixed_entropy = entropy
            .wrapping_add(u64::from(retry_index))
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let jitter = mixed_entropy % jitter_window.saturating_add(1);

        Duration::from_millis(lower_bound.saturating_add(jitter))
    }
}

impl Default for RetryBackoff {
    fn default() -> Self {
        Self::new(Duration::from_millis(100), Duration::from_secs(2))
    }
}

impl OperationClass {
    pub(crate) const fn is_retry_safe(self) -> bool {
        matches!(self, Self::ReadOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_read_only_operations_are_retry_safe() {
        assert!(OperationClass::ReadOnly.is_retry_safe());
        assert!(!OperationClass::Authentication.is_retry_safe());
        assert!(!OperationClass::Mutation.is_retry_safe());
    }

    #[test]
    fn backoff_grows_exponentially_until_capped() {
        let backoff = RetryBackoff::new(Duration::from_millis(100), Duration::from_millis(400));

        let first = backoff.delay_for(0, 7);
        let second = backoff.delay_for(1, 7);
        let capped = backoff.delay_for(10, 7);

        assert!((Duration::from_millis(50)..=Duration::from_millis(100)).contains(&first));
        assert!((Duration::from_millis(100)..=Duration::from_millis(200)).contains(&second));
        assert!((Duration::from_millis(200)..=Duration::from_millis(400)).contains(&capped));
    }

    #[test]
    fn backoff_jitter_varies_with_entropy() {
        let backoff = RetryBackoff::default();

        assert_ne!(backoff.delay_for(1, 11), backoff.delay_for(1, 12));
    }
}
