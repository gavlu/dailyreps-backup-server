# Implementation Plan: DailyReps Encrypted Backup Server

**Status**: Planning Phase
**Last Updated**: 2025-12-09
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
│                    PostgreSQL DATABASE                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  users table:                                                    │
│    ├─ id (TEXT, PK): "a4f3e21..." (SHA-256 hash)                 │
│    └─ created_at (TIMESTAMPTZ)                                   │
│                                                                   │
│  backups table:                                                  │
│    ├─ storage_key (TEXT, PK): "9b2c8d..." (SHA-256 hash)         │
│    ├─ user_id (TEXT, FK): references users.id                    │
│    ├─ encrypted_data (TEXT): base64 encrypted blob               │
│    ├─ created_at (TIMESTAMPTZ)                                   │
│    └─ updated_at (TIMESTAMPTZ)                                   │
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

### Phase 1: Project Setup & Infrastructure ✓

**Status**: In Progress

**Tasks**:
- [x] Initialize Rust project with Cargo
- [x] Create CLAUDE.md with project guidelines
- [x] Create IMPLEMENTATION_PLAN.md (this document)
- [ ] Set up Cargo.toml with all dependencies
- [ ] Create project directory structure
- [ ] Set up .env.example file
- [ ] Initialize git repository
- [ ] Create .gitignore for Rust projects

**Dependencies needed**:
```toml
[dependencies]
# Web framework
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace", "compression"] }

# Database
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "chrono"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Security & Crypto (minimal - most crypto happens client-side)
sha2 = "0.10"

# Configuration
dotenvy = "0.15"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Rate limiting
tower-governor = "0.4"

[dev-dependencies]
reqwest = "0.11"
tokio-test = "0.4"
```

**Deliverable**: Rust project initialized with proper structure and dependencies

---

### Phase 2: Database Setup

**Status**: Pending

**Tasks**:
- [ ] Install PostgreSQL (Docker recommended for development)
- [ ] Install sqlx-cli: `cargo install sqlx-cli --features postgres`
- [ ] Create initial migration for users table
- [ ] Create migration for backups table
- [ ] Add indexes for performance
- [ ] Test migrations (run and revert)
- [ ] Set up database connection pool in code
- [ ] Create database models (User, Backup structs)

**Database Schema**:

```sql
-- Migration 001: Create users table
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_created_at ON users(created_at);

-- Migration 002: Create backups table
CREATE TABLE backups (
    storage_key TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    encrypted_data TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_backups_user_id ON backups(user_id);
CREATE INDEX idx_backups_updated_at ON backups(updated_at);
```

**Deliverable**: Database schema created and migrations working

---

### Phase 3: Core API Implementation

**Status**: Pending

#### 3.1: Basic Server Setup

**Tasks**:
- [ ] Create main.rs with Axum server setup
- [ ] Configure Tokio runtime
- [ ] Set up basic routing
- [ ] Add health check endpoint
- [ ] Test server starts and responds

**Code structure**:
```rust
// src/main.rs
#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    // Set up tracing/logging
    // Create database pool
    // Build Axum router
    // Start server
}
```

#### 3.2: User Registration Endpoint

**Tasks**:
- [ ] Create POST /api/register route
- [ ] Validate userId format (must be 64-char hex string)
- [ ] Check for existing user in database
- [ ] Insert new user if doesn't exist
- [ ] Return appropriate responses (200 success, 409 conflict)
- [ ] Add error handling
- [ ] Write unit tests
- [ ] Write integration tests

**Rust implementation**:
```rust
// src/routes/register.rs
pub async fn register(
    State(pool): State<PgPool>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, AppError> {
    // Validate userId format
    // Check if user exists
    // Insert new user
    // Return success
}
```

#### 3.3: Backup Storage Endpoint

**Tasks**:
- [ ] Create POST /api/backup route
- [ ] Validate userId, storageKey, and data fields
- [ ] Verify userId exists in users table
- [ ] Validate encrypted data is valid base64
- [ ] Upsert backup (insert or update if exists)
- [ ] Return success with timestamp
- [ ] Add error handling
- [ ] Write tests for happy path
- [ ] Write tests for error cases (invalid user, etc.)

**Rust implementation**:
```rust
// src/routes/backup.rs
pub async fn store_backup(
    State(pool): State<PgPool>,
    Json(payload): Json<BackupRequest>,
) -> Result<Json<BackupResponse>, AppError> {
    // Validate inputs
    // Verify user exists
    // Upsert backup
    // Return success
}
```

#### 3.4: Backup Retrieval Endpoint

**Tasks**:
- [ ] Create GET /api/backup route
- [ ] Extract userId and storageKey from query params
- [ ] Validate parameter formats
- [ ] Fetch backup from database
- [ ] Return 404 if not found
- [ ] Return encrypted data with metadata
- [ ] Add error handling
- [ ] Write tests

