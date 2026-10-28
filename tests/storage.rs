use std::sync::Arc;

use matrix_bot_sdk::storage::{
    ICryptoStorageProvider, IStorageProvider, MemoryStorageProvider, RustSdkCryptoStorageProvider,
};
use serde_json::json;

#[tokio::test]
async fn memory_storage_get_set() {
    let storage = MemoryStorageProvider::new();

    assert!(
        storage
            .get("key1")
            .await
            .expect("get should work")
            .is_none()
    );

    storage
        .set("key1", json!("value1"))
        .await
        .expect("set should work");

    let value = storage.get("key1").await.expect("get should work");
    assert_eq!(value, Some(json!("value1")));
}

#[tokio::test]
async fn memory_storage_delete() {
    let storage = MemoryStorageProvider::new();

    storage
        .set("key1", json!("value1"))
        .await
        .expect("set should work");
    assert!(
        storage
            .get("key1")
            .await
            .expect("get should work")
            .is_some()
    );

    storage.delete("key1").await.expect("delete should work");
    assert!(
        storage
            .get("key1")
            .await
            .expect("get should work")
            .is_none()
    );
}

#[tokio::test]
async fn memory_storage_keys() {
    let storage = MemoryStorageProvider::new();

    storage
        .set("key1", json!("value1"))
        .await
        .expect("set should work");
    storage
        .set("key2", json!("value2"))
        .await
        .expect("set should work");
    storage
        .set("key3", json!("value3"))
        .await
        .expect("set should work");

    let keys = storage.keys().await.expect("keys should work");
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&"key1".to_owned()));
    assert!(keys.contains(&"key2".to_owned()));
    assert!(keys.contains(&"key3".to_owned()));
}

#[tokio::test]
async fn memory_storage_update_existing_key() {
    let storage = MemoryStorageProvider::new();

    storage
        .set("key1", json!("value1"))
        .await
        .expect("set should work");
    storage
        .set("key1", json!("updated"))
        .await
        .expect("set should work");

    let value = storage.get("key1").await.expect("get should work");
    assert_eq!(value, Some(json!("updated")));
}

#[tokio::test]
async fn memory_storage_complex_value() {
    let storage = MemoryStorageProvider::new();

    let complex = json!({
        "nested": {
            "key": "value"
        },
        "array": [1, 2, 3]
    });

    storage
        .set("complex", complex.clone())
        .await
        .expect("set should work");

    let value = storage.get("complex").await.expect("get should work");
    assert_eq!(value, Some(complex));
}

#[tokio::test]
async fn appservice_storage_register_user() {
    let storage = MemoryStorageProvider::new();

    let user_id = "@alice:example.org";

    assert!(
        !storage
            .is_user_registered(user_id)
            .await
            .expect("check should work")
    );

    storage
        .register_user(user_id)
        .await
        .expect("register should work");

    assert!(
        storage
            .is_user_registered(user_id)
            .await
            .expect("check should work")
    );
}

#[tokio::test]
async fn appservice_storage_multiple_users() {
    let storage = MemoryStorageProvider::new();

    let user1 = "@alice:example.org";
    let user2 = "@bob:example.org";

    storage
        .register_user(user1)
        .await
        .expect("register should work");

    assert!(
        storage
            .is_user_registered(user1)
            .await
            .expect("check should work")
    );
    assert!(
        !storage
            .is_user_registered(user2)
            .await
            .expect("check should work")
    );

    storage
        .register_user(user2)
        .await
        .expect("register should work");

    assert!(
        storage
            .is_user_registered(user1)
            .await
            .expect("check should work")
    );
    assert!(
        storage
            .is_user_registered(user2)
            .await
            .expect("check should work")
    );
}

#[tokio::test]
async fn appservice_storage_duplicate_register() {
    let storage = MemoryStorageProvider::new();

    let user_id = "@alice:example.org";

    storage
        .register_user(user_id)
        .await
        .expect("register should work");
    storage
        .register_user(user_id)
        .await
        .expect("register should work");

    assert!(
        storage
            .is_user_registered(user_id)
            .await
            .expect("check should work")
    );
}

#[tokio::test]
async fn crypto_storage_session_key() {
    let storage = MemoryStorageProvider::new();

    let room_id = "!room:example.org";
    let session_key = "session_key_value";

    assert!(
        storage
            .session_key(room_id)
            .await
            .expect("get should work")
            .is_none()
    );

    storage
        .store_session_key(room_id, session_key)
        .await
        .expect("store should work");

    let key = storage.session_key(room_id).await.expect("get should work");
    assert_eq!(key, Some(session_key.to_owned()));
}

#[tokio::test]
async fn crypto_storage_multiple_rooms() {
    let storage = MemoryStorageProvider::new();

    let room1 = "!room1:example.org";
    let room2 = "!room2:example.org";

    storage
        .store_session_key(room1, "key1")
        .await
        .expect("store should work");
    storage
        .store_session_key(room2, "key2")
        .await
        .expect("store should work");

    assert_eq!(
        storage.session_key(room1).await.expect("get should work"),
        Some("key1".to_owned())
    );
    assert_eq!(
        storage.session_key(room2).await.expect("get should work"),
        Some("key2".to_owned())
    );
}

