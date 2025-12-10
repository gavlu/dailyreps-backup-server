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
- **Axum** - Type-safe, ergonomic web framework built on Tokio
- **Tower** - Middleware for CORS, rate limiting, tracing
- **Tokio** - Async runtime

### Database
- **PostgreSQL** - Primary database for user records and encrypted backups
- **SQLx** - Compile-time checked SQL queries with async support
- Migrations managed via `sqlx-cli`

### Security & Cryptography
- **sha2** - SHA-256 hashing for user IDs and storage keys
- **argon2** - Password hashing (if needed for future features)
- **tower-http** - CORS middleware, request logging
- **tower-governor** - Rate limiting per IP/user

### Serialization & Configuration
- **serde** + **serde_json** - JSON serialization/deserialization
- **dotenvy** - Environment variable management
- **config** - Application configuration management

### Development & Testing
- **tokio-test** - Testing utilities for async code
- **reqwest** - HTTP client for integration tests
- **testcontainers** - Docker containers for database testing

## Project Structure

```
dailyreps-backup-server/
├── src/
│   ├── main.rs              # Application entry point, server setup
│   ├── config.rs            # Configuration management
│   ├── routes/
│   │   ├── mod.rs           # Route module exports
│   │   ├── health.rs        # Health check endpoint
│   │   ├── register.rs      # User registration
│   │   ├── backup.rs        # Backup storage/retrieval
│   │   └── auth.rs          # Authentication helpers
│   ├── models/
│   │   ├── mod.rs           # Model exports
│   │   ├── user.rs          # User model
│   │   └── backup.rs        # Backup model
│   ├── db/
│   │   ├── mod.rs           # Database module exports
│   │   └── pool.rs          # Database connection pool
│   ├── middleware/
│   │   ├── mod.rs           # Middleware exports
│   │   ├── cors.rs          # CORS configuration
│   │   ├── rate_limit.rs    # Rate limiting
│   │   └── logging.rs       # Request/response logging
│   └── error.rs             # Error types and handling
├── migrations/              # SQLx database migrations
├── tests/
│   ├── integration/         # Integration tests
│   └── common/              # Shared test utilities
├── Cargo.toml               # Dependencies and metadata
├── .env.example             # Example environment variables
├── Dockerfile               # Production container image
└── fly.toml                 # Fly.io deployment configuration
```

## Development Commands

### Running the application

```bash
# Start PostgreSQL (via Docker)
docker run -d --name dailyreps-postgres \
  -e POSTGRES_PASSWORD=dev_password \
  -e POSTGRES_DB=dailyreps_backup \
  -p 5432:5432 \
  postgres:16

# Run database migrations
sqlx migrate run

# Start development server
cargo run

# Start with auto-reload
cargo watch -x run
```

### Database Management

```bash
# Install sqlx-cli
cargo install sqlx-cli --no-default-features --features postgres

# Create a new migration
sqlx migrate add <migration_name>

# Run migrations
sqlx migrate run

# Revert last migration
sqlx migrate revert

# Check database schema matches SQLx queries at compile time
cargo sqlx prepare
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
  "userId": "sha256_hash_of_email"
}
```

**Response (200):**
```json
{
  "success": true
}
```

**Response (409 Conflict):**
```json
{
  "error": "User already exists"
}
```

### POST /api/backup
Store or update encrypted backup data.

**Request:**
```json
{
  "userId": "sha256_hash_of_email",
  "storageKey": "sha256_hash_of_userid_plus_password",
  "data": "base64_encoded_encrypted_data"
}
```

**Response (200):**
```json
{
  "success": true,
  "updatedAt": "2025-12-09T12:34:56Z"
}
```

### GET /api/backup?userId=...&storageKey=...
Retrieve encrypted backup data.

**Query Parameters:**
- `userId` - Server user ID hash
- `storageKey` - Storage key hash

**Response (200):**
```json
{
  "data": "base64_encoded_encrypted_data",
  "updatedAt": "2025-12-09T12:34:56Z"
}
```