**Rust implementation**:
```rust
pub async fn retrieve_backup(
    State(pool): State<PgPool>,
    Query(params): Query<RetrieveParams>,
) -> Result<Json<RetrieveResponse>, AppError> {
    // Validate inputs
    // Fetch from database
    // Return data or 404
}
```

**Deliverable**: All three core endpoints implemented and tested

---

### Phase 4: Security Hardening

**Status**: Pending

#### 4.1: CORS Configuration

**Tasks**:
- [ ] Configure tower-http CORS middleware
- [ ] Whitelist only allowed origins (from env var)
- [ ] Set allowed methods (GET, POST)
- [ ] Set allowed headers
- [ ] Test CORS from browser

**Implementation**:
```rust
use tower_http::cors::{CorsLayer, Any};

let cors = CorsLayer::new()
    .allow_origin(allowed_origins)
    .allow_methods([Method::GET, Method::POST])
    .allow_headers(Any);
```

#### 4.2: Rate Limiting

**Tasks**:
- [ ] Add tower-governor for rate limiting
- [ ] Configure per-IP limits
- [ ] Stricter limits on /api/register
- [ ] Return 429 with Retry-After header
- [ ] Test rate limiting behavior

**Implementation**:
```rust
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};

let governor_conf = Box::new(
    GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(5)
        .finish()
        .unwrap(),
);
```

#### 4.3: Input Validation

**Tasks**:
- [ ] Create validation functions for all inputs
- [ ] Validate hash formats (64 hex chars for SHA-256)
- [ ] Validate data size limits (prevent huge uploads)
- [ ] Sanitize error messages (no internal details leaked)
- [ ] Add comprehensive validation tests

#### 4.4: Logging & Monitoring

**Tasks**:
- [ ] Set up tracing-subscriber
- [ ] Log all requests with request IDs
- [ ] Log errors with context (but not to clients)
- [ ] Configure log levels via environment
- [ ] Test logging output

**Deliverable**: Production-ready security features implemented

---

### Phase 5: Testing & Quality Assurance

**Status**: Pending

**Tasks**:
- [ ] Write unit tests for all validation functions
- [ ] Write integration tests for each endpoint
- [ ] Test error cases (invalid inputs, missing data, etc.)
- [ ] Test concurrent requests
- [ ] Test database failure scenarios
- [ ] Set up test database with testcontainers
- [ ] Achieve >80% code coverage
- [ ] Run clippy and fix all warnings
- [ ] Format all code with cargo fmt

**Test categories**:
1. **Unit tests**: Individual functions and validation logic
2. **Integration tests**: Full request/response cycles
3. **Error handling**: All error paths covered
4. **Performance tests**: Measure response times under load
5. **Security tests**: Rate limiting, input validation, CORS

**Deliverable**: Comprehensive test suite with high coverage

---

### Phase 6: Deployment Preparation

**Status**: Pending

#### 6.1: Docker Configuration

**Tasks**:
- [ ] Create Dockerfile for production build
- [ ] Use multi-stage build (minimize image size)
- [ ] Test local Docker build
- [ ] Verify Docker image runs correctly

**Dockerfile**:
```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates
COPY --from=builder /app/target/release/dailyreps-backup-server /usr/local/bin/
CMD ["dailyreps-backup-server"]
```

#### 6.2: Fly.io Setup

**Tasks**:
- [ ] Install flyctl CLI
- [ ] Create Fly.io account
- [ ] Run `flyctl launch` to create app
- [ ] Configure fly.toml
- [ ] Set up Fly.io Postgres database
- [ ] Set environment secrets
- [ ] Deploy to production
- [ ] Test production deployment
- [ ] Set up health checks
- [ ] Configure auto-scaling if needed

**fly.toml**:
```toml
app = "dailyreps-backup"

[build]
  dockerfile = "Dockerfile"

[env]
  SERVER_PORT = "8080"
  RUST_LOG = "info"

[[services]]
  http_checks = []
  internal_port = 8080
  protocol = "tcp"

  [[services.ports]]
    force_https = true
    handlers = ["http"]
    port = 80

  [[services.ports]]
    handlers = ["tls", "http"]
    port = 443
```

**Deliverable**: Server deployed and accessible at production URL

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
1. PostgreSQL via Docker
2. Rust server on localhost:8080
3. SvelteKit on localhost:5173
4. Test with local CORS configuration

### Staging Environment

**Optional staging on Fly.io**:
- Separate app instance
- Separate database
- Test production configuration
- Use for integration testing

### Production Environment

**Fly.io production setup**:
- Primary region: Closest to users (US-West or US-East)
- Auto-scaling: 1-3 instances based on load
- PostgreSQL: Fly.io managed Postgres
- Monitoring: Fly.io metrics + custom logging
- Backups: Automated daily database backups

