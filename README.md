# DailyReps Backup Server

A secure Rust backend service for the DailyReps workout tracking app, providing zero-knowledge encrypted backup storage with comprehensive anti-griefing measures.

![Rust](https://img.shields.io/badge/Rust-1.91-orange?style=flat&logo=rust)
![Axum](https://img.shields.io/badge/Axum-0.7-blue?style=flat)
![PostgreSQL](https://img.shields.io/badge/PostgreSQL-16-blue?style=flat&logo=postgresql)
![License](https://img.shields.io/badge/license-MIT-blue)

## Overview

This server stores encrypted workout data from DailyReps clients using a zero-knowledge architecture - the server never has access to plaintext data. All encryption and decryption happens client-side.

### Key Features

- 🔐 **Zero-knowledge encryption** - Server stores only encrypted blobs
- 🛡️ **HMAC signature verification** - Ensures data comes from official app
- ⏱️ **Timestamp validation** - Prevents replay attacks (5-minute window)
- 🚦 **Rate limiting** - Database-backed limits (5/hour, 20/day per user)
- 📏 **Size limits** - 5MB maximum payload size
- 🗑️ **Complete deletion** - Users can permanently delete all their data
- 🔄 **CORS support** - Configured for web client access

## Architecture

### Zero-Knowledge Design

The server never sees plaintext data or encryption keys:

```
Client generates:
├── serverUserId = SHA-256(username)           # Server sees this
├── storageKey = SHA-256(serverUserId + password)  # Server sees this
├── encryptionKey = PBKDF2(password, username)     # Server NEVER sees this
└── encrypted = AES-GCM(data, encryptionKey)       # Server stores this

Server verifies:
├── HMAC signature using APP_SECRET_KEY
├── Timestamp within 5-minute window
└── Payload size under 5MB limit
```

### Database Schema

**users**
- `id` (TEXT, PK) - SHA-256(username)
- `storage_key` (TEXT, UNIQUE) - For backup retrieval
- `created_at` (TIMESTAMPTZ)

**user_backups**
- `storage_key` (TEXT, PK, FK → users)
- `data` (TEXT) - Encrypted JSON blob
- `updated_at` (TIMESTAMPTZ)

**user_rate_limits**
- `user_id` (TEXT, PK, FK → users)
- `backups_this_hour` (INTEGER)
- `backups_today` (INTEGER)
- `last_backup_at` (TIMESTAMPTZ)
- `hour_reset_at` (TIMESTAMPTZ)
- `day_reset_at` (TIMESTAMPTZ)

## Technology Stack

- **Framework**: [Axum 0.7](https://github.com/tokio-rs/axum) - Fast, ergonomic web framework
- **Runtime**: [Tokio](https://tokio.rs/) - Async runtime
- **Database**: PostgreSQL 16 with [SQLx 0.7](https://github.com/launchbadge/sqlx)
- **Security**: HMAC-SHA256 for signatures, timestamp validation
- **CORS**: [tower-http](https://github.com/tower-rs/tower-http)

## API Endpoints

### POST /api/register
Register a new backup user.

**Request:**
```json
{
  "userId": "64-char-hex-sha256",
  "signature": "64-char-hex-hmac-sha256",
  "timestamp": 1234567890
}
```

**Response:**
```json
{
  "success": true
}
```

**Errors:**
- `409 Conflict` - User already exists
- `401 Unauthorized` - Invalid signature or timestamp

---

### POST /api/backup
Store encrypted backup data.

**Request:**
```json
{
  "userId": "64-char-hex-sha256",
  "storageKey": "64-char-hex-sha256",
  "data": "{\"encrypted\":\"base64...\",\"nonce\":\"base64...\"}",
  "signature": "64-char-hex-hmac-sha256",
  "timestamp": 1234567890
}
```

**Response:**
```json
{
  "success": true,
  "updatedAt": "2025-01-01T12:00:00Z"
}
```

**Errors:**
- `401 Unauthorized` - Invalid signature or timestamp
- `404 Not Found` - User not registered
- `413 Payload Too Large` - Data exceeds 5MB
- `429 Too Many Requests` - Rate limit exceeded

---

### GET /api/backup?userId={userId}&storageKey={storageKey}
Retrieve encrypted backup data.

**Response:**
```json
{
  "data": "{\"encrypted\":\"base64...\",\"nonce\":\"base64...\"}",
  "updatedAt": "2025-01-01T12:00:00Z"
}
```

**Errors:**
- `404 Not Found` - No backup found for this user
- `401 Unauthorized` - Storage key doesn't match user

---

### DELETE /api/user
Permanently delete user and all associated data.

**Request:**
```json
{
  "userId": "64-char-hex-sha256",
  "storageKey": "64-char-hex-sha256",
  "signature": "64-char-hex-hmac-sha256",
  "timestamp": 1234567890
}
```

**Response:**
```json
{
  "success": true,
  "message": "User and all associated data permanently deleted"
}
```

**Errors:**
- `401 Unauthorized` - Invalid signature, timestamp, or storage key
- `404 Not Found` - User not found

---

### GET /health
Health check endpoint.

**Response:**
```json
{
  "status": "healthy"
}
```

## Setup & Installation

### Prerequisites

- Rust 1.91+ (install via [rustup](https://rustup.rs/))
- PostgreSQL 16+
- OpenSSL (for HMAC)

### Database Setup

```bash
# Create database
createdb dailyreps_backup

# Set DATABASE_URL
export DATABASE_URL="postgresql://username:password@localhost/dailyreps_backup"

# Run migrations
sqlx migrate run
```

### Configuration

Create `.env` file:

```bash
# Server Configuration
PORT=8080
HOST=0.0.0.0

# Database
DATABASE_URL=postgresql://username:password@localhost/dailyreps_backup

# Security (MUST match client app)
APP_SECRET_KEY=your-secret-key-here-generate-with-openssl-rand-hex-32

# CORS (your client domain)
ALLOWED_ORIGIN=https://your-app.netlify.app
```

**Generate secure secret key:**
```bash
openssl rand -hex 32
```

### Build & Run

```bash
# Development
cargo run

# Production build
cargo build --release
./target/release/dailyreps-backup-server

# With logging
RUST_LOG=info cargo run
```

## Development

### Project Structure

```
dailyreps-backup-server/
├── src/
│   ├── main.rs              # Server entry point, routes
│   ├── config.rs            # Environment configuration
│   ├── constants.rs         # Anti-griefing limits
│   ├── error.rs             # Custom error types
│   ├── security.rs          # HMAC & timestamp validation
│   ├── models/              # Data models
│   │   ├── user.rs
│   │   └── backup.rs
│   ├── db/
│   │   └── pool.rs          # Database connection
│   └── routes/              # API endpoints
│       ├── health.rs
│       ├── register.rs
│       ├── backup.rs
│       └── delete.rs
├── migrations/              # SQL migrations
│   ├── 20251209000001_create_users.sql
│   ├── 20251209000002_create_backups.sql
│   └── 20251209000003_create_rate_limits.sql
├── Cargo.toml
├── CLAUDE.md               # Development guide
└── IMPLEMENTATION_PLAN.md  # Implementation status
```

### Anti-Griefing Measures

**Layer 1: Size Limits**
- 5MB hard limit on payload size
- 1MB warning threshold logged
- Defined in `src/constants.rs`

**Layer 2: Rate Limiting**
- 5 backups per hour per user
- 20 backups per day per user
- Database-backed tracking in `user_rate_limits` table
- Automatic reset at hour/day boundaries

**Layer 3: HMAC Signatures**
- All requests must include HMAC-SHA256 signature
- Proves data comes from official app
- Uses shared `APP_SECRET_KEY`

**Layer 4: Timestamp Validation**
- Requests must include current Unix timestamp
- Must be within 5-minute window (±300 seconds)
- Prevents replay attacks

### Testing

Currently focused on integration testing via client test suite. Future work includes:

- [ ] Unit tests for crypto validation
- [ ] Integration tests for API endpoints
- [ ] Load testing for rate limits
- [ ] Security audit

## Deployment

### Fly.io (Recommended)

```bash
# Install flyctl
curl -L https://fly.io/install.sh | sh

# Login
fly auth login

# Create app
fly launch

# Set secrets
fly secrets set APP_SECRET_KEY=your-secret-here
fly secrets set DATABASE_URL=postgres://...

# Deploy
fly deploy
```

### Docker

```dockerfile
FROM rust:1.91 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libpq5 ca-certificates
COPY --from=builder /app/target/release/dailyreps-backup-server /usr/local/bin/
CMD ["dailyreps-backup-server"]
```

### Environment Variables (Production)

```bash
# Required
DATABASE_URL=postgresql://...
APP_SECRET_KEY=xxx  # Must match client
ALLOWED_ORIGIN=https://your-app.com

# Optional
PORT=8080
HOST=0.0.0.0
RUST_LOG=info
```

## Security Considerations

### What the Server Can See
- ✅ Encrypted data blobs (AES-GCM ciphertext)
- ✅ SHA-256 hashes (userIds, storageKeys)
- ✅ Timestamps and rate limit counters
- ✅ Request metadata (IP, timing, etc.)

### What the Server CANNOT See
- ❌ Plaintext workout data
- ❌ Encryption keys (PBKDF2 derived)
- ❌ Original usernames
- ❌ Passwords
- ❌ Any unencrypted user information

### Threat Model

**Protected Against:**
- ✅ Unauthorized data access (server admin can't read data)
- ✅ Replay attacks (timestamp validation)
- ✅ Fake clients (HMAC signatures)
- ✅ Storage abuse (size limits, rate limiting)
- ✅ User enumeration (consistent error messages)

**Not Protected Against:**
- ⚠️ Client compromise (if client is hacked, keys are exposed)
- ⚠️ Weak passwords (use strong passwords!)
- ⚠️ Physical device access (data encrypted at rest in DB recommended)

### Recommendations

1. **Use TLS/HTTPS** - Encrypt all network traffic
2. **Rotate secrets** - Change APP_SECRET_KEY periodically
3. **Monitor logs** - Watch for suspicious patterns
4. **Database encryption** - Enable PostgreSQL encryption at rest
5. **Regular backups** - Back up the PostgreSQL database
6. **Firewall rules** - Limit database access to app server only

## Monitoring & Logging

```bash
# Development logging
RUST_LOG=debug cargo run

# Production logging (structured)
RUST_LOG=info cargo run

# Log levels by module
RUST_LOG=dailyreps_backup_server=debug,axum=info cargo run
```

**Key log events:**
- User registration
- Backup operations (save/restore)
- Rate limit warnings
- Security validation failures
- Large payload warnings (>1MB)

## Troubleshooting

### Database Connection Issues

```bash
# Test connection
psql $DATABASE_URL

# Check migrations
sqlx migrate info
```

### HMAC Signature Mismatches

- Ensure `APP_SECRET_KEY` matches between client and server
- Check timestamp synchronization
- Verify request body hasn't been modified in transit

### Rate Limit Issues

```sql
-- Check user rate limits
SELECT * FROM user_rate_limits WHERE user_id = 'xxx';

-- Reset for testing
UPDATE user_rate_limits SET backups_this_hour = 0, backups_today = 0;
```

## Contributing

Contributions welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure `cargo clippy` passes
5. Format with `cargo fmt`
6. Submit a Pull Request

## License

MIT License - see LICENSE file for details

## Related Projects

- **DailyReps** - Frontend workout tracking app
  - Repository: [gavlu/dailyreps](https://github.com/gavlu/dailyreps)
  - Tech: SvelteKit, TypeScript, Web Crypto API

## Acknowledgments

- Built with [Axum](https://github.com/tokio-rs/axum)
- Powered by [Tokio](https://tokio.rs/)
- Database via [SQLx](https://github.com/launchbadge/sqlx)

---

**Built for privacy-conscious fitness enthusiasts**

🤖 Generated with [Claude Code](https://claude.com/claude-code)
