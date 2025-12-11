# Implementation Plan: DailyReps Encrypted Backup Server

**Status**: Server Implementation Complete - Ready for Deployment
**Last Updated**: 2025-12-10
**Target**: Production-Ready Rust API Server

## Table of Contents

1. [Project Goals](#project-goals)
2. [Architecture Overview](#architecture-overview)
3. [Implementation Phases](#implementation-phases)
4. [Security Considerations](#security-considerations)
5. [Client-Server Integration](#client-server-integration)
6. [Deployment Strategy](#deployment-strategy)
7. [Testing Strategy](#testing-strategy)
8. [Future Enhancements](#future-enhancements)

---

## Project Goals

### Primary Objectives

1. **Zero-Knowledge Encrypted Backups**: Provide secure backup storage where the server cannot access user data
2. **Production-Ready Security**: Implement industry-standard security practices from day one
3. **Reliable Data Storage**: Ensure user backups are safely stored and retrievable
4. **Seamless Integration**: Work smoothly with existing SvelteKit frontend on Netlify

### Non-Goals

- User account recovery (by design - lost credentials = lost data)
- Server-side data analysis or processing
- Real-time sync (backup is on-demand)
- Multi-device session management

---

## Architecture Overview

### Cryptographic Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLIENT (Browser)                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  User enters: username + password                                │
│  (username can be any text string)                               │
│                                                                   │
│  1. serverUserId = SHA256(username.toLowerCase())                │
│     └─> Sent to server for registration/auth                     │
│                                                                   │
│  2. encryptionKey = PBKDF2(password, username, 100000 iterations)│
│     └─> NEVER sent to server, used for AES-GCM encryption        │
│                                                                   │
│  3. storageKey = SHA256(serverUserId + password)                 │
│     └─> Sent to server to identify where data is stored          │
│                                                                   │
│  4. encryptedData = AES-GCM(userData, encryptionKey, nonce)      │
│     └─> Sent to server as opaque blob                            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ HTTPS
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      RUST SERVER (Axum)                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  POST /api/register                                              │
│    ├─> Receives: serverUserId (hash)                             │
│    ├─> Checks: Does user already exist?                          │
│    ├─> Stores: { id: serverUserId, created_at }                  │
│    └─> Knows: NOTHING about actual email or password             │
│                                                                   │
│  POST /api/backup                                                │
│    ├─> Receives: userId, storageKey, encryptedData               │
│    ├─> Validates: userId exists in users table                   │
│    ├─> Stores: { storage_key, encrypted_data, user_id }          │
│    └─> Knows: NOTHING about actual data contents                 │
│                                                                   │
│  GET /api/backup?userId=X&storageKey=Y                           │
│    ├─> Receives: userId, storageKey (both hashes)                │
│    ├─> Fetches: encrypted_data from database                     │
│    └─> Returns: opaque encrypted blob to client                  │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    redb EMBEDDED DATABASE                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  USERS table: user_id -> UserRecord (bincode)                   │
│    └─ created_at (Unix timestamp)                                │
│                                                                   │
│  BACKUPS table: storage_key -> BackupRecord (bincode)           │
│    ├─ user_id: SHA-256 hash                                      │
│    ├─ encrypted_data: base64 encrypted blob                      │
│    ├─ created_at (Unix timestamp)                                │
│    └─ updated_at (Unix timestamp)                                │
│                                                                   │
│  RATE_LIMITS table: user_id -> RateLimitRecord (bincode)        │
│    ├─ backups_this_hour, backups_today                           │
│    └─ hour_reset_at, day_reset_at (Unix timestamps)              │
│                                                                   │
│  USER_BACKUPS table: user_id -> Vec<storage_key>                │
│    └─ Index for cascade delete                                   │
│                                                                   │
│  Database contains ONLY:                                         │
│    ✓ Hashed identifiers                                          │
│    ✓ Encrypted blobs (unreadable)                                │
│    ✓ Timestamps                                                  │
│                                                                   │
│  Database CANNOT determine:                                      │
│    ✗ User usernames or passwords                                 │
│    ✗ Actual user data                                            │
│    ✗ Which storage_key belongs to which user                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow: Registration

```
┌──────────┐                  ┌──────────┐                 ┌──────────┐
│  Client  │                  │  Server  │                 │    DB    │
└────┬─────┘                  └────┬─────┘                 └────┬─────┘
     │                             │                            │
     │ 1. User enters username     │                            │
     ├─────────────────────────────┤                            │
     │ 2. Calculate serverUserId   │                            │
     │    = SHA256(username)       │                            │
     │                             │                            │
     │ 3. POST /api/register       │                            │
     │    { userId: "a4f3e21..." } │                            │
     ├────────────────────────────>│                            │
     │                             │ 4. Check if exists         │
     │                             ├───────────────────────────>│
     │                             │                            │
     │                             │ 5. Return existing check   │
     │                             │<───────────────────────────┤
     │                             │                            │
     │                             │ 6. If new, INSERT user     │
     │                             ├───────────────────────────>│
     │                             │                            │
     │ 7. Response: { success }    │                            │
     │<────────────────────────────┤                            │
     │                             │                            │
     │ 8. Store username + password│                            │
     │    locally for future use   │                            │
     │                             │                            │
```

### Data Flow: Backup Save

```
┌──────────┐                  ┌──────────┐                 ┌──────────┐
│  Client  │                  │  Server  │                 │    DB    │
└────┬─────┘                  └────┬─────┘                 └────┬─────┘
     │                             │                            │
     │ 1. User triggers backup     │                            │
     ├─────────────────────────────┤                            │
     │ 2. Get data from localStorage│                           │
     │                             │                            │
     │ 3. Derive encryptionKey     │                            │
     │    from username + password │                            │
     │    (PBKDF2, never sent!)    │                            │
     │                             │                            │
     │ 4. Encrypt data with        │                            │
     │    AES-GCM(data, key)       │                            │
     │                             │                            │
     │ 5. Calculate keys:          │                            │
     │    serverUserId = SHA256(username)                       │
     │    storageKey = SHA256(userId + password)                │
     │                             │                            │
     │ 6. POST /api/backup         │                            │
     │    {                        │                            │
     │      userId: "a4f3e21...",  │                            │
     │      storageKey: "9b2c8...",│                            │
     │      data: "encrypted_blob" │                            │
     │    }                        │                            │
     ├────────────────────────────>│                            │
     │                             │ 7. Verify userId exists    │
     │                             ├───────────────────────────>│
     │                             │                            │
     │                             │ 8. Upsert backup           │
     │                             │    (insert or update)      │
     │                             ├───────────────────────────>│
     │                             │                            │
     │ 9. Response:                │                            │
     │    { success, updatedAt }   │                            │
     │<────────────────────────────┤                            │
     │                             │                            │
```

### Data Flow: Backup Retrieve

```
┌──────────┐                  ┌──────────┐                 ┌──────────┐
│  Client  │                  │  Server  │                 │    DB    │
└────┬─────┘                  └────┬─────┘                 └────┬─────┘
     │                             │                            │
     │ 1. User wants to restore    │                            │
     │    (new device or data loss)│                            │
     ├─────────────────────────────┤                            │
     │ 2. User enters username + pw│                            │
     │                             │                            │
     │ 3. Calculate keys:          │                            │
     │    serverUserId = SHA256(username)                       │
     │    storageKey = SHA256(userId + password)                │
     │                             │                            │
     │ 4. GET /api/backup?         │                            │
     │    userId=a4f3e21...&       │                            │
     │    storageKey=9b2c8...      │                            │
     ├────────────────────────────>│                            │
     │                             │ 5. Fetch backup            │
     │                             ├───────────────────────────>│
     │                             │                            │
     │                             │ 6. Return encrypted data   │
     │                             │<───────────────────────────┤
     │                             │                            │
     │ 7. Response:                │                            │
     │    { data, updatedAt }      │                            │
     │<────────────────────────────┤                            │
     │                             │                            │
     │ 8. Derive encryptionKey     │                            │
     │    from username + password │                            │
     │                             │                            │
     │ 9. Decrypt data with        │                            │
     │    AES-GCM(encryptedData,   │                            │
     │            encryptionKey)   │                            │
     │                             │                            │
     │ 10. Save to localStorage    │                            │
     │                             │                            │
```

---

## Implementation Phases

### Phase 1: Project Setup & Infrastructure ✅ COMPLETE

**Status**: Complete

**Tasks**:
- [x] Initialize Rust project with Cargo
- [x] Create CLAUDE.md with project guidelines
- [x] Create IMPLEMENTATION_PLAN.md (this document)
- [x] Set up Cargo.toml with all dependencies
- [x] Create project directory structure
- [x] Set up .env.example file
- [x] Initialize git repository
- [x] Create .gitignore for Rust projects

**Dependencies (Cargo.toml)**:
```toml
[dependencies]
# Web framework
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# Database - embedded key-value store
redb = "2"
bincode = "1.3"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Security & Crypto
sha2 = "0.10"
hmac = "0.12"
hex = "0.4"

# Configuration
dotenvy = "0.15"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Date/time
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
reqwest = "0.12"
tokio-test = "0.4"
tempfile = "3"
```

**Deliverable**: ✅ Rust project initialized with proper structure and dependencies

---

### Phase 2: Database Setup ✅ COMPLETE

**Status**: Complete (migrated from PostgreSQL to redb)

**Tasks**:
- [x] ~~Install PostgreSQL~~ → Using redb embedded database instead
- [x] Create table definitions for users, backups, rate_limits, user_backups
- [x] Create database models with bincode serialization
- [x] Set up database initialization in code
- [x] Create RateLimitRecord model with check_and_increment logic

**Database Schema (redb tables in `src/db/tables.rs`)**:

```rust
// Users table: user_id (SHA-256 hash) -> UserRecord (serialized)
pub const USERS: TableDefinition<&str, &[u8]> = TableDefinition::new("users");

// Backups table: storage_key (SHA-256 hash) -> BackupRecord (serialized)
pub const BACKUPS: TableDefinition<&str, &[u8]> = TableDefinition::new("backups");

// Rate limits table: user_id -> RateLimitRecord (serialized)
pub const RATE_LIMITS: TableDefinition<&str, &[u8]> = TableDefinition::new("rate_limits");

// User backups index: user_id -> Vec<storage_key> (for cascade delete)
pub const USER_BACKUPS: TableDefinition<&str, &[u8]> = TableDefinition::new("user_backups");
```

**Benefits of redb over PostgreSQL**:
- Zero external dependencies (no PostgreSQL server needed)
- Reduced latency (in-process vs network)
- Lower hosting costs (~$2/month savings)
- Simpler deployment (single binary + volume)

**Deliverable**: ✅ Database tables created with automatic initialization

---

### Phase 3: Core API Implementation ✅ COMPLETE

**Status**: Complete

#### 3.1: Basic Server Setup ✅

**Tasks**:
- [x] Create main.rs with Axum server setup
- [x] Configure Tokio runtime
- [x] Set up basic routing
- [x] Add health check endpoint
- [x] Test server starts and responds

#### 3.2: User Registration Endpoint ✅

**Tasks**:
- [x] Create POST /api/register route
- [x] Validate userId format (must be 64-char hex string)
- [x] Check for existing user in database
- [x] Insert new user if doesn't exist
- [x] Return appropriate responses (200 success, 409 conflict)
- [x] Add error handling

#### 3.3: Backup Storage Endpoint ✅

**Tasks**:
- [x] Create POST /api/backup route
- [x] Validate userId, storageKey, and data fields
- [x] Verify userId exists in users table
- [x] Validate encrypted data is valid base64
- [x] Upsert backup (insert or update if exists)
- [x] Return success with timestamp
- [x] Add error handling

#### 3.4: Backup Retrieval Endpoint ✅

**Tasks**:
- [x] Create GET /api/backup route
- [x] Extract userId and storageKey from query params
- [x] Validate parameter formats
- [x] Fetch backup from database
- [x] Return 404 if not found
- [x] Return encrypted data with metadata
- [x] Add error handling

#### 3.5: User Deletion Endpoint ✅

**Tasks**:
- [x] Create DELETE /api/user route
- [x] Validate userId and storageKey formats
- [x] Verify HMAC signature
- [x] Validate timestamp (prevent replay attacks)
- [x] Cascade delete all user data (backups, rate_limits, user_backups index)
- [x] Return success response

**Deliverable**: ✅ All four core endpoints implemented

---

### Phase 4: Security Hardening ✅ COMPLETE

**Status**: Complete

#### 4.1: CORS Configuration ✅

**Tasks**:
- [x] Configure tower-http CORS middleware
- [x] Whitelist only allowed origins (from env var)
- [x] Set allowed methods (GET, POST, DELETE)
- [x] Set allowed headers

#### 4.2: Rate Limiting ✅

**Tasks**:
- [x] Database-backed rate limiting (5/hour, 20/day per user)
- [x] Rate limit counters auto-reset on time window expiry
- [x] Return 429 Too Many Requests when exceeded

#### 4.3: Input Validation ✅

**Tasks**:
- [x] Create validation functions for all inputs
- [x] Validate hash formats (64 hex chars for SHA-256)
- [x] Validate data size limits (5MB max, 1MB warning threshold)
- [x] Sanitize error messages (no internal details leaked)

#### 4.4: HMAC Signature Verification ✅

**Tasks**:
- [x] Implement HMAC-SHA256 signature verification
- [x] Verify signatures on backup store and user delete endpoints
- [x] Timestamp validation to prevent replay attacks (5 min window)

#### 4.5: Logging & Monitoring ✅

**Tasks**:
- [x] Set up tracing-subscriber
- [x] Log all requests with context
- [x] Log errors with context (but not to clients)
- [x] Configure log levels via environment

**Deliverable**: ✅ Production-ready security features implemented

---

### Phase 5: Testing & Quality Assurance 🔄 IN PROGRESS

**Status**: Partial - Build compiles, tests pending

**Tasks**:
- [x] Code compiles with `cargo check`
- [x] No clippy warnings
- [x] Code formatted with `cargo fmt`
- [ ] Write unit tests for all validation functions
- [ ] Write integration tests for each endpoint
- [ ] Test error cases (invalid inputs, missing data, etc.)
- [ ] Test rate limiting behavior
- [ ] Achieve >80% code coverage

**Existing unit tests**:
- `src/models/user.rs` - User ID validation tests
- `src/models/backup.rs` - Storage key and encrypted data validation tests
- `src/models/rate_limit.rs` - Rate limit check_and_increment tests

**Deliverable**: Comprehensive test suite with high coverage

---

### Phase 6: Deployment Preparation ✅ COMPLETE

**Status**: Complete - Ready for deployment

#### 6.1: Docker Configuration ✅

**Tasks**:
- [x] Create Dockerfile for production build
- [x] Use multi-stage build (minimize image size)
- [x] Configure /data volume for redb database

**Dockerfile**:
```dockerfile
# Build stage
FROM rust:1.83-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/dailyreps-backup-server /usr/local/bin/
RUN mkdir -p /data
ENV DATABASE_PATH=/data/dailyreps.db
EXPOSE 8080
CMD ["dailyreps-backup-server"]
```

#### 6.2: Fly.io Setup ✅

**Tasks**:
- [x] Configure fly.toml with volume mount
- [x] Set environment variables for redb

**fly.toml**:
```toml
app = "dailyreps-backup-server"
primary_region = "iad"

[env]
  DATABASE_PATH = "/data/dailyreps.db"
  SERVER_HOST = "0.0.0.0"
  SERVER_PORT = "8080"
  RUST_LOG = "info"

[http_service]
  internal_port = 8080
  force_https = true
  auto_stop_machines = "stop"
  auto_start_machines = true

[[vm]]
  memory = "256mb"
  cpu_kind = "shared"
  cpus = 1

[[mounts]]
  source = "dailyreps_data"
  destination = "/data"
```

**Deployment Commands**:
```bash
fly volumes create dailyreps_data --region iad --size 1
fly secrets set APP_SECRET_KEY=<your-secret>
fly secrets set ALLOWED_ORIGINS=https://dailyreps.netlify.app
fly deploy
```

**Deliverable**: ✅ Deployment configuration ready

---

### Phase 7: Client Integration

**Status**: Pending

**Tasks**:
- [ ] Design client-side backup UI
- [ ] Implement Web Crypto API encryption
- [ ] Create backup registration flow
- [ ] Create backup save flow
- [ ] Create backup restore flow
- [ ] Add loading states and error handling
- [ ] Test full end-to-end flow
- [ ] Update SvelteKit app documentation

**Client-side implementation** (to be added to SvelteKit app):

```typescript
// src/lib/backup.ts

interface BackupCredentials {
  username: string
  password: string
}

// Calculate server user ID from username
async function getServerUserId(username: string): Promise<string> {
  const normalized = username.toLowerCase()
  const encoder = new TextEncoder()
  const data = encoder.encode(normalized)
  const hashBuffer = await crypto.subtle.digest('SHA-256', data)
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('')
}

// Derive encryption key from password and username
async function deriveEncryptionKey(
  password: string,
  username: string
): Promise<CryptoKey> {
  const encoder = new TextEncoder()
  const passwordData = encoder.encode(password)
  const salt = encoder.encode(username.toLowerCase())

  // Import password as key material
  const keyMaterial = await crypto.subtle.importKey(
    'raw',
    passwordData,
    'PBKDF2',
    false,
    ['deriveKey']
  )

  // Derive AES-GCM key
  return crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt: salt,
      iterations: 100000,
      hash: 'SHA-256'
    },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  )
}

// Calculate storage key
async function getStorageKey(
  userId: string,
  password: string
): Promise<string> {
  const encoder = new TextEncoder()
  const data = encoder.encode(userId + password)
  const hashBuffer = await crypto.subtle.digest('SHA-256', data)
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('')
}

// Encrypt data
async function encryptData(
  data: string,
  key: CryptoKey
): Promise<{ encrypted: string; nonce: string }> {
  const encoder = new TextEncoder()
  const dataBuffer = encoder.encode(data)

  // Generate random nonce
  const nonce = crypto.getRandomValues(new Uint8Array(12))

  // Encrypt
  const encryptedBuffer = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv: nonce },
    key,
    dataBuffer
  )

  // Convert to base64
  const encrypted = btoa(
    String.fromCharCode(...new Uint8Array(encryptedBuffer))
  )
  const nonceB64 = btoa(String.fromCharCode(...nonce))

  return { encrypted, nonce: nonceB64 }
}

// Decrypt data
async function decryptData(
  encrypted: string,
  nonce: string,
  key: CryptoKey
): Promise<string> {
  const encryptedBuffer = Uint8Array.from(atob(encrypted), c => c.charCodeAt(0))
  const nonceBuffer = Uint8Array.from(atob(nonce), c => c.charCodeAt(0))

  const decryptedBuffer = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv: nonceBuffer },
    key,
    encryptedBuffer
  )

  const decoder = new TextDecoder()
  return decoder.decode(decryptedBuffer)
}

// Register user
export async function registerBackupUser(
  credentials: BackupCredentials
): Promise<void> {
  const userId = await getServerUserId(credentials.username)

  const response = await fetch(`${BACKUP_API_URL}/api/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ userId })
  })

  if (!response.ok) {
    if (response.status === 409) {
      throw new Error('User already registered')
    }
    throw new Error('Registration failed')
  }
}

// Save backup
export async function saveBackup(
  credentials: BackupCredentials,
  data: object
): Promise<void> {
  const userId = await getServerUserId(credentials.username)
  const storageKey = await getStorageKey(userId, credentials.password)
  const encryptionKey = await deriveEncryptionKey(
    credentials.password,
    credentials.username
  )

  // Encrypt the data
  const dataJson = JSON.stringify(data)
  const { encrypted, nonce } = await encryptData(dataJson, encryptionKey)

  // Combine encrypted data and nonce
  const payload = JSON.stringify({ encrypted, nonce })

  const response = await fetch(`${BACKUP_API_URL}/api/backup`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      userId,
      storageKey,
      data: payload
    })
  })

  if (!response.ok) {
    throw new Error('Backup failed')
  }
}

