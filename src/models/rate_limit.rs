use serde::{Deserialize, Serialize};

use crate::constants::{MAX_BACKUPS_PER_DAY, MAX_BACKUPS_PER_HOUR};
use crate::error::{AppError, Result};

/// Rate limit record for tracking backup frequency per user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitRecord {
    /// Number of backups made in the current hour window
    pub backups_this_hour: u32,
    /// Number of backups made in the current day window
    pub backups_today: u32,
    /// Unix timestamp of the last backup
    pub last_backup_at: Option<i64>,
    /// Unix timestamp when the hourly counter resets
    pub hour_reset_at: i64,
    /// Unix timestamp when the daily counter resets
    pub day_reset_at: i64,
}

impl RateLimitRecord {
    /// Create a new rate limit record with initial reset times
    pub fn new(now: i64) -> Self {
        Self {
            backups_this_hour: 0,
            backups_today: 0,
            last_backup_at: None,
            hour_reset_at: now + 3600,  // 1 hour from now
            day_reset_at: now + 86400,  // 24 hours from now
        }
    }

    /// Check if rate limits allow a new backup, and update counters if allowed
    /// Returns Ok(()) if allowed, Err(RateLimitExceeded) if not
    pub fn check_and_increment(&mut self, now: i64) -> Result<()> {
        // Reset counters if time windows have expired
        if now >= self.hour_reset_at {
            self.backups_this_hour = 0;
            self.hour_reset_at = now + 3600;
        }

        if now >= self.day_reset_at {
            self.backups_today = 0;
            self.day_reset_at = now + 86400;
        }

        // Check limits before incrementing
        if self.backups_this_hour >= MAX_BACKUPS_PER_HOUR as u32 {
            tracing::warn!(
                "Hourly rate limit would be exceeded: {}/{}",
                self.backups_this_hour,
                MAX_BACKUPS_PER_HOUR
            );
            return Err(AppError::RateLimitExceeded);
        }

        if self.backups_today >= MAX_BACKUPS_PER_DAY as u32 {
            tracing::warn!(
                "Daily rate limit would be exceeded: {}/{}",
                self.backups_today,
                MAX_BACKUPS_PER_DAY
            );
            return Err(AppError::RateLimitExceeded);
        }

        // Increment counters
        self.backups_this_hour += 1;
        self.backups_today += 1;
        self.last_backup_at = Some(now);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rate_limit_record() {
        let now = 1000000;
        let record = RateLimitRecord::new(now);

        assert_eq!(record.backups_this_hour, 0);
        assert_eq!(record.backups_today, 0);
        assert!(record.last_backup_at.is_none());
        assert_eq!(record.hour_reset_at, now + 3600);
        assert_eq!(record.day_reset_at, now + 86400);
    }

    #[test]
    fn test_check_and_increment_success() {
        let now = 1000000;
        let mut record = RateLimitRecord::new(now);

        // First backup should succeed
        assert!(record.check_and_increment(now).is_ok());
        assert_eq!(record.backups_this_hour, 1);
        assert_eq!(record.backups_today, 1);
        assert_eq!(record.last_backup_at, Some(now));
    }

    #[test]
    fn test_hourly_rate_limit() {
        let now = 1000000;
        let mut record = RateLimitRecord::new(now);

        // Use up hourly limit
        for _ in 0..MAX_BACKUPS_PER_HOUR {
            assert!(record.check_and_increment(now).is_ok());
        }

        // Next should fail
        assert!(matches!(
            record.check_and_increment(now),
            Err(AppError::RateLimitExceeded)
        ));
    }

    #[test]
    fn test_hourly_reset() {
        let now = 1000000;
        let mut record = RateLimitRecord::new(now);

        // Use up hourly limit
        for _ in 0..MAX_BACKUPS_PER_HOUR {
            assert!(record.check_and_increment(now).is_ok());
        }

        // After hour resets, should succeed again
        let after_reset = now + 3601;
        assert!(record.check_and_increment(after_reset).is_ok());
        assert_eq!(record.backups_this_hour, 1);
    }

    #[test]
    fn test_daily_rate_limit() {
        let mut now = 1000000;
        let mut record = RateLimitRecord::new(now);

        // Use up daily limit (resetting hourly as needed)
        for i in 0..MAX_BACKUPS_PER_DAY {
            // Move time forward past hourly reset if needed
            if i > 0 && i as u32 % MAX_BACKUPS_PER_HOUR as u32 == 0 {
                now += 3601;
            }
            assert!(record.check_and_increment(now).is_ok(), "Backup {} should succeed", i);
        }

        // Move past hourly reset but not daily
        now += 3601;

        // Should still fail because daily limit reached
        assert!(matches!(
            record.check_and_increment(now),
            Err(AppError::RateLimitExceeded)
        ));
    }
}
