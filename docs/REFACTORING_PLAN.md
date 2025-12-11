# Refactoring Plan: Code Duplication Fixes

**Created**: 2025-12-10
**Updated**: 2025-12-11
**Status**: Completed (7 of 8 fixes implemented)

This document outlines all code duplication issues found in the codebase and provides specific action items to fix them.

---

## Completed Fixes

### 1. Fix Storage Key Validation in delete.rs ✅

**Problem**: `delete.rs` manually implements SHA-256 hash validation instead of using the existing `Backup::validate_storage_key()` function.

**Solution**: Replaced inline validation with `Backup::validate_storage_key()` call and added import.

---

### 2. Extract HMAC & Timestamp Validation Helper ✅

**Problem**: Identical HMAC signature and timestamp validation code in `backup.rs` and `delete.rs`.

**Solution**: Created `src/routes/validation.rs` with:
- `SignedRequestError` enum with constrained error variants (`InvalidSignature`, `InvalidTimestamp`)
- `validate_signed_request()` function returning `Result<(), SignedRequestError>`
- `From<SignedRequestError> for AppError` implementation for seamless error conversion
- `timestamp_to_rfc3339()` helper function

Updated `backup.rs` and `delete.rs` to use the new helper.

---

### 4. Add Error Message Constants ✅

**Problem**: Same error message strings repeated across files.

**Solution**: Added to `src/constants.rs`:
```rust
pub const ERR_INVALID_USER_ID: &str = "Invalid user ID format";
pub const ERR_INVALID_STORAGE_KEY: &str = "Invalid storage key format";
pub const ERR_INVALID_TIMESTAMP: &str = "Timestamp too old or in the future";
pub const ERR_USER_ID_MUST_BE_SHA256: &str = "User ID must be a valid SHA-256 hash (64 hex characters)";
```

Updated all routes to use these constants.

---

### 5. Extract Timestamp Conversion Helper ✅

**Problem**: Same `DateTime::from_timestamp(...).unwrap_or_else(Utc::now)` pattern in 2 places.

**Solution**: Added `timestamp_to_rfc3339()` to `src/routes/validation.rs` and updated `backup.rs` to use it.

---

### 6. Create Test Setup Helpers ✅

**Problem**: Same user registration and backup setup pattern repeated 15+ times in tests.

**Solution**: Added to `tests/integration_tests.rs`:
- `setup_registered_user()` - sets up a registered user and returns (user_id, storage_key, app)
- `setup_user_with_backup()` - sets up a user with a backup and returns (user_id, storage_key, data, app)

---

### 7. Create HTTP Request Factory Functions ✅

**Problem**: Same `Request::builder()` pattern repeated 40+ times.

**Solution**: Added to `tests/integration_tests.rs`:
- `make_post_request(uri, body)` - creates POST request with JSON body
- `make_get_request(uri)` - creates GET request
- `make_delete_request(uri, body)` - creates DELETE request with JSON body

Updated all tests to use these helpers.

---

## Skipped Fixes

### 3. Create Database Transaction Helper ⏭️

**Problem**: Same `spawn_blocking` + transaction pattern repeated in 5 route handlers.

**Reason for skipping**: Higher risk change that affects all routes. The current pattern works well and the benefit doesn't outweigh the risk of introducing bugs. Can be revisited in a future refactoring effort if needed.

---

### 8. Add Response Assertion Helpers ⏭️

**Problem**: Could reduce boilerplate in test assertions.

**Reason for skipping**: No concrete plan for usage. Adding dead code is not desirable. Can be added when there's a specific need.

---

## Verification

All changes verified with:
```bash
cargo fmt
cargo clippy -- -D warnings
USER_ID_PEPPER=test-pepper APP_SECRET_KEY=test-secret cargo test
```

All 53 tests pass (34 unit + 19 integration).

---

## Summary of Changes

### New Files
- `src/routes/validation.rs` - HMAC/timestamp validation helper with constrained error type

### Modified Files
- `src/constants.rs` - Added error message constants
- `src/routes/mod.rs` - Added validation module export
- `src/routes/backup.rs` - Use validation helper and error constants
- `src/routes/delete.rs` - Use validation helper, Backup::validate_storage_key(), and error constants
- `src/routes/register.rs` - Use error constants
- `tests/integration_tests.rs` - Added HTTP request factories and test setup helpers
