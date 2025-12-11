# Refactoring Plan: Code Duplication Fixes

**Created**: 2025-12-10
**Status**: Ready for implementation

This document outlines all code duplication issues found in the codebase and provides specific action items to fix them.

---

## High Priority Fixes

### 1. Fix Storage Key Validation in delete.rs

**Problem**: `delete.rs` manually implements SHA-256 hash validation instead of using the existing `Backup::validate_storage_key()` function.

**Files**:
- `src/routes/delete.rs` lines 53-59

**Current code**:
```rust
if payload.storage_key.len() != 64
    || !payload.storage_key.chars().all(|c| c.is_ascii_hexdigit())
{
    return Err(AppError::InvalidInput(
        "Invalid storage key format".to_string(),
    ));
}
```

**Fix**: Replace with:
```rust
if !Backup::validate_storage_key(&payload.storage_key) {
    return Err(AppError::InvalidInput(
        "Invalid storage key format".to_string(),
    ));
}
```

**Also add import**: `use crate::models::Backup;`

---

### 2. Extract HMAC & Timestamp Validation Helper

**Problem**: Identical HMAC signature and timestamp validation code in `backup.rs` and `delete.rs`.

**Files**:
- `src/routes/backup.rs` lines 68-82
- `src/routes/delete.rs` lines 63-80

**Action**: Create new file `src/routes/validation.rs` with:

```rust
use crate::constants::MAX_TIMESTAMP_AGE_SECS;
use crate::error::{AppError, Result};
use crate::security::{validate_timestamp, verify_hmac};

/// Verify HMAC signature and timestamp for authenticated requests
pub fn validate_signed_request(
    data: &str,
    signature: &str,
    timestamp: i64,
    secret: &str,
) -> Result<()> {
    if !verify_hmac(data, signature, secret) {
        tracing::warn!("Invalid HMAC signature");
        return Err(AppError::InvalidSignature);
    }

    if !validate_timestamp(timestamp, MAX_TIMESTAMP_AGE_SECS) {
        return Err(AppError::InvalidInput(
            "Timestamp too old or in the future".to_string(),
        ));
    }

    Ok(())
}
```

**Update `src/routes/mod.rs`**:
```rust
pub mod validation;
pub use validation::validate_signed_request;
```

**Update `backup.rs` and `delete.rs`** to use:
```rust
validate_signed_request(&payload.data, &payload.signature, payload.timestamp, &state.config.app_secret_key)?;
```

---

### 3. Create Database Transaction Helper

**Problem**: Same `spawn_blocking` + transaction pattern repeated in 5 route handlers.

**Files**:
- `src/routes/backup.rs` lines 152-216, 254-271
- `src/routes/delete.rs` lines 88-149
- `src/routes/register.rs` lines 56-79
- `src/routes/health.rs` lines 13-21

**Action**: Add helper to `src/db/mod.rs`:

```rust
use std::future::Future;
use tokio::task::JoinError;

/// Execute a closure within a write transaction
pub async fn with_write_txn<F, T>(db: Db, f: F) -> crate::error::Result<T>
where
    F: FnOnce(&redb::WriteTransaction) -> crate::error::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let write_txn = db.begin_write()?;
        let result = f(&write_txn)?;
        write_txn.commit()?;
        Ok(result)
    })
    .await?
}

/// Execute a closure within a read transaction
pub async fn with_read_txn<F, T>(db: Db, f: F) -> crate::error::Result<T>
where
    F: FnOnce(&redb::ReadTransaction) -> crate::error::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let read_txn = db.begin_read()?;
        f(&read_txn)
    })
    .await?
}
```

---

## Medium Priority Fixes

### 4. Add Error Message Constants

**Problem**: Same error message strings repeated across files.

**Action**: Add to `src/constants.rs`:

