//! Integration tests for the DailyReps Backup Server API
//!
//! These tests verify the complete request/response cycle for all endpoints.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{delete, get, post},
};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use redb::Database;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

// Test configuration constants
const TEST_SECRET: &str = "test-secret-key";

// =============================================================================
// Test Helpers
// =============================================================================

/// Create a test configuration
fn test_config() -> dailyreps_backup_server::Config {
    dailyreps_backup_server::Config {
        server_host: "127.0.0.1".to_string(),
        server_port: 0,                // Random port
        database_path: "".to_string(), // Will be set per test
        allowed_origins: vec!["http://localhost:5173".to_string()],
        rate_limit_requests: 100,
        rate_limit_window_secs: 60,
        register_rate_limit_requests: 10,
        register_rate_limit_window_secs: 60,
        environment: "test".to_string(),
        app_secret_key: TEST_SECRET.to_string(),
        admin_secret_key: None,
        log_requests: false,
    }
}

/// Create a test database in a temporary directory
fn create_test_db(temp_dir: &TempDir) -> Arc<Database> {
    let db_path = temp_dir.path().join("test.db");
    let db = Database::create(&db_path).expect("Failed to create test database");

    // Initialize tables
    let write_txn = db.begin_write().unwrap();
    {
        use dailyreps_backup_server::db::tables;
        let _ = write_txn.open_table(tables::USERS).unwrap();
        let _ = write_txn.open_table(tables::BACKUPS).unwrap();
        let _ = write_txn.open_table(tables::RATE_LIMITS).unwrap();
        let _ = write_txn.open_table(tables::USER_BACKUPS).unwrap();
    }
    write_txn.commit().unwrap();

    Arc::new(db)
}

/// Create a test app router
fn create_test_app(db: Arc<Database>) -> Router {
    use dailyreps_backup_server::routes::*;

    let config = test_config();
    let state = dailyreps_backup_server::AppState { db, config };

    Router::new()
        .route("/health", get(health_check))
        .route("/api/register", post(register_user))
        .route("/api/backup", post(store_backup).get(retrieve_backup))
        .route("/api/user", delete(delete_user))
        .with_state(state)
}

/// Generate a valid SHA-256 hash (64 hex chars)
fn generate_user_id() -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("test-user-{}", rand_bytes()));
    hex::encode(hasher.finalize())
}

/// Generate pseudo-random bytes for testing
fn rand_bytes() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}", nanos)
}

/// Generate a storage key from user_id and password
fn generate_storage_key(user_id: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_id);
    hasher.update(password);
    hex::encode(hasher.finalize())
}