// Restore backup
export async function restoreBackup(
  credentials: BackupCredentials
): Promise<object> {
  const userId = await getServerUserId(credentials.username)
  const storageKey = await getStorageKey(userId, credentials.password)
  const encryptionKey = await deriveEncryptionKey(
    credentials.password,
    credentials.username
  )

  const response = await fetch(
    `${BACKUP_API_URL}/api/backup?userId=${userId}&storageKey=${storageKey}`
  )

  if (!response.ok) {
    if (response.status === 404) {
      throw new Error('No backup found')
    }
    throw new Error('Failed to retrieve backup')
  }

  const { data } = await response.json()
  const { encrypted, nonce } = JSON.parse(data)

  // Decrypt
  const decryptedJson = await decryptData(encrypted, nonce, encryptionKey)
  return JSON.parse(decryptedJson)
}
```

**Deliverable**: Full backup/restore functionality working in SvelteKit app

---

## Security Considerations

### Threat Model

**What we're protecting against:**
1. **Database breach**: Attacker gains read access to database
   - ✓ Mitigation: All data encrypted client-side, server has no keys
2. **Server compromise**: Attacker gains full control of server
   - ✓ Mitigation: Server cannot decrypt data, only stores encrypted blobs
3. **Network interception**: Man-in-the-middle attack
   - ✓ Mitigation: HTTPS/TLS for all connections
4. **Brute force attacks**: Attacker tries to guess credentials
   - ✓ Mitigation: Rate limiting, PBKDF2 with high iteration count
5. **Data tampering**: Attacker modifies stored backups
   - ⚠️ Future: Add HMAC authentication tags to encrypted data

**What we're NOT protecting against:**
1. Client-side compromise (malware on user's device)
2. Phishing attacks (user gives credentials to attacker)
3. Weak passwords chosen by users
4. Timing attacks on cryptographic operations (acceptable risk)

### Security Checklist

**Server Security**:
- [ ] All endpoints use HTTPS (TLS 1.3)
- [ ] CORS properly configured with whitelist
- [ ] Rate limiting on all endpoints
- [ ] Input validation on all user inputs
- [ ] SQL injection prevented (SQLx parameterized queries)
- [ ] Error messages don't leak internal details
- [ ] Secrets stored in environment variables (not code)
- [ ] Database connections use SSL in production
- [ ] Logs don't contain sensitive data

**Cryptographic Security**:
- [ ] SHA-256 used for all hashing
- [ ] PBKDF2 with 100,000+ iterations
- [ ] AES-GCM for authenticated encryption
- [ ] Cryptographically random nonces
- [ ] No encryption keys stored on server
- [ ] No plaintext passwords transmitted

**Operational Security**:
- [ ] Regular dependency updates
- [ ] Security audit of critical paths
- [ ] Monitoring for unusual patterns
- [ ] Backup of database (encrypted data is safe to backup)
- [ ] Incident response plan

---

## Client-Server Integration

### API Contract

**Base URL**: `https://dailyreps-backup.fly.dev` (or custom domain)

