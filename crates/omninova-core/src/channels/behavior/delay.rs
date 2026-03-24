//! Response Delay Types
//!
//! Defines response delay configurations for simulating human-like responses.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Response delay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ResponseDelay {
    /// No delay - respond immediately
    None,

    /// Fixed delay in milliseconds
    Fixed { delay_ms: u64 },

    /// Random delay within a range
    Random { min_ms: u64, max_ms: u64 },

    /// Simulate typing with characters per second
    Typing { chars_per_second: f64 },
}

impl Default for ResponseDelay {
    fn default() -> Self {
        Self::None
    }
}

impl ResponseDelay {
    /// Create a no-delay configuration
    pub fn none() -> Self {
        Self::None
    }

    /// Create a fixed delay
    pub fn fixed(duration: Duration) -> Self {
        Self::Fixed {
            delay_ms: duration.as_millis() as u64,
        }
    }

    /// Create a random delay range
    pub fn random(min: Duration, max: Duration) -> Self {
        Self::Random {
            min_ms: min.as_millis() as u64,
            max_ms: max.as_millis() as u64,
        }
    }

    /// Create a typing simulation delay
    pub fn typing(chars_per_second: f64) -> Self {
        Self::Typing { chars_per_second }
    }

    /// Calculate the delay for a given message
    ///
    /// Returns the duration to wait before sending.
    pub fn calculate_delay(&self, message_length: usize) -> Duration {
        match self {
            Self::None => Duration::ZERO,
            Self::Fixed { delay_ms } => Duration::from_millis(*delay_ms),
            Self::Random { min_ms, max_ms } => {
                // Use a simple deterministic "random" based on message length
                // In production, use a proper random number generator
                let range = max_ms - min_ms;
                let factor = (message_length as u64 % 100) as f64 / 100.0;
                let delay = min_ms + (range as f64 * factor) as u64;
                Duration::from_millis(delay)
            }
            Self::Typing { chars_per_second } => {
                if *chars_per_second <= 0.0 {
                    return Duration::ZERO;
                }
                let seconds = message_length as f64 / chars_per_second;
                Duration::from_secs_f64(seconds)
            }
        }
    }

    /// Check if this delay configuration has any delay
    pub fn has_delay(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Typing delay calculator for more realistic simulations
pub struct TypingDelay {
    /// Base characters per second
    pub chars_per_second: f64,

    /// Variance factor (0.0 to 1.0)
    pub variance: f64,

    /// Pause probability per sentence (0.0 to 1.0)
    pub sentence_pause_probability: f64,

    /// Sentence pause duration in milliseconds
    pub sentence_pause_ms: u64,
}

impl Default for TypingDelay {
    fn default() -> Self {
        Self {
            chars_per_second: 30.0,      // ~30 chars/sec is moderate typing
            variance: 0.2,               // 20% variance
            sentence_pause_probability: 0.5,
            sentence_pause_ms: 200,
        }
    }
}

impl TypingDelay {
    /// Create a new typing delay calculator
    pub fn new(chars_per_second: f64) -> Self {
        Self {
            chars_per_second,
            ..Self::default()
        }
    }

    /// Calculate the total delay for typing a message
    pub fn calculate(&self, message: &str) -> Duration {
        let base_time = message.len() as f64 / self.chars_per_second;

        // Add variance
        let variance_factor = 1.0 + (self.variance * 2.0 - self.variance);
        let adjusted_time = base_time * variance_factor;

        // Count sentences for potential pauses
        let sentence_count = message.matches(|c| c == '.' || c == '!' || c == '?').count();
        let expected_pauses = (sentence_count as f64 * self.sentence_pause_probability) as u64;
        let pause_time = expected_pauses * self.sentence_pause_ms;

        Duration::from_secs_f64(adjusted_time) + Duration::from_millis(pause_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none_delay() {
        let delay = ResponseDelay::none();
        assert_eq!(delay.calculate_delay(100), Duration::ZERO);
        assert!(!delay.has_delay());
    }

    #[test]
    fn test_fixed_delay() {
        let delay = ResponseDelay::fixed(Duration::from_millis(500));
        assert_eq!(delay.calculate_delay(100), Duration::from_millis(500));
        assert!(delay.has_delay());
    }

    #[test]
    fn test_random_delay() {
        let delay = ResponseDelay::random(Duration::from_millis(100), Duration::from_millis(500));
        let result = delay.calculate_delay(50);

        // Result should be within range (deterministic based on message length)
        assert!(result >= Duration::from_millis(100));
        assert!(result <= Duration::from_millis(500));
    }

    #[test]
    fn test_typing_delay() {
        let delay = ResponseDelay::typing(10.0); // 10 chars per second
        let result = delay.calculate_delay(100);

        // 100 chars / 10 chars/sec = 10 seconds
        assert_eq!(result, Duration::from_secs(10));
    }

    #[test]
    fn test_typing_delay_zero_rate() {
        let delay = ResponseDelay::typing(0.0);
        let result = delay.calculate_delay(100);
        assert_eq!(result, Duration::ZERO);
    }

    #[test]
    fn test_typing_delay_calculator() {
        let typing = TypingDelay::new(50.0);
        let message = "Hello world. This is a test.";
        let duration = typing.calculate(message);

        // Should take ~0.5 seconds for 27 chars at 50 cps, plus possible pauses
        assert!(duration >= Duration::from_millis(400));
        assert!(duration < Duration::from_secs(2));
    }

    #[test]
    fn test_serialization() {
        let delay = ResponseDelay::fixed(Duration::from_millis(1000));
        let json = serde_json::to_string(&delay).unwrap();
        assert!(json.contains("fixed"));
        assert!(json.contains("1000"));

        let deserialized: ResponseDelay = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ResponseDelay::Fixed { delay_ms: 1000 }));
    }

    #[test]
    fn test_serialization_typing() {
        let delay = ResponseDelay::typing(25.5);
        let json = serde_json::to_string(&delay).unwrap();
        let deserialized: ResponseDelay = serde_json::from_str(&json).unwrap();

        if let ResponseDelay::Typing { chars_per_second } = deserialized {
            assert!((chars_per_second - 25.5).abs() < 0.001);
        } else {
            panic!("Expected Typing variant");
        }
    }
}