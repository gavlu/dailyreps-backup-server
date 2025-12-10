-- Create rate limiting table for tracking backup frequency
CREATE TABLE user_rate_limits (
    user_id TEXT PRIMARY KEY,
    backups_this_hour INTEGER NOT NULL DEFAULT 0,
    backups_today INTEGER NOT NULL DEFAULT 0,
    last_backup_at TIMESTAMPTZ,
    hour_reset_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    day_reset_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Index for querying rate limits
CREATE INDEX idx_rate_limits_hour_reset ON user_rate_limits(hour_reset_at);
CREATE INDEX idx_rate_limits_day_reset ON user_rate_limits(day_reset_at);

-- Add comments
COMMENT ON TABLE user_rate_limits IS 'Track backup frequency per user to prevent abuse';
COMMENT ON COLUMN user_rate_limits.backups_this_hour IS 'Number of backups in current hour window';
COMMENT ON COLUMN user_rate_limits.backups_today IS 'Number of backups in current day window';
