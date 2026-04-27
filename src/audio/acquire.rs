use anyhow::{anyhow, Result};
use std::time::Duration;

/// Outcome of one attempt to acquire a device by `acquire_with_backoff`.
#[allow(dead_code)] // ImmediateRetry is only used by the WASAPI backend.
pub enum AcquireDecision<T> {
    /// Successful attempt — return this value.
    Ok(T),
    /// Transient failure: sleep through the next backoff slot, then retry.
    /// Used for "device busy" conditions where another holder is releasing.
    BackoffRetry,
    /// Recoverable failure that doesn't need a sleep — retry immediately
    /// with refreshed parameters. Used e.g. for WASAPI buffer-size alignment
    /// where the second attempt uses a recomputed period.
    ImmediateRetry,
    /// Permanent failure — bail out, no more retries.
    Fatal(anyhow::Error),
}

/// Drive a retry loop for acquiring/initializing a device. The classification
/// returned by `attempt_fn` selects the next action:
///
/// - `Ok(T)` ends the loop successfully.
/// - `BackoffRetry` sleeps through the next entry in `backoffs_ms` and retries;
///   if all backoff slots are exhausted, returns an error.
/// - `ImmediateRetry` retries without sleeping; capped at 4 immediate retries
///   to bound the loop.
/// - `Fatal(e)` propagates `e` as-is.
///
/// `label` appears in the timeout/exhaustion error messages so multiple call
/// sites are distinguishable.
pub fn acquire_with_backoff<T>(
    label: &'static str,
    backoffs_ms: &[u64],
    mut attempt_fn: impl FnMut() -> AcquireDecision<T>,
) -> Result<T> {
    const MAX_IMMEDIATE: usize = 4;
    let mut backoff_used = 0usize;
    let mut immediate_used = 0usize;

    loop {
        match attempt_fn() {
            AcquireDecision::Ok(v) => return Ok(v),
            AcquireDecision::ImmediateRetry => {
                immediate_used += 1;
                if immediate_used > MAX_IMMEDIATE {
                    return Err(anyhow!(
                        "{}: too many immediate retries ({})",
                        label,
                        MAX_IMMEDIATE
                    ));
                }
            }
            AcquireDecision::BackoffRetry => {
                if backoff_used >= backoffs_ms.len() {
                    return Err(anyhow!(
                        "{}: still busy after {} backoff retries",
                        label,
                        backoffs_ms.len()
                    ));
                }
                let ms = backoffs_ms[backoff_used];
                backoff_used += 1;
                std::thread::sleep(Duration::from_millis(ms));
            }
            AcquireDecision::Fatal(e) => return Err(e),
        }
    }
}

/// Default backoff schedule used by both WASAPI and ALSA acquisition paths:
/// 50, 100, 200, 400, 800 ms (cumulative ≤ 1.55 s).
pub const DEFAULT_ACQUIRE_BACKOFFS_MS: &[u64] = &[50, 100, 200, 400, 800];