#[tokio::test]
async fn rust_crypto_storage_wrapper() {
    let inner = Arc::new(MemoryStorageProvider::new());
    let storage = RustSdkCryptoStorageProvider::new(inner);

    storage
        .set("test_key", json!("test_value"))
        .await
        .expect("set should work");

    let value = storage.get("test_key").await.expect("get should work");
    assert_eq!(value, Some(json!("test_value")));
}

#[tokio::test]
async fn rust_crypto_storage_session_key() {
    let inner = Arc::new(MemoryStorageProvider::new());
    let storage = RustSdkCryptoStorageProvider::new(inner);

    let room_id = "!room:example.org";
    let session_key = "encrypted_session_key";

    storage
        .store_session_key(room_id, session_key)
        .await
        .expect("store should work");

    let key = storage.session_key(room_id).await.expect("get should work");
    assert_eq!(key, Some(session_key.to_owned()));
}

#[tokio::test]
async fn rust_crypto_storage_keys_prefix_stripped() {
    let inner = Arc::new(MemoryStorageProvider::new());
    let storage = RustSdkCryptoStorageProvider::new(inner.clone());

    storage
        .set("key1", json!("value1"))
        .await
        .expect("set should work");
    storage
        .set("key2", json!("value2"))
        .await
        .expect("set should work");

    let keys = storage.keys().await.expect("keys should work");

    assert!(keys.contains(&"key1".to_owned()));
    assert!(keys.contains(&"key2".to_owned()));

    let inner_keys = inner.keys().await.expect("inner keys should work");
    assert!(inner_keys.iter().all(|k| k.starts_with("rust_crypto:")));
}

// ---------------------------------------------------------------------------
// SimpleFsStorageProvider tests
// ---------------------------------------------------------------------------

use matrix_bot_sdk::storage::{IAppserviceStorageProvider, SimpleFsStorageProvider};

#[tokio::test]
async fn simple_fs_storage_get_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("storage.json");
    let storage = SimpleFsStorageProvider::new(&path).await.expect("create");

    assert!(storage.get("key1").await.expect("get").is_none());

    storage
        .set("key1", serde_json::json!("value1"))
        .await
        .expect("set");

    let value = storage.get("key1").await.expect("get");
    assert_eq!(value, Some(serde_json::json!("value1")));
}

#[tokio::test]
async fn simple_fs_storage_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("storage.json");
    let storage = SimpleFsStorageProvider::new(&path).await.expect("create");

    storage
        .set("key1", serde_json::json!("value1"))
        .await
        .expect("set");
    assert!(storage.get("key1").await.expect("get").is_some());

    storage.delete("key1").await.expect("delete");
    assert!(storage.get("key1").await.expect("get").is_none());
}

#[tokio::test]
async fn simple_fs_storage_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("storage.json");
    let storage = SimpleFsStorageProvider::new(&path).await.expect("create");

    storage.set("a", serde_json::json!(1)).await.expect("set");
    storage.set("b", serde_json::json!(2)).await.expect("set");
    storage.set("c", serde_json::json!(3)).await.expect("set");

    let keys = storage.keys().await.expect("keys");
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&"a".to_owned()));
    assert!(keys.contains(&"b".to_owned()));
    assert!(keys.contains(&"c".to_owned()));
}

#[tokio::test]
async fn simple_fs_storage_persists_across_instances() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("storage.json");

    {
        let storage = SimpleFsStorageProvider::new(&path).await.expect("create");
        storage
            .set("persistent_key", serde_json::json!("persistent_value"))
            .await
            .expect("set");
    }

    // Re-open same file
    let storage2 = SimpleFsStorageProvider::new(&path).await.expect("reopen");
    let value = storage2.get("persistent_key").await.expect("get");
    assert_eq!(value, Some(serde_json::json!("persistent_value")));
}

#[tokio::test]
async fn simple_fs_storage_register_user() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("storage.json");
    let storage = SimpleFsStorageProvider::new(&path).await.expect("create");

    assert!(
        !storage
            .is_user_registered("@alice:example.org")
            .await
            .expect("check")
    );

    storage
        .register_user("@alice:example.org")
        .await
        .expect("register");

    assert!(
        storage
            .is_user_registered("@alice:example.org")
            .await
            .expect("check")
    );
}

#[tokio::test]
async fn simple_fs_storage_session_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("storage.json");
    let storage = SimpleFsStorageProvider::new(&path).await.expect("create");

    assert!(
        storage
            .session_key("!room:example.org")
            .await
            .expect("get")
            .is_none()
    );

    storage
        .store_session_key("!room:example.org", "my_session_key")
        .await
        .expect("store");

    let key = storage.session_key("!room:example.org").await.expect("get");
    assert_eq!(key, Some("my_session_key".to_owned()));
}

#[tokio::test]
async fn simple_fs_storage_update_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("storage.json");
    let storage = SimpleFsStorageProvider::new(&path).await.expect("create");

    storage
        .set("key", serde_json::json!("original"))
        .await
        .expect("set");
    storage
        .set("key", serde_json::json!("updated"))
        .await
        .expect("update");

    let value = storage.get("key").await.expect("get");
    assert_eq!(value, Some(serde_json::json!("updated")));
}
