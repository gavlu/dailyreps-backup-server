# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a production-ready Rust backend API server that provides secure, encrypted backup storage for the DailyReps exercise tracking application. The server implements zero-knowledge architecture where user data is encrypted client-side before transmission, ensuring the server cannot access user information even if compromised.

**Security-First Design**: This application handles encrypted user data and must maintain the highest security standards. All cryptographic operations, database queries, and API endpoints must be designed with security as the primary concern.

## Core Architecture Principles

### Zero-Knowledge Architecture

The server operates on a zero-knowledge principle:

1. **Client-Side Encryption**: All user data is encrypted on the client device before transmission
2. **Server Cannot Decrypt**: Server never receives encryption keys or unencrypted data
3. **Hash-Based Identity**: User identities are SHA-256 hashes of usernames, server never sees actual usernames
4. **Dual-Key System**:
   - `serverUserId` = sha256(username.toLowerCase()) - for preventing duplicate registrations
   - `storageKey` = sha256(serverUserId + password) - for data storage location
   - `encryptionKey` = pbkdf2(password, username) - stays on client, never sent to server

### What the Server Knows vs. Doesn't Know

**Server KNOWS:**
- User ID hashes (can prevent duplicates)
- Encrypted data blobs (cannot read contents)
- Timestamp metadata (created_at, updated_at)
- Request patterns (for rate limiting)

**Server CANNOT Know:**
- Actual usernames (only has sha256 hash)
- User passwords (never transmitted)
- Encryption keys (derived client-side only)
- Actual user data (encrypted before transmission)

## Technology Stack

### Web Framework
- **Axum 0.8** - Type-safe, ergonomic web framework built on Tokio
- **Tower 0.5** - Middleware for CORS, tracing
- **Tokio** - Async runtime

### Database
- **redb 3** - Embedded key-value database (no external dependencies)
- **bincode 2** - Binary serialization for database records

### Security & Cryptography
- **sha2** - SHA-256 hashing for user IDs and storage keys
- **hmac** - HMAC-SHA256 signature verification
- **tower-http 0.6** - CORS middleware, request logging

### Serialization & Configuration
- **serde** + **serde_json** - JSON serialization/deserialization
- **dotenvy** - Environment variable management

### Development & Testing
- **tokio-test** - Testing utilities for async code
- **reqwest** - HTTP client for integration tests
- **tempfile** - Temporary directories for test databases

## Project Structure

```
dailyreps-backup-server/
├── src/
│   ├── main.rs              # Application entry point, server setup
│   ├── config.rs            # Configuration management
│   ├── constants.rs         # Limits & security constants
│   ├── error.rs             # Error types and handling
│   ├── security.rs          # HMAC verification, timestamp validation
│   ├── routes/
│   │   ├── mod.rs           # Route module exports
│   │   ├── admin.rs         # Admin diagnostics endpoint
│   │   ├── health.rs        # Health check endpoint
│   │   ├── register.rs      # User registration
│   │   ├── backup.rs        # Backup storage/retrieval
│   │   └── delete.rs        # User deletion
│   ├── models/
│   │   ├── mod.rs           # Model exports
│   │   ├── user.rs          # User model
│   │   ├── backup.rs        # Backup model
│   │   └── rate_limit.rs    # Rate limit tracking
│   └── db/
│       ├── mod.rs           # Database initialization
│       └── tables.rs        # redb table definitions
├── tests/
│   └── integration_tests.rs # Integration tests
├── Cargo.toml               # Dependencies and metadata
├── .env.example             # Example environment variables
├── Dockerfile               # Production container image
└── fly.toml                 # Fly.io deployment configuration
```

## Development Commands

### Running the application

```bash
# Set required environment variables
export APP_SECRET_KEY=$(openssl rand -hex 32)
export DATABASE_PATH=./data/dailyreps.db
export ALLOWED_ORIGINS=http://localhost:5173

# Start development server
cargo run

# Start with auto-reload
cargo watch -x run

# Database file is created automatically at DATABASE_PATH
```

### Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run integration tests only
cargo test --test '*'

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html
```

### Code Quality

```bash
# Format code
cargo fmt

# Check formatting without making changes
cargo fmt -- --check

# Run linter
cargo clippy

# Run linter with all warnings as errors (CI mode)
cargo clippy -- -D warnings

# Check code compiles without building
cargo check
```

### Building for Production

```bash
# Build optimized release binary
cargo build --release

# Build Docker image
docker build -t dailyreps-backup-server .