```rust
// Error messages
pub const ERR_INVALID_USER_ID: &str = "Invalid user ID format";
pub const ERR_INVALID_STORAGE_KEY: &str = "Invalid storage key format";
pub const ERR_INVALID_TIMESTAMP: &str = "Timestamp too old or in the future";
pub const ERR_USER_ID_MUST_BE_SHA256: &str = "User ID must be a valid SHA-256 hash (64 hex characters)";
```

**Update routes** to use these constants instead of string literals.

---

### 5. Extract Timestamp Conversion Helper

**Problem**: Same `DateTime::from_timestamp(...).unwrap_or_else(Utc::now)` pattern in 2 places.

**Files**:
- `src/routes/backup.rs` lines 224 and 278

**Action**: Add to `src/routes/mod.rs` or a new `src/utils.rs`:

```rust
use chrono::{DateTime, Utc};

/// Convert Unix timestamp to DateTime, defaulting to now if invalid
pub fn timestamp_to_rfc3339(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}
```

---

### 6. Create Test Setup Helpers

**Problem**: Same user registration and backup setup pattern repeated 15+ times in tests.

**File**: `tests/integration_tests.rs`

**Action**: Add helper functions:

```rust
/// Setup a registered user and return (user_id, storage_key, app)
async fn setup_registered_user(db: Arc<Database>) -> (String, String, Router) {
    let app = create_test_app(db.clone());
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });

    let response = app
        .oneshot(make_post_request("/api/register", register_body.to_string()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let storage_key = generate_storage_key(&user_id, "test-password");
    let app = create_test_app(db);
    (user_id, storage_key, app)
}

/// Setup a user with a stored backup
async fn setup_user_with_backup(db: Arc<Database>) -> (String, String, String, Router) {
    let (user_id, storage_key, app) = setup_registered_user(db.clone()).await;

    let data = generate_valid_backup_data();
    let timestamp = chrono::Utc::now().timestamp();
    let signature = generate_hmac_signature(&data, TEST_SECRET);

    let backup_body = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "data": data,
        "signature": signature,
        "timestamp": timestamp
    });

    let response = app
        .oneshot(make_post_request("/api/backup", backup_body.to_string()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let app = create_test_app(db);
    (user_id, storage_key, data, app)
}
```

---

### 7. Create HTTP Request Factory Functions

**Problem**: Same `Request::builder()` pattern repeated 40+ times.

**File**: `tests/integration_tests.rs`

**Action**: Add helper functions:

```rust
fn make_post_request(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn make_get_request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn make_delete_request(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}
```

---

## Low Priority Fixes

### 8. Add Response Assertion Helpers

**File**: `tests/integration_tests.rs`

**Action**: Add:

```rust
async fn assert_success(response: axum::response::Response) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["success"], true);
    body
}

async fn assert_error(response: axum::response::Response, expected_status: StatusCode) -> Value {
    assert_eq!(response.status(), expected_status);
    body_to_json(response.into_body()).await
}
```

---

## Implementation Order

1. **Fix #1** - Storage key validation (5 min, low risk)
2. **Fix #2** - HMAC/timestamp helper (15 min, medium risk)
3. **Fix #4** - Error constants (10 min, low risk)
4. **Fix #5** - Timestamp helper (5 min, low risk)
5. **Fix #7** - Test request factories (10 min, low risk)
6. **Fix #6** - Test setup helpers (20 min, low risk)
7. **Fix #3** - DB transaction helper (30 min, higher risk - affects all routes)
8. **Fix #8** - Response assertion helpers (10 min, low risk)

---

## Verification

After each fix:
```bash
cargo fmt
cargo clippy
USER_ID_PEPPER=test-pepper APP_SECRET_KEY=test-secret cargo test
```

All 53 tests must pass after each change.

---

## Notes

- Fix #3 (DB transaction helper) is the most impactful but also highest risk - consider doing it last
- Fixes #6, #7, #8 only affect tests and can be done together
- Consider committing after each logical group of fixes