/// Generate HMAC signature for data
fn generate_hmac_signature(data: &str, secret: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Generate valid backup data (any string - server just stores it)
fn generate_valid_backup_data() -> String {
    // The server stores whatever data the client sends (after HMAC verification)
    // In production, this would be encrypted data from the client
    format!("encrypted-backup-data-{}", rand_bytes())
}

/// Parse response body as JSON
async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Create a POST request with JSON body
fn make_post_request(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// Create a GET request
fn make_get_request(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

/// Create a DELETE request with JSON body
fn make_delete_request(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// Setup a registered user and return (user_id, storage_key, app)
async fn setup_registered_user(db: Arc<Database>) -> (String, String, Router) {
    let app = create_test_app(db.clone());
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });

    let response = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let storage_key = generate_storage_key(&user_id, "test-password");
    let app = create_test_app(db);
    (user_id, storage_key, app)
}

/// Setup a user with a stored backup and return (user_id, storage_key, data, app)
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

// =============================================================================
// Health Check Tests
// =============================================================================

#[tokio::test]
async fn test_health_check_returns_healthy() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    let response = app.oneshot(make_get_request("/health")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["database"], "connected");
    assert!(body["version"].as_str().is_some());
}

// =============================================================================
// Registration Tests
// =============================================================================

#[tokio::test]
async fn test_register_user_success() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    let user_id = generate_user_id();
    let body = json!({ "userId": user_id });

    let response = app
        .oneshot(make_post_request("/api/register", body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn test_register_duplicate_user_returns_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    let user_id = generate_user_id();
    let body = json!({ "userId": user_id });

    // First registration
    let response = app
        .oneshot(make_post_request("/api/register", body.to_string()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Second registration with same user ID
    let app = create_test_app(db);
    let response = app
        .oneshot(make_post_request("/api/register", body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = body_to_json(response.into_body()).await;
    assert!(body["error"].as_str().unwrap().contains("already exists"));
}

#[tokio::test]
async fn test_register_invalid_user_id_format() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    // Too short
    let body = json!({ "userId": "abc123" });

    let response = app
        .oneshot(make_post_request("/api/register", body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_register_invalid_hex_characters() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    // Valid length but invalid hex chars
    let body = json!({ "userId": "z".repeat(64) });

    let response = app
        .oneshot(make_post_request("/api/register", body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// =============================================================================
// Backup Storage Tests
// =============================================================================

#[tokio::test]
async fn test_store_backup_success() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    // First register a user
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });

    let response = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Now store a backup
    let app = create_test_app(db);
    let storage_key = generate_storage_key(&user_id, "test-password");
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

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["success"], true);
    assert!(body["updatedAt"].as_str().is_some());
}

#[tokio::test]
async fn test_store_backup_invalid_signature() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    // Register user first
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });
    let _ = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();

    // Try to store backup with wrong signature
    let app = create_test_app(db);
    let storage_key = generate_storage_key(&user_id, "test-password");
    let data = generate_valid_backup_data();
    let timestamp = chrono::Utc::now().timestamp();
    let wrong_signature = "0".repeat(64);

    let backup_body = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "data": data,
        "signature": wrong_signature,
        "timestamp": timestamp
    });

    let response = app
        .oneshot(make_post_request("/api/backup", backup_body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_store_backup_expired_timestamp() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    // Register user first
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });
    let _ = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();

    // Try to store backup with old timestamp
    let app = create_test_app(db);
    let storage_key = generate_storage_key(&user_id, "test-password");
    let data = generate_valid_backup_data();
    let old_timestamp = chrono::Utc::now().timestamp() - 600; // 10 minutes ago
    let signature = generate_hmac_signature(&data, TEST_SECRET);

    let backup_body = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "data": data,
        "signature": signature,
        "timestamp": old_timestamp
    });

    let response = app
        .oneshot(make_post_request("/api/backup", backup_body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_store_backup_nonexistent_user() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    // Try to store backup for non-registered user
    let user_id = generate_user_id();
    let storage_key = generate_storage_key(&user_id, "test-password");
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =============================================================================
// Backup Retrieval Tests
// =============================================================================

#[tokio::test]
async fn test_retrieve_backup_success() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    // Register user
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });
    let _ = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();

    // Store backup
    let app = create_test_app(db.clone());
    let storage_key = generate_storage_key(&user_id, "test-password");
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

    let _ = app
        .oneshot(make_post_request("/api/backup", backup_body.to_string()))
        .await
        .unwrap();

    // Retrieve backup
    let app = create_test_app(db);
    let uri = format!("/api/backup?userId={}&storageKey={}", user_id, storage_key);

    let response = app.oneshot(make_get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["data"], data);
    assert!(body["updatedAt"].as_str().is_some());
}

