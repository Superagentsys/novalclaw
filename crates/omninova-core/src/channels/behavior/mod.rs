//! Channel Behavior Configuration
//!
//! This module provides types and utilities for configuring channel-specific
//! behavior such as response style, trigger keywords, delays, and working hours.

mod config;
mod delay;
mod style;
mod trigger;
mod working_hours;

pub use config::{ChannelBehaviorConfig, ChannelBehaviorStore, SqliteBehaviorStore};
pub use delay::{ResponseDelay, TypingDelay};
pub use style::{ResponseStyle, ResponseStyleProcessor};
pub use trigger::{MatchType, TriggerKeyword, TriggerKeywordMatcher};
pub use working_hours::{TimeSlot, WorkingHours, WorkingHoursChecker};