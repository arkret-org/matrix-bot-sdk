use matrix_bot_sdk::e2ee::{CryptoClient, CryptoEngine, ICryptoRoomInformation, RoomTracker};
use serde_json::json;

#[tokio::test]
async fn crypto_client_prepare_and_sign() {
    let crypto = CryptoClient::new(CryptoEngine::new(b"shared-secret".to_vec()));
    crypto.prepare().await.expect("prepare should succeed");

    assert!(crypto.is_ready().await);
    assert!(crypto.client_device_id().await.is_some());
    assert!(crypto.client_device_ed25519().await.is_some());

    let signatures = crypto
        .sign(&json!({"hello": "world"}))
        .await
        .expect("sign should succeed");
    assert!(signatures.get("@unknown:localhost").is_some());
}

#[tokio::test]
async fn crypto_client_encrypt_decrypt_room_event_roundtrip() {
    let crypto = CryptoClient::new(CryptoEngine::new(b"room-secret".to_vec()));
    crypto.prepare().await.expect("prepare should succeed");
    crypto.mark_room_encrypted("!room:example.org", true).await;

    let encrypted = crypto
        .encrypt_room_event(
            "!room:example.org",
            "m.room.message",
            &json!({"body": "hello", "msgtype": "m.text"}),
        )
        .await
        .expect("encrypt_room_event should succeed");
    assert_eq!(encrypted.algorithm, "m.megolm.v1.aes-sha2");
    assert!(!encrypted.ciphertext.is_empty());

    let decrypted = crypto
        .decrypt_room_event(&encrypted, "!room:example.org")
        .await
        .expect("decrypt_room_event should succeed");
    assert_eq!(decrypted.room_id, "!room:example.org");
    assert_eq!(decrypted.event.content["body"], "hello");
}

#[tokio::test]
async fn crypto_client_encrypt_decrypt_media_roundtrip() {
    let crypto = CryptoClient::new(CryptoEngine::new(b"media-secret".to_vec()));
    crypto.prepare().await.expect("prepare should succeed");

    let plaintext = b"hello encrypted media";
    let (encrypted, file_info) = crypto
        .encrypt_media(plaintext)
        .await
        .expect("encrypt_media should succeed");
    let decrypted = crypto
        .decrypt_media_bytes(&file_info, &encrypted)
        .await
        .expect("decrypt_media_bytes should succeed");

    assert_eq!(decrypted, plaintext);
}

#[tokio::test]
async fn room_tracker_tracks_encryption_state() {
    let tracker = RoomTracker::new();
    assert!(!tracker.is_room_encrypted("!room:example.org").await);

    tracker.set_encrypted("!room:example.org", true).await;
    assert!(tracker.is_room_encrypted("!room:example.org").await);

    let info = tracker
        .get_room_crypto_config("!room:example.org")
        .await
        .expect("config should be readable");
    assert_eq!(info.algorithm.as_deref(), Some("m.megolm.v1.aes-sha2"));
}