**Deployment process**:
```bash
# Deploy to production
flyctl deploy

# Run migrations
flyctl ssh console
./migrations/run.sh

# Check logs
flyctl logs

# Scale if needed
flyctl scale count 2
```

### Rollback Plan

If deployment fails:
1. Revert to previous Docker image: `flyctl releases`
2. Roll back database migration: `sqlx migrate revert`
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
- SQL injection attempts (should be impossible with SQLx)
- Oversized payload handling

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

5. **Compression Analysis for Anomaly Detection**
   - Detect non-JSON data patterns in encrypted backups
   - Analyze entropy to identify abuse (random data vs structured JSON)
   - Add constants when implementing:
     - `EXPECTED_APP_ID: &str = "dailyreps-app"` - for envelope validation
     - `PROTOCOL_VERSION: &str = "1.0"` - for versioning backup format
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
- [x] **Updated Rust toolchain to 1.91.1** (latest stable)

**Phase 2: Database Setup** ✅
- [x] Create migration for users table
- [x] Create migration for backups table
- [x] Create migration for rate_limits table
- [x] Add indexes for performance
- [x] Set up database connection pool
- [x] Create database models (User, Backup structs)

**Phase 3: Core API Implementation** ✅
- [x] Create error handling module with custom types
- [x] Create configuration module
- [x] Create main.rs with Axum server setup
- [x] Implement health check endpoint (GET /health)
- [x] Implement user registration endpoint (POST /api/register)
- [x] Implement backup storage endpoint (POST /api/backup)
- [x] Implement backup retrieval endpoint (GET /api/backup)
- [x] **Implement user deletion endpoint (DELETE /api/user)** ✨ NEW

**Phase 4: Security Hardening** ✅
- [x] Configure CORS middleware (whitelist origins)
- [x] Implement size limits (5MB max backup size)
- [x] Implement HMAC signature verification
- [x] Implement timestamp validation (prevent replay attacks)
- [x] Implement database-backed rate limiting (5/hour, 20/day)
- [x] Input validation on all endpoints
- [x] Sanitize error messages (no internal details)
- [x] Set up structured logging with tracing

**Anti-Griefing Measures** ✅
- [x] Layer 1: Size limits (5MB hard cap)
- [x] Layer 2: Rate limiting (per-user hourly/daily limits)
- [x] Layer 3: HMAC signatures (proves data from official app)
- [x] Layer 4: Timestamp validation (prevents replay attacks)
- [x] Constants module for all limits
- [x] Security module for cryptographic verification

---

### 🚧 IN PROGRESS

**Phase 5: Testing & Quality Assurance** 🔄
- [ ] Set up test database with Docker
- [ ] Write unit tests for validation functions
- [ ] Write integration tests for each endpoint
- [ ] Test error cases (invalid inputs, missing data, etc.)
- [ ] Test rate limiting behavior
- [ ] Achieve >80% code coverage
- [ ] Run clippy and fix all warnings
- [ ] Format all code with cargo fmt

**Blockers:**
- SQLx compile-time query checking requires DATABASE_URL or prepared query cache
- Need to either:
  1. Set up local PostgreSQL database, or
  2. Generate SQLx query cache with `cargo sqlx prepare`, or
  3. Switch to runtime-only query methods (less type-safe)

---

### 📋 TODO (Remaining Work)

**Phase 6: Deployment Preparation**
- [ ] Create Dockerfile for production build
- [ ] Test local Docker build
- [ ] Set up Fly.io configuration (fly.toml)
- [ ] Create Fly.io Postgres database
- [ ] Set environment secrets in Fly.io
- [ ] Deploy to production
- [ ] Set up health checks
- [ ] Configure auto-scaling

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
- [ ] Compression analysis for anomaly detection
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
| SQL injection prevention | SQLx parameterized queries | ✅ |
| Error message sanitization | No internal details leaked | ✅ |
| Structured logging | All actions logged securely | ✅ |
| Cascading deletes | User deletion removes all data | ✅ |

---

## Next Immediate Steps

1. **Resolve SQLx compilation**
   - Option A: Set up local PostgreSQL and run migrations
   - Option B: Use `cargo sqlx prepare` to generate query cache
   - Option C: Switch to runtime queries (less ideal)

2. **Run full test suite**
   - Write comprehensive tests for all endpoints
   - Test anti-griefing measures work correctly
   - Verify rate limiting resets properly

3. **Deploy to Fly.io**
   - Create production Postgres database
   - Configure environment variables
   - Deploy and test in production environment

4. **Build client integration**
   - Implement TypeScript backup functions in SvelteKit
   - Create UI for backup/restore/delete
   - Test end-to-end encryption flow

---

**Last Updated**: 2025-12-09
**Author**: Claude + Gavin
**Status**: Core implementation complete - ready for database setup and testing
**Completion**: ~85% (Core functionality done, testing and deployment remaining)
