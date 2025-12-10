-- Create backups table
CREATE TABLE backups (
    storage_key TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    encrypted_data TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX idx_backups_user_id ON backups(user_id);
CREATE INDEX idx_backups_updated_at ON backups(updated_at);

-- Add comments
COMMENT ON TABLE backups IS 'Encrypted user backup data';
COMMENT ON COLUMN backups.storage_key IS 'SHA-256 hash of userId + password';
COMMENT ON COLUMN backups.encrypted_data IS 'Base64-encoded encrypted data blob';