**Response (404):**
```json
{
  "error": "Backup not found"
}
```

### DELETE /api/user
Permanently delete user and all associated data.

**Request:**
```json
{
  "userId": "sha256_hash_of_username",
  "storageKey": "sha256_hash_of_userid_plus_password",
  "signature": "hmac_sha256_signature",
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

**Response (401):**
```json
{
  "error": "Invalid signature - data must come from official app"
}
```

**Response (404):**
```json
{
  "error": "User not found"
}
```

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
  "database": "connected",
  "version": "0.1.0"
}
```

## Database Schema

### users table
```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,              -- SHA-256 hash of email
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_created_at ON users(created_at);
```

### backups table
```sql
CREATE TABLE backups (
    storage_key TEXT PRIMARY KEY,     -- SHA-256 hash of userId + password
    user_id TEXT NOT NULL,            -- References users(id)
    encrypted_data TEXT NOT NULL,     -- Base64 encoded encrypted blob
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_backups_user_id ON backups(user_id);
CREATE INDEX idx_backups_updated_at ON backups(updated_at);
```

## Environment Variables

Required environment variables (see `.env.example`):

```bash
# Server Configuration
SERVER_HOST=0.0.0.0
SERVER_PORT=8080

# Database
DATABASE_URL=postgresql://user:password@localhost:5432/dailyreps_backup

# CORS (comma-separated allowed origins)
ALLOWED_ORIGINS=http://localhost:5173,https://dailyreps.netlify.app

# Rate Limiting
RATE_LIMIT_REQUESTS=100      # Requests per window
RATE_LIMIT_WINDOW_SECS=60    # Window duration in seconds

# Logging
RUST_LOG=info                # debug, info, warn, error
```

## Security Best Practices

### Input Validation
- Always validate input sizes (prevent DoS via large payloads)
- Validate hash formats (must be valid hex strings of correct length)
- Sanitize error messages (don't leak internal details)

### Rate Limiting
- Per-IP rate limiting on all endpoints
- Stricter limits on registration endpoint
- Return 429 Too Many Requests with Retry-After header

### CORS Configuration
- Explicitly whitelist allowed origins (never use `*` in production)
- Allow only necessary methods (GET, POST)
- Don't expose sensitive headers

### Database Security
- Use parameterized queries (SQLx provides this by default)
- Never concatenate user input into SQL
- Use connection pooling with limits
- Enable SSL for database connections in production

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
Use `anyhow` for application errors with context:

```rust
use anyhow::{Context, Result};

async fn do_something() -> Result<String> {
    let data = fetch_data()
        .await
        .context("Failed to fetch data from database")?;
    Ok(data)
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

### Database Queries
Use SQLx with compile-time checked queries:

```rust
let user = sqlx::query_as!(
    User,
    r#"SELECT id, created_at FROM users WHERE id = $1"#,
    user_id
)
.fetch_optional(&pool)
.await?;
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
    let pool = setup_test_db().await;
    let result = register_user(&pool, "test_hash").await;
    assert!(result.is_ok());
}
```

## Deployment

### Fly.io Deployment (Recommended)

```bash
# Install flyctl
curl -L https://fly.io/install.sh | sh

# Login
flyctl auth login

# Launch app (first time)
flyctl launch

# Deploy
flyctl deploy

# Set environment variables
flyctl secrets set DATABASE_URL=postgresql://...
flyctl secrets set ALLOWED_ORIGINS=https://dailyreps.netlify.app

# View logs
flyctl logs

# SSH into instance
flyctl ssh console
```

### Docker Deployment

```bash
# Build image
docker build -t dailyreps-backup-server .

# Run container
docker run -d \
  -p 8080:8080 \
  -e DATABASE_URL=postgresql://... \
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
- Database connection pool usage
- Active connections

### Health Checks
- Database connectivity check
- Response time check
- Disk space check (for container deployments)

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
- [ ] Database queries are parameterized
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
- `anyhow` for application-level errors

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
3. **Create migration** - Add database changes in `migrations/`
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