# Run production binary
./target/release/dailyreps-backup-server
```

## API Endpoints

### POST /api/register
Register a new user by claiming a server user ID.

**Request:**
```json
{
  "userId": "64-char-hex-sha256",
  "signature": "64-char-hex-hmac-sha256",
  "timestamp": 1234567890
}
```

**Response (200):**
```json
{
  "success": true
}
```

**Errors:**
- `409 Conflict` - User already exists
- `401 Unauthorized` - Invalid signature or timestamp

### POST /api/backup
Store or update encrypted backup data.

**Request:**
```json
{
  "userId": "64-char-hex-sha256",
  "storageKey": "64-char-hex-sha256",
  "data": "base64_encoded_encrypted_data",
  "signature": "64-char-hex-hmac-sha256",
  "timestamp": 1234567890
}
```

**Response (200):**
```json
{
  "success": true,
  "updatedAt": "2025-12-09T12:34:56Z"
}
```

**Errors:**
- `401 Unauthorized` - Invalid signature or timestamp
- `404 Not Found` - User not registered
- `413 Payload Too Large` - Data exceeds 5MB
- `429 Too Many Requests` - Rate limit exceeded (5/hour, 20/day)

### GET /api/backup?userId=...&storageKey=...
Retrieve encrypted backup data.

**Query Parameters:**
- `userId` - Server user ID hash (64-char hex)
- `storageKey` - Storage key hash (64-char hex)

**Response (200):**
```json
{
  "data": "base64_encoded_encrypted_data",
  "updatedAt": "2025-12-09T12:34:56Z"
}
```

**Errors:**
- `404 Not Found` - Backup not found

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

**Response (200):**
```json
{
  "success": true,
  "message": "User and all associated data permanently deleted"
}
```

**Errors:**
- `401 Unauthorized` - Invalid signature, timestamp, or storage key mismatch
- `404 Not Found` - User not found

**Security:**
- Requires valid HMAC signature (proves request from official app)
- Requires valid timestamp (within 5 minutes)
- Verifies storage key matches user (proves password knowledge)
- Cascading delete removes all user data (backups, rate limits)

### GET /health
Health check endpoint for monitoring.

**Response (200):**
```json
{
  "status": "healthy",
  "database": "connected"
}
```

### GET /admin/stats?key=...
Admin endpoint for database diagnostics. Only available if `ADMIN_SECRET_KEY` is configured.

**Query Parameters:**
- `key` - Admin secret key (must match `ADMIN_SECRET_KEY` environment variable)

**Response (200):**
```json
{
  "user_count": 42,
  "backup_count": 38,
  "database_size_bytes": 1048576,
  "database_size_human": "1.00 MB"
}
```

**Errors:**
- `401 Unauthorized` - Missing or invalid admin key, or admin endpoints not enabled

**Security:**
- Endpoint is disabled unless `ADMIN_SECRET_KEY` environment variable is set
- Key is passed as query parameter for easy curl access from Fly.io SSH

## Database Schema (redb)

The server uses redb, an embedded key-value database. All records are serialized with bincode.

### Tables

```rust
// Users table: user_id (SHA-256 hash) -> UserRecord
USERS: TableDefinition<&str, &[u8]>
// UserRecord { created_at: i64 }  // Unix timestamp

// Backups table: storage_key (SHA-256 hash) -> BackupRecord
BACKUPS: TableDefinition<&str, &[u8]>
// BackupRecord { user_id, encrypted_data, created_at, updated_at }

// Rate limits table: user_id -> RateLimitRecord
RATE_LIMITS: TableDefinition<&str, &[u8]>
// RateLimitRecord { backups_this_hour, backups_today, hour_reset_at, day_reset_at }

// User backups index: user_id -> Vec<storage_key> (for cascade delete)
USER_BACKUPS: TableDefinition<&str, &[u8]>
```

## Environment Variables

Required environment variables (see `.env.example`):

```bash
# Server Configuration
SERVER_HOST=0.0.0.0
SERVER_PORT=8080

# Database (redb file path)
DATABASE_PATH=./data/dailyreps.db

# Security (MUST match client app)
APP_SECRET_KEY=your-secret-key-here-generate-with-openssl-rand-hex-32

# CORS (comma-separated allowed origins)
ALLOWED_ORIGINS=http://localhost:5173,https://dailyreps.netlify.app

# Logging
RUST_LOG=info                # debug, info, warn, error

