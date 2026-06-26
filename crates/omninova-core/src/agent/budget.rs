//! Hard run budgets spanning a whole request (plan + execute + reflect).

use crate::config::schema::BudgetConfig;
use crate::providers::TokenUsage;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

/// Tracks consumption against [`BudgetConfig`] caps. Cheap to share by
/// reference across the planner, executor and reflector of one request.
pub struct BudgetTracker {
    started: Instant,
    total_tokens: AtomicU64,
    provider_calls: AtomicU32,
    limits: BudgetConfig,
}

impl BudgetTracker {
    pub fn new(limits: BudgetConfig) -> Self {
        Self {
            started: Instant::now(),
            total_tokens: AtomicU64::new(0),
            provider_calls: AtomicU32::new(0),
            limits,
        }
    }

    /// Record one provider call and its reported token usage.
    pub fn record_call(&self, usage: Option<&TokenUsage>) {
        self.provider_calls.fetch_add(1, Ordering::Relaxed);
        if let Some(usage) = usage {
            let tokens = usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0);
            if tokens > 0 {
                self.total_tokens.fetch_add(tokens, Ordering::Relaxed);
            }
        }
    }

    /// Returns the violation reason when any configured cap is exceeded.
    pub fn check(&self) -> Option<String> {
        if let Some(max_secs) = self.limits.max_wall_time_secs {
            let elapsed = self.started.elapsed().as_secs();
            if elapsed >= max_secs {
                return Some(format!(
                    "wall-time budget exhausted ({elapsed}s elapsed, cap {max_secs}s)"
                ));
            }
        }
        if let Some(max_tokens) = self.limits.max_total_tokens {
            let used = self.total_tokens.load(Ordering::Relaxed);
            if used >= max_tokens {
                return Some(format!(
                    "token budget exhausted ({used} tokens used, cap {max_tokens})"
                ));
            }
        }
        if let Some(max_calls) = self.limits.max_provider_calls {
            let calls = self.provider_calls.load(Ordering::Relaxed);
            if calls >= max_calls {
                return Some(format!(
                    "provider-call budget exhausted ({calls} calls made, cap {max_calls})"
                ));
            }
        }
        None
    }

    pub fn summary(&self) -> String {
        format!(
            "tokens={} provider_calls={} elapsed={}s",
            self.total_tokens.load(Ordering::Relaxed),
            self.provider_calls.load(Ordering::Relaxed),
            self.started.elapsed().as_secs()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_budget_never_trips() {
        let tracker = BudgetTracker::new(BudgetConfig::default());
        tracker.record_call(Some(&TokenUsage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(1_000_000),
        }));
        assert!(tracker.check().is_none());
    }

    #[test]
    fn token_cap_trips() {
        let tracker = BudgetTracker::new(BudgetConfig {
            max_total_tokens: Some(100),
            ..BudgetConfig::default()
        });
        assert!(tracker.check().is_none());
        tracker.record_call(Some(&TokenUsage {
            input_tokens: Some(80),
            output_tokens: Some(30),
        }));
        let reason = tracker.check().expect("should trip");
        assert!(reason.contains("token budget"));
    }

    #[test]
    fn provider_call_cap_trips() {
        let tracker = BudgetTracker::new(BudgetConfig {
            max_provider_calls: Some(2),
            ..BudgetConfig::default()
        });
        tracker.record_call(None);
        assert!(tracker.check().is_none());
        tracker.record_call(None);
        let reason = tracker.check().expect("should trip");
        assert!(reason.contains("provider-call budget"));
    }

    #[test]
    fn wall_time_cap_trips_at_zero() {
        let tracker = BudgetTracker::new(BudgetConfig {
            max_wall_time_secs: Some(0),
            ..BudgetConfig::default()
        });
        let reason = tracker.check().expect("should trip immediately");
        assert!(reason.contains("wall-time budget"));
    }
}