#[tokio::test]
async fn test_retrieve_backup_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    let user_id = generate_user_id();
    let storage_key = generate_storage_key(&user_id, "test-password");
    let uri = format!("/api/backup?userId={}&storageKey={}", user_id, storage_key);

    let response = app.oneshot(make_get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_retrieve_backup_invalid_user_id() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    let storage_key = "a".repeat(64);
    let uri = format!("/api/backup?userId=invalid&storageKey={}", storage_key);

    let response = app.oneshot(make_get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_retrieve_backup_wrong_storage_key() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);

    let (user_id, _storage_key, _data, app) = setup_user_with_backup(db.clone()).await;

    // Try to retrieve with wrong storage key
    let wrong_storage_key = generate_storage_key(&user_id, "wrong-password");
    let uri = format!(
        "/api/backup?userId={}&storageKey={}",
        user_id, wrong_storage_key
    );

    let response = app.oneshot(make_get_request(&uri)).await.unwrap();

    // Should return NOT_FOUND (we don't reveal if the key exists but is wrong)
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =============================================================================
// User Deletion Tests
// =============================================================================

#[tokio::test]
async fn test_delete_user_success() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    // Register user
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });
    let _ = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();

    // Store backup
    let app = create_test_app(db.clone());
    let storage_key = generate_storage_key(&user_id, "test-password");
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

    let _ = app
        .oneshot(make_post_request("/api/backup", backup_body.to_string()))
        .await
        .unwrap();

    // Delete user
    let app = create_test_app(db.clone());
    let delete_timestamp = chrono::Utc::now().timestamp();
    let delete_signature = generate_hmac_signature(&storage_key, TEST_SECRET);

    let delete_body = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "signature": delete_signature,
        "timestamp": delete_timestamp
    });

    let response = app
        .oneshot(make_delete_request("/api/user", delete_body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["success"], true);

    // Verify user is actually deleted - can't retrieve backup
    let app = create_test_app(db);
    let uri = format!("/api/backup?userId={}&storageKey={}", user_id, storage_key);

    let response = app.oneshot(make_get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_user_invalid_signature() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    // Register user
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });
    let _ = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();

    // Store backup
    let app = create_test_app(db.clone());
    let storage_key = generate_storage_key(&user_id, "test-password");
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

    let _ = app
        .oneshot(make_post_request("/api/backup", backup_body.to_string()))
        .await
        .unwrap();

    // Try to delete with wrong signature
    let app = create_test_app(db);
    let delete_timestamp = chrono::Utc::now().timestamp();
    let wrong_signature = "0".repeat(64);

    let delete_body = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "signature": wrong_signature,
        "timestamp": delete_timestamp
    });

    let response = app
        .oneshot(make_delete_request("/api/user", delete_body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_delete_user_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db);

    let user_id = generate_user_id();
    let storage_key = generate_storage_key(&user_id, "test-password");
    let delete_timestamp = chrono::Utc::now().timestamp();
    let delete_signature = generate_hmac_signature(&storage_key, TEST_SECRET);

    let delete_body = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "signature": delete_signature,
        "timestamp": delete_timestamp
    });

    let response = app
        .oneshot(make_delete_request("/api/user", delete_body.to_string()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =============================================================================
// Rate Limiting Tests
// =============================================================================

#[tokio::test]
async fn test_rate_limiting_backup_hourly() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);

    // Register user
    let app = create_test_app(db.clone());
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });
    let _ = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();

    let storage_key = generate_storage_key(&user_id, "test-password");

    // Store backups up to hourly limit (5)
    for i in 0..5 {
        let app = create_test_app(db.clone());
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

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Backup {} should succeed",
            i + 1
        );
    }

    // 6th backup should fail with rate limit
    let app = create_test_app(db);
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

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// =============================================================================
// Backup Update Tests (Upsert Behavior)
// =============================================================================

#[tokio::test]
async fn test_backup_update_replaces_data() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);
    let app = create_test_app(db.clone());

    // Register user
    let user_id = generate_user_id();
    let register_body = json!({ "userId": user_id });
    let _ = app
        .oneshot(make_post_request(
            "/api/register",
            register_body.to_string(),
        ))
        .await
        .unwrap();

    let storage_key = generate_storage_key(&user_id, "test-password");

    // Store first backup
    let app = create_test_app(db.clone());
    let data1 = generate_valid_backup_data();
    let timestamp = chrono::Utc::now().timestamp();
    let signature = generate_hmac_signature(&data1, TEST_SECRET);

    let backup_body = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "data": data1,
        "signature": signature,
        "timestamp": timestamp
    });

    let _ = app
        .oneshot(make_post_request("/api/backup", backup_body.to_string()))
        .await
        .unwrap();

    // Store second backup with same storage key
    let app = create_test_app(db.clone());
    let data2 = generate_valid_backup_data();
    let timestamp2 = chrono::Utc::now().timestamp();
    let signature2 = generate_hmac_signature(&data2, TEST_SECRET);

    let backup_body2 = json!({
        "userId": user_id,
        "storageKey": storage_key,
        "data": data2,
        "signature": signature2,
        "timestamp": timestamp2
    });

    let _ = app
        .oneshot(make_post_request("/api/backup", backup_body2.to_string()))
        .await
        .unwrap();

    // Retrieve and verify it's the second backup
    let app = create_test_app(db);
    let uri = format!("/api/backup?userId={}&storageKey={}", user_id, storage_key);

    let response = app.oneshot(make_get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["data"], data2);
}

