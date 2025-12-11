# DailyReps Backup Server

A secure Rust backend service for the DailyReps workout tracking app, providing zero-knowledge encrypted backup storage with comprehensive anti-griefing measures.

![Rust](https://img.shields.io/badge/Rust-1.91-orange?style=flat&logo=rust)
![Axum](https://img.shields.io/badge/Axum-0.7-blue?style=flat)
![redb](https://img.shields.io/badge/redb-2.x-green?style=flat)
![License](https://img.shields.io/badge/license-MIT-blue)

## Overview

This server stores encrypted workout data from DailyReps clients using a zero-knowledge architecture - the server never has access to plaintext data. All encryption and decryption happens client-side.

### Key Features

- **Zero-knowledge encryption** - Server stores only encrypted blobs
- **HMAC signature verification** - Ensures data comes from official app
- **Timestamp validation** - Prevents replay attacks (5-minute window)
- **Rate limiting** - Database-backed limits (5/hour, 20/day per user)
- **Size limits** - 5MB maximum payload size
- **Envelope validation** - Verifies backup format and entropy
- **Complete deletion** - Users can permanently delete all their data
- **Embedded database** - No external dependencies (redb)

## Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         DailyReps Client                            │
│  ┌───────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │ Web Crypto API│  │ HMAC Signing │  │ Key Derivation           │  │
│  │ AES-GCM       │  │ SHA-256      │  │ PBKDF2(password,username)│  │
│  └───────┬───────┘  └──────┬───────┘  └──────────────────────────┘  │
│          │                 │                                        │
│          ▼                 ▼                                        │
│  ┌─────────────────────────────────────┐                            │
│  │ Encrypted Backup Envelope           │                            │
│  │ { appId, encrypted: base64(...) }   │                            │
│  └─────────────────┬───────────────────┘                            │
└────────────────────│────────────────────────────────────────────────┘
                     │ HTTPS
                     ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Backup Server (Rust/Axum)                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                    Security Layers                          │    │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌───────┐  │    │
│  │  │  Size   │ │  Rate   │ │  HMAC   │ │Timestamp│ │Entropy│  │    │
│  │  │ Limits  │ │ Limits  │ │  Verify │ │ Check   │ │ Check │  │    │
│  │  │  5MB    │ │ 5/hr    │ │ SHA-256 │ │ ±5 min  │ │ >0.75 │  │    │
│  │  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └───┬───┘  │    │
│  │       └───────────┴───────────┴───────────┴─────────┘       │    │
│  └─────────────────────────────┬───────────────────────────────┘    │
│                                │                                    │
│  ┌─────────────────────────────▼───────────────────────────────┐    │
│  │                    redb (Embedded Database)                 │    │
│  │  ┌──────────┐ ┌──────────┐ ┌────────────┐ ┌──────────────┐  │    │
│  │  │  users   │ │ backups  │ │rate_limits │ │ user_backups │  │    │
│  │  │  table   │ │  table   │ │   table    │ │    index     │  │    │
│  │  └──────────┘ └──────────┘ └────────────┘ └──────────────┘  │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

### Zero-Knowledge Design

The server never sees plaintext data or encryption keys:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Client-Side Key Derivation                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   username ──┬──► SHA-256 ──────────────────► serverUserId      │
│              │                                (sent to server)  │
│              │                                                  │
│   password ──┼──► SHA-256(serverUserId + password) ► storageKey │
│              │                                (sent to server)  │
│              │                                                  │
│              └──► PBKDF2(password, username) ──► encryptionKey  │
│                                                 (NEVER sent)    │
│                                                                 │
│   data ──────────► AES-GCM(data, encryptionKey) ► encrypted     │
│                                                   (sent to      │
│                                                    server)      │
└─────────────────────────────────────────────────────────────────┘
```

### Request Flow

```
Client Request                     Server Processing
     │                                   │
     │  POST /api/backup                 │
     │  {                                │
     │    userId: "abc123...",           │
     │    storageKey: "def456...",   ───►│──► 1. Verify HMAC signature
     │    data: "{appId:...,             │
     │           encrypted:...}",        │──► 2. Validate timestamp (±5 min)
     │    signature: "...",              │
     │    timestamp: 1234567890          │──► 3. Check size (≤5MB)
     │  }                                │
     │                                   │──► 4. Validate userId/storageKey format
     │                                   │
     │                                   │──► 5. Parse envelope, check appId
     │                                   │
     │                                   │──► 6. Calculate entropy (≥0.75)
     │                                   │
     │                                   │──► 7. Verify user exists
     │                                   │
     │                                   │──► 8. Check rate limits (5/hr, 20/day)
     │                                   │
     │                                   │──► 9. Store encrypted blob
     │                                   │
     │  200 OK ◄─────────────────────────│
     │  { success: true }                │
     ▼                                   ▼
```

### Database Schema (redb)

```
┌─────────────────────────────────────────────────────────────────┐
│                         redb Tables                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  USERS: TableDefinition<&str, &[u8]>                            │
│  ┌─────────────────┬────────────────────────────────────────┐   │
│  │ Key (user_id)   │ Value (UserRecord serialized)          │   │
│  │ "a1b2c3..."     │ { created_at: 1702134000 }             │   │
│  └─────────────────┴────────────────────────────────────────┘   │
│                                                                 │
│  BACKUPS: TableDefinition<&str, &[u8]>                          │
│  ┌─────────────────┬────────────────────────────────────────┐   │
│  │ Key (storage_key)│ Value (BackupRecord serialized)       │   │
│  │ "d4e5f6..."     │ { user_id, encrypted_data,             │   │
│  │                 │   created_at, updated_at }             │   │
│  └─────────────────┴────────────────────────────────────────┘   │
│                                                                 │
│  RATE_LIMITS: TableDefinition<&str, &[u8]>                      │
│  ┌─────────────────┬────────────────────────────────────────┐   │
│  │ Key (user_id)   │ Value (RateLimitRecord serialized)     │   │
│  │ "a1b2c3..."     │ { backups_this_hour, backups_today,    │   │
│  │                 │   hour_reset_at, day_reset_at }        │   │
│  └─────────────────┴────────────────────────────────────────┘   │
│                                                                 │
│  USER_BACKUPS: TableDefinition<&str, &[u8]>                     │
│  ┌─────────────────┬────────────────────────────────────────┐   │
│  │ Key (user_id)   │ Value (Vec<storage_key> serialized)    │   │
│  │ "a1b2c3..."     │ ["d4e5f6...", "g7h8i9..."]             │   │
│  └─────────────────┴────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Security Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                    Anti-Griefing Stack                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Layer 5: Envelope Validation + Entropy Check                   │
│  ├── Requires { appId: "dailyreps-app", encrypted: "..." }      │
│  ├── Rejects non-JSON envelope formats                          │
│  └── Flags entropy < 0.75 (likely unencrypted data)             │
│           │                                                     │
│  Layer 4: Timestamp Validation                                  │
│  ├── Request timestamp must be within ±5 minutes                │
│  └── Prevents replay attacks                                    │
│           │                                                     │
│  Layer 3: HMAC Signature Verification                           │
│  ├── HMAC-SHA256(data, APP_SECRET_KEY)                          │
│  └── Proves request came from official app                      │
│           │                                                     │
│  Layer 2: Rate Limiting                                         │
│  ├── 5 backups per hour per user                                │
│  ├── 20 backups per day per user                                │
│  └── Tracked in database, auto-resets                           │
│           │                                                     │
│  Layer 1: Size Limits                                           │
│  ├── 5MB hard limit (413 error)                                 │
│  └── 1MB warning threshold (logged)                             │
│           │                                                     │
│           ▼                                                     │
│      [Request Accepted]                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Technology Stack

- **Framework**: [Axum 0.7](https://github.com/tokio-rs/axum) - Fast, ergonomic web framework
- **Runtime**: [Tokio](https://tokio.rs/) - Async runtime
- **Database**: [redb 2.x](https://github.com/cberner/redb) - Embedded key-value store
- **Serialization**: [bincode](https://github.com/bincode-org/bincode) - Binary encoding
- **Security**: HMAC-SHA256 signatures, timestamp validation, entropy analysis
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
  "data": "{\"appId\":\"dailyreps-app\",\"encrypted\":\"base64...\"}",
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
- `400 Bad Request` - Invalid envelope format or suspicious entropy
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
  "data": "{\"appId\":\"dailyreps-app\",\"encrypted\":\"base64...\"}",
  "updatedAt": "2025-01-01T12:00:00Z"
}
```

**Errors:**
- `404 Not Found` - No backup found for this user

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
  "status": "healthy",
  "database": "connected"
}
```

## Setup & Installation

### Prerequisites

- Rust 1.91+ (install via [rustup](https://rustup.rs/))

### Configuration

Create `.env` file:

```bash
# Server Configuration
SERVER_HOST=0.0.0.0
SERVER_PORT=8080

# Database (redb file path)
DATABASE_PATH=./data/dailyreps.db

# Security (MUST match client app)
APP_SECRET_KEY=your-secret-key-here-generate-with-openssl-rand-hex-32

# CORS (your client domain)
ALLOWED_ORIGINS=https://your-app.netlify.app

# Environment
ENVIRONMENT=production
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
│   ├── main.rs              # Server entry point
│   ├── config.rs            # Environment configuration
│   ├── constants.rs         # Limits & security constants
│   ├── error.rs             # Custom error types
│   ├── security.rs          # HMAC, timestamp, entropy validation
│   ├── models/
│   │   ├── mod.rs
│   │   ├── user.rs          # User model
│   │   ├── backup.rs        # Backup model
│   │   └── rate_limit.rs    # Rate limit tracking
│   ├── db/
│   │   ├── mod.rs           # Database init
│   │   └── tables.rs        # Table definitions
│   └── routes/
│       ├── mod.rs
│       ├── health.rs
│       ├── register.rs
│       ├── backup.rs
│       └── delete.rs
├── Cargo.toml
├── Dockerfile
├── fly.toml                 # Fly.io deployment config
├── CLAUDE.md                # Development guide
└── IMPLEMENTATION_PLAN.md   # Implementation status
```

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Full quality check
cargo fmt && cargo clippy -- -D warnings && cargo test
```

## Deployment

### Fly.io (Recommended)

```bash
# Install flyctl
curl -L https://fly.io/install.sh | sh

# Login
fly auth login

# Create volume for database
fly volumes create dailyreps_data --region iad --size 1

# Set secrets
fly secrets set APP_SECRET_KEY=your-secret-here
fly secrets set ALLOWED_ORIGINS=https://your-app.netlify.app

# Deploy
fly deploy
```

### Docker

```bash
# Build
docker build -t dailyreps-backup-server .

# Run
docker run -d \
  -p 8080:8080 \
  -v dailyreps_data:/data \
  -e APP_SECRET_KEY=xxx \
  -e ALLOWED_ORIGINS=https://your-app.com \
  dailyreps-backup-server
```

## Security Considerations

### What the Server Can See
- Encrypted data blobs (AES-GCM ciphertext)
- SHA-256 hashes (userIds, storageKeys)
- Timestamps and rate limit counters
- Request metadata (timing, size)

### What the Server CANNOT See
- Plaintext workout data
- Encryption keys (PBKDF2 derived client-side)
- Original usernames
- Passwords
- Any unencrypted user information

### Threat Model

**Protected Against:**
- Unauthorized data access (server admin can't read data)
- Replay attacks (timestamp validation)
- Fake clients (HMAC signatures + envelope validation)
- Storage abuse (size limits, rate limiting, entropy check)
- User enumeration (consistent error messages)

**Not Protected Against:**
- Client compromise (if client is hacked, keys are exposed)
- Weak passwords (use strong passwords!)

## License

MIT License - see LICENSE file for details

---

**Built for privacy-conscious fitness enthusiasts**
