-- Create users table
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for querying by creation date
CREATE INDEX idx_users_created_at ON users(created_at);

-- Add comment explaining the table
COMMENT ON TABLE users IS 'Registered users identified by SHA-256 hash of username';
COMMENT ON COLUMN users.id IS 'SHA-256 hash of username (64 hex characters)';