**Authentication**: None (authentication is implicit via userId + storageKey hashes)

**Content-Type**: `application/json`

### Request/Response Formats

See [API Endpoints](#api-endpoints) section in CLAUDE.md for detailed specifications.

### Error Handling

**Client should handle**:
- Network errors (offline, timeout)
- 409 Conflict (user already exists)
- 404 Not Found (backup doesn't exist)
- 429 Too Many Requests (rate limited)
- 500 Internal Server Error (server issue)

**Error response format**:
```json
{
  "error": "Human readable error message"
}
```

---

## Deployment Strategy

### Development Environment

**Local development setup**:
1. Rust server on localhost:8080 (redb creates database automatically)
2. SvelteKit on localhost:5173
3. Test with local CORS configuration

```bash
# Start development server
cargo run

# Database file created at: ./data/dailyreps.db
```

### Production Environment

**Fly.io production setup**:
- Primary region: iad (US-East)
- Single instance with volume mount for redb
- No external database needed (embedded)
- Monitoring: Fly.io metrics + custom logging
- Backups: Volume snapshots

**Deployment process**:
```bash
# First time: create volume
fly volumes create dailyreps_data --region iad --size 1

# Set secrets
fly secrets set APP_SECRET_KEY=<secret>
fly secrets set ALLOWED_ORIGINS=https://dailyreps.netlify.app

# Deploy
fly deploy

# Check logs
fly logs
```

### Rollback Plan

If deployment fails:
1. Revert to previous Docker image: `fly releases`
2. Database is embedded - no separate migration rollback needed
3. Monitor error logs for issues

---

## Testing Strategy

### Unit Tests

**Coverage areas**:
- Input validation functions
- Hash generation
- Database query logic
- Error handling

**Run with**: `cargo test --lib`

### Integration Tests

**Coverage areas**:
- Full request/response cycles
- Database operations
- Error responses
- CORS behavior

**Run with**: `cargo test --test '*'`

### Manual Testing

**Test scenarios**:
1. Register new user → Success
2. Register duplicate user → 409 error
3. Save backup → Success with timestamp
4. Update existing backup → Success
5. Retrieve backup → Returns encrypted data
6. Retrieve non-existent backup → 404
7. Rate limit exceeded → 429
8. Invalid input formats → 400

### Load Testing

**Use wrk or similar**:
```bash
# Test registration endpoint
wrk -t2 -c10 -d30s --latency \
  -s register.lua \
  https://dailyreps-backup.fly.dev/api/register

# Measure p50, p95, p99 latencies
# Target: <100ms p95
```

### Security Testing

**Manual checks**:
- CORS from browser console
- Rate limiting behavior
- Oversized payload handling
- Invalid HMAC signature rejection

---

## Future Enhancements

### Short-term (Next 3-6 months)

1. **HMAC Authentication Tags**
   - Add HMAC to encrypted data for tamper detection
   - Verify integrity on retrieval

2. **Backup Versioning**
   - Store multiple versions of backups
   - Allow users to restore previous versions
   - Auto-prune old versions (keep last 10)

3. **Usage Metrics**
   - Track backup size per user
   - Implement storage quotas
   - Show user their backup size

4. **Email Notifications**
   - Optional: Email when backup succeeds/fails
   - Requires email service integration

5. **Compression Analysis for Anomaly Detection** ✅ COMPLETE
   - ✅ Detect non-JSON data patterns in encrypted backups via envelope validation
   - ✅ Analyze entropy to identify abuse (low entropy = unencrypted data)
   - ✅ Added `EXPECTED_APP_ID` constant for envelope validation
   - ✅ BackupEnvelope struct validates appId and encrypted data
   - ✅ Shannon entropy calculation flags suspiciously low-entropy data
   - This helps prevent storage abuse from non-official clients

### Long-term (6-12+ months)

1. **Multi-Device Sync**
   - Real-time sync instead of manual backup
   - Conflict resolution
   - Delta sync for efficiency

2. **Backup Scheduling**
   - Automatic daily/weekly backups
   - Background sync when app is open

3. **Recovery Codes**
   - Generate one-time recovery codes
   - Allow account recovery if password forgotten
   - Still maintains encryption security

4. **Audit Logs**
   - Track all access to user data
   - Show users when their backup was accessed
   - Detect suspicious activity

5. **Paid Tier**
   - Unlimited backup size
   - Longer version history
   - Priority support

---

## Success Metrics

**Technical Metrics**:
- ✓ All tests passing
- ✓ Zero clippy warnings
- ✓ >80% code coverage
- ✓ <100ms p95 latency
- ✓ Zero data breaches
- ✓ 99.9% uptime

**User Metrics** (after launch):
- Number of registered users
- Backup success rate
- Average backup size
- Restore success rate
- User retention

---

## Timeline Estimate

**Phase 1 (Setup)**: 1-2 days
**Phase 2 (Database)**: 1 day
**Phase 3 (Core API)**: 3-4 days
**Phase 4 (Security)**: 2-3 days
**Phase 5 (Testing)**: 2-3 days
**Phase 6 (Deployment)**: 1-2 days
**Phase 7 (Client Integration)**: 2-3 days

**Total estimate**: 12-18 days of focused development

---

## Questions & Decisions Log

**Q: Why not use traditional OAuth/JWT authentication?**
A: JWT would require server to know user identity. We want zero-knowledge architecture where server only knows hashes.

**Q: What if user forgets their password?**
A: By design, there is no recovery. This is a feature, not a bug - it ensures true zero-knowledge security. Future: recovery codes as optional feature.

**Q: Why PBKDF2 instead of Argon2 for key derivation?**
A: PBKDF2 is natively supported in Web Crypto API (browser). Argon2 would require WASM, adding complexity. PBKDF2 with 100k iterations is sufficient for this use case.

**Q: Could an attacker link userId to storageKey?**
A: Not without the password. storageKey = SHA256(userId + password), so password is required to derive the link.

**Q: What prevents rainbow table attacks on userId hashes?**
A: Short or common usernames could be vulnerable to rainbow tables. Users should be encouraged to use unique usernames. Future: add server-side pepper for additional protection.

**Q: Why allow any username instead of requiring email?**
A: Provides more privacy and flexibility. Users can choose any identifier they want without exposing their email address to even the hashed form on the server.

---

## Implementation Status

### ✅ COMPLETED

**Phase 1: Project Setup & Infrastructure** ✅
- [x] Initialize Rust project with Cargo
- [x] Create CLAUDE.md with project guidelines
- [x] Create IMPLEMENTATION_PLAN.md (this document)
- [x] Set up Cargo.toml with all dependencies
- [x] Create project directory structure
- [x] Set up .env.example file
- [x] Create .gitignore for Rust projects

**Phase 2: Database Setup** ✅ (Migrated to redb)
- [x] Create redb table definitions (users, backups, rate_limits, user_backups)
- [x] Create database models with bincode serialization
- [x] Set up database initialization in code
- [x] Create RateLimitRecord model with check_and_increment logic

**Phase 3: Core API Implementation** ✅
- [x] Create error handling module with custom types
- [x] Create configuration module
- [x] Create main.rs with Axum server setup
- [x] Implement health check endpoint (GET /health)
- [x] Implement user registration endpoint (POST /api/register)
- [x] Implement backup storage endpoint (POST /api/backup)
- [x] Implement backup retrieval endpoint (GET /api/backup)
- [x] Implement user deletion endpoint (DELETE /api/user)

**Phase 4: Security Hardening** ✅
- [x] Configure CORS middleware (whitelist origins)
- [x] Implement size limits (5MB max backup size)
- [x] Implement HMAC signature verification
- [x] Implement timestamp validation (prevent replay attacks)
- [x] Implement database-backed rate limiting (5/hour, 20/day)
- [x] Input validation on all endpoints
- [x] Sanitize error messages (no internal details)
- [x] Set up structured logging with tracing

**Phase 6: Deployment Preparation** ✅
- [x] Create Dockerfile for production build
- [x] Configure fly.toml with volume mount
- [x] Set environment variables for redb

**Anti-Griefing Measures** ✅
- [x] Layer 1: Size limits (5MB hard cap)
- [x] Layer 2: Rate limiting (per-user hourly/daily limits)
- [x] Layer 3: HMAC signatures (proves data from official app)
- [x] Layer 4: Timestamp validation (prevents replay attacks)
- [x] Layer 5: Compression analysis (envelope validation + entropy check)
- [x] Constants module for all limits
- [x] Security module for cryptographic verification

---

### 🚧 IN PROGRESS

**Phase 5: Testing & Quality Assurance** 🔄
- [x] Code compiles with `cargo check`
- [x] No clippy warnings
- [ ] Write additional unit tests for validation functions
- [ ] Write integration tests for each endpoint
- [ ] Test rate limiting behavior
- [ ] Achieve >80% code coverage

---

### 📋 TODO (Remaining Work)

**Phase 7: Client Integration**
- [ ] Design client-side backup UI (SvelteKit)
- [ ] Implement Web Crypto API encryption in client
- [ ] Implement HMAC signature generation in client
- [ ] Create backup registration flow
- [ ] Create backup save flow
- [ ] Create backup restore flow
- [ ] Create user deletion flow (with confirmation)
- [ ] Add loading states and error handling
- [ ] Test full end-to-end flow
- [ ] Update SvelteKit app documentation

**Future Enhancements** (Post-Launch)
- [ ] Backup versioning (keep last 10 versions)
- [ ] Usage metrics dashboard
- [ ] Email notifications (optional)
- [x] Compression analysis for anomaly detection ✅
- [ ] Per-user storage quotas
- [ ] HMAC authentication tags on encrypted data

---

## API Endpoints Summary

| Endpoint | Method | Purpose | Status |
|----------|--------|---------|--------|
| `/health` | GET | Health check | ✅ Complete |
| `/api/register` | POST | Register new user | ✅ Complete |
| `/api/backup` | POST | Store encrypted backup | ✅ Complete |
| `/api/backup` | GET | Retrieve encrypted backup | ✅ Complete |
| `/api/user` | DELETE | Delete user and all data | ✅ Complete |

---

## Security Features Summary

| Feature | Implementation | Status |
|---------|---------------|--------|
| Zero-knowledge architecture | Server never sees plaintext data | ✅ |
| Size limits | 5MB max backup size | ✅ |
| Rate limiting | 5/hour, 20/day per user | ✅ |
| HMAC signatures | Proves data from official app | ✅ |
| Timestamp validation | Prevents replay attacks (5min window) | ✅ |
| CORS whitelist | Only allowed origins can access API | ✅ |
| Input validation | All user inputs validated | ✅ |
| Embedded database | redb - no external attack surface | ✅ |
| Error message sanitization | No internal details leaked | ✅ |
| Structured logging | All actions logged securely | ✅ |
| Cascading deletes | User deletion removes all data | ✅ |
| Compression analysis | Envelope validation + entropy check | ✅ |

---

## Next Immediate Steps

1. **Deploy to Fly.io**
   ```bash
   fly volumes create dailyreps_data --region iad --size 1
   fly secrets set APP_SECRET_KEY=<secret>
   fly secrets set ALLOWED_ORIGINS=https://dailyreps.netlify.app
   fly deploy
   ```

2. **Run tests and verify deployment**
   - Test health endpoint
   - Test registration flow
   - Test backup/restore flow
   - Verify rate limiting works

3. **Build client integration**
   - Implement TypeScript backup functions in SvelteKit
   - Create UI for backup/restore/delete
   - Test end-to-end encryption flow

---

**Last Updated**: 2025-12-10
**Author**: Claude + Gavin
**Status**: Server implementation complete - ready for deployment
**Completion**: ~90% (Server done, client integration remaining)
