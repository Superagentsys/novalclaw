//! Working Hours Types and Checker
//!
//! Defines working hours configuration and provides checking utilities.

use chrono::{DateTime, Datelike, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// Working hours configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingHours {
    /// Timezone for interpreting times (e.g., "Asia/Shanghai", "UTC")
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// Time slots during the day when active
    pub time_slots: Vec<TimeSlot>,

    /// Enabled days of week (1 = Monday, 7 = Sunday)
    #[serde(default = "default_workdays")]
    pub enabled_days: Vec<u8>,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_workdays() -> Vec<u8> {
    vec![1, 2, 3, 4, 5] // Monday to Friday
}

impl Default for WorkingHours {
    fn default() -> Self {
        Self {
            timezone: default_timezone(),
            time_slots: vec![TimeSlot::default()],
            enabled_days: default_workdays(),
        }
    }
}

impl WorkingHours {
    /// Create working hours with default settings (9-5, Mon-Fri)
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the timezone
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = timezone.into();
        self
    }

    /// Set the time slots
    pub fn with_time_slots(mut self, slots: Vec<TimeSlot>) -> Self {
        self.time_slots = slots;
        self
    }

    /// Set the enabled days
    pub fn with_enabled_days(mut self, days: Vec<u8>) -> Self {
        self.enabled_days = days;
        self
    }

    /// Add a time slot
    pub fn add_time_slot(mut self, slot: TimeSlot) -> Self {
        self.time_slots.push(slot);
        self
    }
}

/// A time slot during the day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSlot {
    /// Start time in HH:MM format (24-hour)
    pub start: String,

    /// End time in HH:MM format (24-hour)
    pub end: String,
}

impl Default for TimeSlot {
    fn default() -> Self {
        Self {
            start: "09:00".to_string(),
            end: "17:00".to_string(),
        }
    }
}

impl TimeSlot {
    /// Create a new time slot
    pub fn new(start: &str, end: &str) -> Self {
        Self {
            start: start.to_string(),
            end: end.to_string(),
        }
    }

    /// Parse start time
    pub fn parse_start(&self) -> Result<NaiveTime, String> {
        NaiveTime::parse_from_str(&self.start, "%H:%M")
            .map_err(|e| format!("Invalid start time '{}': {}", self.start, e))
    }

    /// Parse end time
    pub fn parse_end(&self) -> Result<NaiveTime, String> {
        NaiveTime::parse_from_str(&self.end, "%H:%M")
            .map_err(|e| format!("Invalid end time '{}': {}", self.end, e))
    }

    /// Check if a time is within this slot
    pub fn contains(&self, time: NaiveTime) -> bool {
        match (self.parse_start(), self.parse_end()) {
            (Ok(start), Ok(end)) => time >= start && time <= end,
            _ => false,
        }
    }
}

/// Checker for working hours
pub struct WorkingHoursChecker;

impl WorkingHoursChecker {
    /// Check if the current time is within working hours
    pub fn is_within_working_hours(config: &WorkingHours) -> bool {
        Self::is_within_working_hours_at_utc(config, Utc::now())
    }

    /// Check if a specific UTC datetime is within working hours
    pub fn is_within_working_hours_at_utc(config: &WorkingHours, dt: DateTime<Utc>) -> bool {
        // Get the day of week (1 = Monday, 7 = Sunday)
        let weekday = dt.weekday().number_from_monday() as u8;

        // Check if this day is enabled
        if !config.enabled_days.contains(&weekday) {
            return false;
        }

        // Convert to the configured timezone, get the time portion
        let time = match config.timezone.parse::<chrono_tz::Tz>() {
            Ok(tz) => dt.with_timezone(&tz).time(),
            Err(_) => dt.time(), // Fall back to UTC if timezone is invalid
        };

        // Check if the time falls within any time slot
        config.time_slots.iter().any(|slot| slot.contains(time))
    }

    /// Check if a specific datetime is within working hours (generic version)
    pub fn is_within_working_hours_at<Tz: TimeZone>(config: &WorkingHours, dt: DateTime<Tz>) -> bool {
        Self::is_within_working_hours_at_utc(config, dt.with_timezone(&Utc))
    }

    /// Get the next working time after the given datetime
    pub fn next_working_time(config: &WorkingHours, dt: DateTime<Utc>) -> Option<DateTime<Utc>> {
        // Simple implementation: check next 7 days
        let mut check_time = dt;

        for _ in 0..168 { // Check next 168 hours (7 days)
            check_time = check_time + chrono::Duration::hours(1);

            if Self::is_within_working_hours_at_utc(config, check_time) {
                return Some(check_time);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_slot_default() {
        let slot = TimeSlot::default();
        assert_eq!(slot.start, "09:00");
        assert_eq!(slot.end, "17:00");
    }

    #[test]
    fn test_time_slot_contains() {
        let slot = TimeSlot::new("09:00", "17:00");

        let within = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        assert!(slot.contains(within));

        let before = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        assert!(!slot.contains(before));

        let after = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        assert!(!slot.contains(after));
    }

    #[test]
    fn test_working_hours_default() {
        let hours = WorkingHours::default();
        assert_eq!(hours.timezone, "UTC");
        assert_eq!(hours.enabled_days, vec![1, 2, 3, 4, 5]);
        assert_eq!(hours.time_slots.len(), 1);
    }

    #[test]
    fn test_working_hours_builder() {
        let hours = WorkingHours::new()
            .with_timezone("Asia/Shanghai")
            .with_enabled_days(vec![1, 2, 3, 4, 5, 6]);

        assert_eq!(hours.timezone, "Asia/Shanghai");
        assert_eq!(hours.enabled_days.len(), 6);
    }

    #[test]
    fn test_checker_weekend() {
        let hours = WorkingHours::default();

        // Create a Saturday (day 6)
        let saturday = chrono::NaiveDate::from_ymd_opt(2024, 1, 6).unwrap()
            .and_hms_opt(12, 0, 0).unwrap();

        let saturday_dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(saturday, Utc);

        assert!(!WorkingHoursChecker::is_within_working_hours_at(&hours, saturday_dt));
    }

    #[test]
    fn test_checker_weekday() {
        let hours = WorkingHours::default();

        // Create a Wednesday at noon (day 3)
        let wednesday = chrono::NaiveDate::from_ymd_opt(2024, 1, 3).unwrap()
            .and_hms_opt(12, 0, 0).unwrap();

        let wednesday_dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(wednesday, Utc);

        assert!(WorkingHoursChecker::is_within_working_hours_at(&hours, wednesday_dt));
    }

    #[test]
    fn test_checker_after_hours() {
        let hours = WorkingHours::default();

        // Create a Wednesday at 8pm (after 5pm)
        let evening = chrono::NaiveDate::from_ymd_opt(2024, 1, 3).unwrap()
            .and_hms_opt(20, 0, 0).unwrap();

        let evening_dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(evening, Utc);

        assert!(!WorkingHoursChecker::is_within_working_hours_at(&hours, evening_dt));
    }

    #[test]
    fn test_serialization() {
        let hours = WorkingHours::new()
            .with_timezone("America/New_York")
            .add_time_slot(TimeSlot::new("08:00", "12:00"));

        let json = serde_json::to_string(&hours).unwrap();
        let deserialized: WorkingHours = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.timezone, "America/New_York");
        assert_eq!(deserialized.time_slots.len(), 2);
    }
}