# Admin API (optional) - enables /admin/stats endpoint
ADMIN_SECRET_KEY=your-admin-secret-key-here
```

## Security Best Practices

### Input Validation
- Always validate input sizes (prevent DoS via large payloads)
- Validate hash formats (must be valid hex strings of correct length)
- Sanitize error messages (don't leak internal details)

### Rate Limiting
- Database-backed per-user rate limiting (5/hour, 20/day)
- Return 429 Too Many Requests when exceeded

### CORS Configuration
- Explicitly whitelist allowed origins (never use `*` in production)
- Allow only necessary methods (GET, POST)
- Don't expose sensitive headers

### Database Security
- Embedded database (redb) - no external attack surface
- All user data is encrypted client-side before storage

### Error Handling
- Return generic error messages to clients
- Log detailed errors server-side only
- Never expose stack traces or internal paths
- Use structured logging with appropriate levels

### HTTPS/TLS
- Terminate TLS at load balancer (Fly.io handles this)
- Redirect HTTP to HTTPS
- Use HSTS headers

## Rust Best Practices for This Project

### Error Handling
Use `thiserror` for custom error types:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("User not found")]
    NotFound,
    #[error("Database error: {0}")]
    Database(#[from] redb::Error),
}
```

### Async Patterns
Always use `async/await` with Tokio:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Async code here
}
```

### Type Safety
Use newtype patterns for IDs:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserId(String);

impl UserId {
    pub fn new(hash: String) -> Result<Self> {
        // Validate hash format
        if hash.len() != 64 {
            anyhow::bail!("Invalid user ID hash length");
        }
        Ok(UserId(hash))
    }
}
```

### Testing
Write tests for all critical paths:

```rust
#[tokio::test]
async fn test_register_user() {
    let db = setup_test_db();
    let result = register_user(&db, "test_hash").await;
    assert!(result.is_ok());
}
```

## Deployment

### Fly.io Deployment (Recommended)

```bash
# Install flyctl
curl -L https://fly.io/install.sh | sh

# Login
fly auth login

# Create volume for database (first time)
fly volumes create dailyreps_data --region iad --size 1

# Set secrets
fly secrets set APP_SECRET_KEY=your-secret-here
fly secrets set ALLOWED_ORIGINS=https://dailyreps.netlify.app

# Deploy
fly deploy

# View logs
fly logs

# SSH into instance
fly ssh console
```

### Docker Deployment

```bash
# Build image
docker build -t dailyreps-backup-server .

# Run container
docker run -d \
  -p 8080:8080 \
  -v dailyreps_data:/data \
  -e APP_SECRET_KEY=your-secret-here \
  -e ALLOWED_ORIGINS=https://dailyreps.netlify.app \
  dailyreps-backup-server
```

## Monitoring & Observability

### Logging
- Use `tracing` crate for structured logging
- Log levels: ERROR for critical issues, WARN for important events, INFO for normal operations, DEBUG for development
- Include request IDs for tracing requests through the system

### Metrics to Track
- Request count by endpoint
- Response times (p50, p95, p99)
- Error rates by endpoint
- Database file size

### Health Checks
- Database connectivity check via `/health` endpoint
- Response time check
- Volume storage check (for container deployments)

## Code Quality Standards

### Before Committing

```bash
# Run full quality check
cargo fmt && cargo clippy -- -D warnings && cargo test
```

All three must pass:
1. **Format**: Code must be formatted with `cargo fmt`
2. **Lint**: No clippy warnings allowed
3. **Tests**: All tests must pass

### Code Review Checklist
- [ ] All functions have doc comments
- [ ] Error handling uses `Result<T>` with proper context
- [ ] Input validation on all user inputs
- [ ] Tests cover happy path and error cases
- [ ] No unwrap() or expect() in production code paths
- [ ] Security implications considered

## Learning Resources for Rust

Since you're new to Rust, here are key concepts to understand:

### Ownership & Borrowing
- Each value has one owner
- Use `&` for immutable borrows, `&mut` for mutable borrows
- Values are dropped when owner goes out of scope

### Error Handling
- `Result<T, E>` for operations that can fail
- `?` operator for propagating errors
- `thiserror` for custom error types

### Async/Await
- `async fn` returns a `Future`
- `.await` to wait for a future to complete
- Tokio runtime executes async tasks

### Traits
- Like interfaces in other languages
- `#[derive(Debug, Clone)]` auto-implements common traits
- Custom traits for shared behavior

## Adding New Features

When adding new endpoints or functionality:

1. **Plan the change** - Update IMPLEMENTATION_PLAN.md
2. **Write the types** - Define models in `src/models/`
3. **Update tables** - Add table definitions in `src/db/tables.rs` if needed
4. **Implement route** - Add handler in `src/routes/`
5. **Add tests** - Cover happy path and errors
6. **Update docs** - Document API endpoint in this file
7. **Run quality checks** - `cargo fmt && cargo clippy && cargo test`

## Git Commits

**IMPORTANT**: Never commit changes automatically unless explicitly requested by the user:

1. **Do NOT commit** after completing work - Wait for the user to explicitly ask
2. **Only commit when requested** - User will say "commit" or "commit changes" when ready
3. **Exception**: When the user asks you to complete a task AND commit in the same request (e.g., "fix this and commit"), then you may commit

This gives the user control over when changes are committed and allows them to review the changes first.
