use std::sync::Arc;

use matrix_bot_sdk::e2ee::{CryptoClient, CryptoEngine, ICryptoRoomInformation, RoomTracker};
use matrix_bot_sdk::storage::{
    ICryptoStorageProvider, IStorageProvider, MemoryStorageProvider, RustSdkCryptoStorageProvider,
};
use serde_json::json;

#[tokio::test]
async fn memory_storage_basic_crud() {
    let storage = MemoryStorageProvider::new();
    storage
        .set("bot.name", json!("matrix-bot"))
        .await
        .expect("set key");

    let current = storage.get("bot.name").await.expect("read key");
    assert_eq!(current, Some(json!("matrix-bot")));

    storage.delete("bot.name").await.expect("delete key");
    assert!(storage.get("bot.name").await.expect("read key").is_none());
}

#[tokio::test]
async fn rust_sdk_crypto_storage_wraps_inner_provider() {
    let inner: Arc<dyn IStorageProvider> = Arc::new(MemoryStorageProvider::new());
    let storage = RustSdkCryptoStorageProvider::new(inner);

    storage
        .store_session_key("!room:example.org", "k123")
        .await
        .expect("store key");
    let value = storage
        .session_key("!room:example.org")
        .await
        .expect("read key");
    assert_eq!(value.as_deref(), Some("k123"));
}

#[tokio::test]
async fn crypto_client_encrypts_and_decrypts() {
    let engine = CryptoEngine::new(vec![1, 2, 3, 4]);
    let crypto = CryptoClient::new(engine);
    let encrypted = crypto
        .encrypt_message("!room:example.org", "hello matrix")
        .await
        .expect("encrypt");
    let ciphertext = serde_json::Value::String(encrypted);
    let decrypted = crypto
        .decrypt_message("!room:example.org", &ciphertext)
        .await
        .expect("decrypt");
    assert_eq!(decrypted.as_str().unwrap(), "hello matrix");
}

#[tokio::test]
async fn room_tracker_tracks_encryption_state() {
    let tracker = RoomTracker::new();
    tracker.set_encrypted("!a:example.org", true).await;
    assert!(tracker.is_room_encrypted("!a:example.org").await);
    assert!(!tracker.is_room_encrypted("!b:example.org").await);
}
