use crate::tileset::error::TileError;
use std::future::Future;
use std::time::Duration;
use tracing::warn;

/// Upper bound on a single backoff sleep, so a large `max_attempts` cannot turn
/// one request into a multi-second stall.
const MAX_BACKOFF: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Total number of attempts, including the first one. `1` disables retries.
    pub max_attempts: u32,
    /// Delay before the second attempt; doubles for each further attempt.
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
        }
    }
}

impl RetryPolicy {
    /// Backoff before `attempt` (1-based), exponential with up to 50% jitter so
    /// concurrent misses on the same degraded upstream do not retry in lockstep.
    fn backoff(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(16);
        let scaled = self
            .base_delay
            .saturating_mul(2u32.saturating_pow(exponent))
            .min(MAX_BACKOFF);
        let jitter = 1.0 + rand::random::<f64>() * 0.5;
        scaled.mul_f64(jitter).min(MAX_BACKOFF)
    }
}

/// Runs `operation`, retrying while the error is transient and attempts remain.
///
/// At the ~1-2% independent error rate seen on the tile bucket, three attempts
/// take the user-visible failure rate from 1-2% to roughly 1 in 10^5.
pub async fn with_retry<F, Fut, T>(
    policy: RetryPolicy,
    key: &str,
    mut operation: F,
) -> Result<T, TileError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, TileError>>,
{
    let max_attempts = policy.max_attempts.max(1);
    let mut attempt = 1;

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) if err.is_retryable() && attempt < max_attempts => {
                let delay = policy.backoff(attempt);
                warn!(
                    key = key,
                    attempt,
                    max_attempts,
                    delay_ms = delay.as_millis() as u64,
                    error = %err,
                    "Transient tile fetch failure, retrying"
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn fast_policy(max_attempts: u32) -> RetryPolicy {
        RetryPolicy {
            max_attempts,
            base_delay: Duration::from_millis(1),
        }
    }

    #[tokio::test]
    async fn returns_first_success_without_retrying() {
        let calls = AtomicU32::new(0);
        let result: Result<u8, TileError> = with_retry(fast_policy(3), "key", || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(7) }
        })
        .await;

        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_transient_errors_until_one_succeeds() {
        let calls = AtomicU32::new(0);
        let result: Result<u8, TileError> = with_retry(fast_policy(3), "key", || {
            let attempt = calls.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if attempt < 3 {
                    Err(TileError::upstream("key", 503, "slow down"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts_and_returns_last_error() {
        let calls = AtomicU32::new(0);
        let result: Result<u8, TileError> = with_retry(fast_policy(3), "key", || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(TileError::upstream("key", 504, "gateway timeout")) }
        })
        .await;

        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let err = result.unwrap_err();
        assert!(err.to_string().contains("504"), "{}", err);
    }

    #[tokio::test]
    async fn does_not_retry_permanent_errors() {
        let calls = AtomicU32::new(0);
        let result: Result<u8, TileError> = with_retry(fast_policy(5), "key", || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(TileError::upstream("key", 404, "NoSuchKey")) }
        })
        .await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn max_attempts_of_zero_still_runs_once() {
        let calls = AtomicU32::new(0);
        let result: Result<u8, TileError> = with_retry(fast_policy(0), "key", || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(TileError::transport("key", "reset")) }
        })
        .await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(result.is_err());
    }

    #[test]
    fn backoff_grows_and_stays_capped() {
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_millis(100),
        };
        assert!(policy.backoff(1) >= Duration::from_millis(100));
        assert!(policy.backoff(1) <= Duration::from_millis(150));
        assert!(policy.backoff(2) >= Duration::from_millis(200));
        for attempt in 1..=32 {
            assert!(policy.backoff(attempt) <= MAX_BACKOFF);
        }
    }
}