// =============================================================================
// Admin Endpoint Tests
// =============================================================================

const TEST_ADMIN_SECRET: &str = "test-admin-secret";

/// Create a test config with admin key enabled
fn test_config_with_admin() -> dailyreps_backup_server::Config {
    dailyreps_backup_server::Config {
        server_host: "127.0.0.1".to_string(),
        server_port: 0,
        database_path: "".to_string(),
        allowed_origins: vec!["http://localhost:5173".to_string()],
        rate_limit_requests: 100,
        rate_limit_window_secs: 60,
        register_rate_limit_requests: 10,
        register_rate_limit_window_secs: 60,
        environment: "test".to_string(),
        app_secret_key: TEST_SECRET.to_string(),
        admin_secret_key: Some(TEST_ADMIN_SECRET.to_string()),
        log_requests: false,
    }
}

/// Create a test app with admin endpoint enabled
fn create_test_app_with_admin(db: Arc<Database>, db_path: String) -> Router {
    use dailyreps_backup_server::routes::*;

    let mut config = test_config_with_admin();
    config.database_path = db_path;
    let state = dailyreps_backup_server::AppState { db, config };

    Router::new()
        .route("/health", get(health_check))
        .route("/api/register", post(register_user))
        .route("/api/backup", post(store_backup).get(retrieve_backup))
        .route("/api/user", delete(delete_user))
        .route("/admin/stats", get(admin_stats))
        .with_state(state)
}

#[tokio::test]
async fn test_admin_stats_success() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::create(&db_path).expect("Failed to create test database");

    // Initialize tables
    let write_txn = db.begin_write().unwrap();
    {
        use dailyreps_backup_server::db::tables;
        let _ = write_txn.open_table(tables::USERS).unwrap();
        let _ = write_txn.open_table(tables::BACKUPS).unwrap();
        let _ = write_txn.open_table(tables::RATE_LIMITS).unwrap();
        let _ = write_txn.open_table(tables::USER_BACKUPS).unwrap();
    }
    write_txn.commit().unwrap();

    let db = Arc::new(db);
    let app = create_test_app_with_admin(db, db_path.to_string_lossy().to_string());

    let uri = format!("/admin/stats?key={}", TEST_ADMIN_SECRET);
    let response = app.oneshot(make_get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["user_count"], 0);
    assert_eq!(body["backup_count"], 0);
    assert!(body["database_size_bytes"].as_u64().is_some());
    assert!(body["database_size_human"].as_str().is_some());
}

#[tokio::test]
async fn test_admin_stats_invalid_key() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::create(&db_path).expect("Failed to create test database");

    let write_txn = db.begin_write().unwrap();
    {
        use dailyreps_backup_server::db::tables;
        let _ = write_txn.open_table(tables::USERS).unwrap();
        let _ = write_txn.open_table(tables::BACKUPS).unwrap();
        let _ = write_txn.open_table(tables::RATE_LIMITS).unwrap();
        let _ = write_txn.open_table(tables::USER_BACKUPS).unwrap();
    }
    write_txn.commit().unwrap();

    let db = Arc::new(db);
    let app = create_test_app_with_admin(db, db_path.to_string_lossy().to_string());

    let response = app
        .oneshot(make_get_request("/admin/stats?key=wrong-key"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_stats_disabled_without_key() {
    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db(&temp_dir);

    // Use standard test app (no admin key configured)
    use dailyreps_backup_server::routes::*;

    let config = test_config();
    let state = dailyreps_backup_server::AppState { db, config };

    let app = Router::new()
        .route("/admin/stats", get(admin_stats))
        .with_state(state);

    let response = app
        .oneshot(make_get_request("/admin/stats?key=any-key"))
        .await
        .unwrap();

    // Should return unauthorized because admin_secret_key is None
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
