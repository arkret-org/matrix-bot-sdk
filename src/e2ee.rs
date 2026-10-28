use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::client::MatrixClient;

/// Fallback encryption engine using XOR cipher
/// For production E2EE, use matrix-sdk directly
#[derive(Debug, Clone, Default)]
pub struct CryptoEngine {
    shared_secret: Arc<RwLock<Vec<u8>>>,
}

impl CryptoEngine {
    pub fn new(shared_secret: Vec<u8>) -> Self {
        Self {
            shared_secret: Arc::new(RwLock::new(shared_secret)),
        }
    }

    pub async fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let key = self.shared_secret.read().await;
        xor_cipher(plaintext, &key)
    }

    pub async fn decrypt(&self, ciphertext: &[u8]) -> Vec<u8> {
        let key = self.shared_secret.read().await;
        xor_cipher(ciphertext, &key)
    }

    pub async fn encrypt_message(&self, plaintext: &str) -> String {
        STANDARD.encode(self.encrypt(plaintext.as_bytes()).await)
    }

    pub async fn decrypt_message(&self, ciphertext: &str) -> anyhow::Result<String> {
        let bytes = STANDARD.decode(ciphertext)?;
        let decrypted = self.decrypt(&bytes).await;
        Ok(String::from_utf8(decrypted)?)
    }
}

fn xor_cipher(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    data.iter()
        .enumerate()
        .map(|(idx, byte)| byte ^ key[idx % key.len()])
        .collect()
}

#[derive(Clone)]
pub struct CryptoClient {
    engine: CryptoEngine,
    client: Option<MatrixClient>,
    device_id: Arc<RwLock<Option<String>>>,
    device_ed25519: Arc<RwLock<Option<String>>>,
    device_curve25519: Arc<RwLock<Option<String>>>,
    ready: Arc<RwLock<bool>>,
    backup_enabled: Arc<RwLock<bool>>,
    room_tracker: RoomTracker,
}

impl CryptoClient {
    pub fn new(engine: CryptoEngine) -> Self {
        Self {
            engine,
            client: None,
            device_id: Arc::new(RwLock::new(None)),
            device_ed25519: Arc::new(RwLock::new(None)),
            device_curve25519: Arc::new(RwLock::new(None)),
            ready: Arc::new(RwLock::new(false)),
            backup_enabled: Arc::new(RwLock::new(false)),
            room_tracker: RoomTracker::new(),
        }
    }

    pub fn with_client(mut self, client: MatrixClient) -> Self {
        self.room_tracker = self.room_tracker.clone().with_client(client.clone());
        self.client = Some(client);
        self
    }

    pub async fn set_device_id(&self, device_id: impl Into<String>) {
        *self.device_id.write().await = Some(device_id.into());
    }

    pub async fn device_id(&self) -> Option<String> {
        self.device_id.read().await.clone()
    }

    pub async fn client_device_id(&self) -> Option<String> {
        self.device_id().await
    }

    pub async fn client_device_ed25519(&self) -> Option<String> {
        self.device_ed25519.read().await.clone()
    }

    pub async fn client_device_curve25519(&self) -> Option<String> {
        self.device_curve25519.read().await.clone()
    }

    pub async fn is_ready(&self) -> bool {
        *self.ready.read().await
    }

    pub async fn prepare(&self) -> anyhow::Result<()> {
        if self.is_ready().await {
            return Ok(());
        }

        let (user_id, device_id) = if let Some(client) = &self.client {
            let whoami = client.get_who_am_i().await.unwrap_or_default();
            let user_id = if whoami.user_id.is_empty() {
                "@unknown:localhost".to_owned()
            } else {
                whoami.user_id
            };
            let device_id = whoami
                .device_id
                .unwrap_or_else(|| "UNKNOWNDEVICE".to_owned());
            (user_id, device_id)
        } else {
            ("@unknown:localhost".to_owned(), "UNKNOWNDEVICE".to_owned())
        };

        self.set_device_id(device_id.clone()).await;

        // Derive deterministic pseudo device keys from identity material. This keeps
        // compatibility surfaces stable without requiring matrix-sdk-crypto.
        let curve_seed = format!("{user_id}|{device_id}|curve25519");
        let ed_seed = format!("{user_id}|{device_id}|ed25519");
        let curve = URL_SAFE_NO_PAD.encode(Sha256::digest(curve_seed.as_bytes()));
        let ed = URL_SAFE_NO_PAD.encode(Sha256::digest(ed_seed.as_bytes()));

        *self.device_curve25519.write().await = Some(curve);
        *self.device_ed25519.write().await = Some(ed);
        *self.ready.write().await = true;
        Ok(())
    }

    pub async fn encrypt_message(&self, _room_id: &str, plaintext: &str) -> anyhow::Result<String> {
        Ok(self.engine.encrypt_message(plaintext).await)
    }

    pub async fn decrypt_message(
        &self,
        _room_id: &str,
        ciphertext: &Value,
    ) -> anyhow::Result<Value> {
        if let Some(str) = ciphertext.as_str() {
            let decrypted = self.engine.decrypt_message(str).await?;
            Ok(Value::String(decrypted))
        } else {
            Ok(ciphertext.clone())
        }
    }

    pub async fn is_room_encrypted(&self, room_id: &str) -> anyhow::Result<bool> {
        if self.room_tracker.is_room_encrypted(room_id).await {
            return Ok(true);
        }
        if let Some(client) = &self.client {
            let state = client
                .get_room_state_event(room_id, "m.room.encryption", "")
                .await;
            Ok(state.is_ok())
        } else {
            Ok(false)
        }
    }

    pub async fn get_encryption_info(
        &self,
        room_id: &str,
    ) -> anyhow::Result<Option<EncryptionInfo>> {
        let tracked = self.room_tracker.get_room_crypto_config(room_id).await?;
        if let Some(algorithm) = tracked.algorithm {
            return Ok(Some(EncryptionInfo {
                algorithm,
                rotation_period_ms: tracked.rotation_period_ms,
                rotation_period_msgs: tracked.rotation_period_msgs,
            }));
        }

        if let Some(client) = &self.client {
            match client
                .get_room_state_event(room_id, "m.room.encryption", "")
                .await
            {
                Ok(value) => {
                    let info: EncryptionInfo = serde_json::from_value(value)?;
                    Ok(Some(info))
                }
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    pub async fn on_room_event(&self, room_id: &str, event: &Value) -> anyhow::Result<()> {
        self.room_tracker.on_room_event(room_id, event).await
    }

    pub async fn on_room_join(&self, room_id: &str) -> anyhow::Result<()> {
        self.room_tracker.on_room_join(room_id).await
    }

    pub async fn mark_room_encrypted(&self, room_id: &str, encrypted: bool) {
        self.room_tracker.set_encrypted(room_id, encrypted).await;
    }

    pub async fn trust_device(&self, _user_id: &str, _device_id: &str) -> anyhow::Result<()> {
        // Placeholder - real implementation needs matrix-sdk-crypto
        Ok(())
    }

    pub async fn untrust_device(&self, _user_id: &str, _device_id: &str) -> anyhow::Result<()> {
        // Placeholder - real implementation needs matrix-sdk-crypto
        Ok(())
    }

    pub async fn get_user_devices(&self, user_id: &str) -> anyhow::Result<Vec<DeviceInfo>> {
        if let Some(client) = &self.client {
            let response = client.get_user_devices(&[user_id]).await?;
            let devices = response
                .get("device_keys")
                .and_then(|d| d.get(user_id))
                .and_then(|d| d.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(device_id, _)| DeviceInfo {
                            device_id: device_id.clone(),
                            display_name: None,
                            last_seen_ts: None,
                            trusted: false,
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(devices)
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn has_unverified_devices(&self, room_id: &str) -> anyhow::Result<bool> {
        if let Some(client) = &self.client {
            let members = client.get_joined_room_members(room_id).await?;
            for user_id in members {
                let devices = self.get_user_devices(&user_id).await?;
                if devices.iter().any(|d| !d.trusted) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub async fn export_room_keys(&self, _passphrase: &str) -> anyhow::Result<Vec<u8>> {
        // Placeholder - real implementation needs matrix-sdk-crypto
        Ok(Vec::new())
    }

    pub async fn import_room_keys(
        &self,
        _encrypted_keys: &[u8],
        _passphrase: &str,
    ) -> anyhow::Result<RoomKeyImportResult> {
        // Placeholder - real implementation needs matrix-sdk-crypto
        Ok(RoomKeyImportResult {
            imported_count: 0,
            total_count: 0,
            keys: HashMap::new(),
        })
    }

    pub async fn export_room_keys_for_session(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let key = format!("{room_id}:{session_id}");
        Ok(vec![key])
    }

    pub async fn update_sync_data(
        &self,
        _to_device_messages: &[Value],
        _otk_counts: &Value,
        _unused_fallback_key_algs: &[String],
        _changed_device_lists: &[String],
        _left_device_lists: &[String],
    ) -> anyhow::Result<()> {
        // Placeholder: full Olm sync flow requires matrix-sdk-crypto integration.
        Ok(())
    }

    pub async fn sign(&self, obj: &Value) -> anyhow::Result<Value> {
        self.prepare().await?;

        let mut cloned = obj.clone();
        if let Some(map) = cloned.as_object_mut() {
            map.remove("signatures");
            map.remove("unsigned");
        }

        let payload = serde_json::to_vec(&cloned)?;
        let key = self.engine.shared_secret.read().await.clone();
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(&key)?;
        mac.update(&payload);
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

        let user_id = if let Some(client) = &self.client {
            client
                .get_user_id()
                .await
                .unwrap_or_else(|_| "@unknown:localhost".to_owned())
        } else {
            "@unknown:localhost".to_owned()
        };
        let device_id = self
            .device_id()
            .await
            .unwrap_or_else(|| "UNKNOWNDEVICE".to_owned());
        let key_id = format!("ed25519:{device_id}");

        Ok(serde_json::json!({
            user_id: {
                key_id: signature
            }
        }))
    }

    pub async fn encrypt_room_event(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
    ) -> anyhow::Result<crate::models::events::EncryptedRoomEvent> {
        self.prepare().await?;

        if !self.is_room_encrypted(room_id).await? {
            anyhow::bail!("Room is not encrypted");
        }

        let payload = serde_json::json!({
            "type": event_type,
            "content": content,
        });
        let ciphertext = self
            .engine
            .encrypt_message(&serde_json::to_string(&payload)?)
            .await;

        Ok(crate::models::events::EncryptedRoomEvent {
            algorithm: "m.megolm.v1.aes-sha2".to_owned(),
            sender_key: self.client_device_curve25519().await.unwrap_or_default(),
            ciphertext,
        })
    }

    pub async fn decrypt_room_event(
        &self,
        event: &crate::models::events::EncryptedRoomEvent,
        room_id: &str,
    ) -> anyhow::Result<crate::models::events::RoomEvent> {
        self.prepare().await?;

        let plaintext = self.engine.decrypt_message(&event.ciphertext).await?;
        let payload: Value = serde_json::from_str(&plaintext)?;
        let event_type = payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("io.t2bot.unknown");
        let content = payload
            .get("content")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));

        let raw = serde_json::json!({
            "type": event_type,
            "content": content,
        });
        Ok(crate::models::events::converter::parse_room_event(
            &raw, room_id,
        )?)
    }

    pub async fn encrypt_media(
        &self,
        file: &[u8],
    ) -> anyhow::Result<(Vec<u8>, crate::models::events::EncryptedFile)> {
        self.prepare().await?;
        let encrypted = self.engine.encrypt(file).await;

        let hash = STANDARD.encode(Sha256::digest(&encrypted));
        let mut hashes = HashMap::new();
        hashes.insert("sha256".to_owned(), hash);

        let key_material = self.engine.shared_secret.read().await.clone();
        let key = URL_SAFE_NO_PAD.encode(key_material);
        let iv = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 16]>());

        let file_info = crate::models::events::EncryptedFile {
            url: String::new(),
            key: crate::models::events::JsonWebKey {
                kty: "oct".to_owned(),
                alg: Some("A256CTR".to_owned()),
                k: key,
                ext: Some(true),
            },
            iv,
            hashes,
            version: "v2".to_owned(),
        };
        Ok((encrypted, file_info))
    }

    pub async fn decrypt_media_bytes(
        &self,
        file: &crate::models::events::EncryptedFile,
        encrypted: &[u8],
    ) -> anyhow::Result<Vec<u8>> {
        if let Some(expected_hash) = file.hashes.get("sha256") {
            let actual_hash = STANDARD.encode(Sha256::digest(encrypted));
            if expected_hash != &actual_hash {
                anyhow::bail!("encrypted media hash mismatch");
            }
        }
        Ok(self.engine.decrypt(encrypted).await)
    }

    pub async fn decrypt_media(
        &self,
        file: &crate::models::events::EncryptedFile,
    ) -> anyhow::Result<Vec<u8>> {
        let client = self
            .client
            .as_ref()
            .context("decrypt_media requires an attached MatrixClient")?;
        if file.url.is_empty() {
            anyhow::bail!("encrypted file url is empty");
        }
        let downloaded = client.download_content(&file.url, true).await?;
        self.decrypt_media_bytes(file, &downloaded.data).await
    }

    pub async fn enable_key_backup(&self, _info: &Value) -> anyhow::Result<()> {
        self.prepare().await?;
        *self.backup_enabled.write().await = true;
        Ok(())
    }

    pub async fn disable_key_backup(&self) -> anyhow::Result<()> {
        *self.backup_enabled.write().await = false;
        Ok(())
    }

    pub async fn is_key_backup_enabled(&self) -> bool {
        *self.backup_enabled.read().await
    }

    pub fn engine(&self) -> &CryptoEngine {
        &self.engine
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomKeyImportResult {
    pub imported_count: usize,
    pub total_count: usize,
    pub keys: HashMap<String, HashMap<String, Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionInfo {
    pub algorithm: String,
    #[serde(default)]
    pub rotation_period_ms: Option<u64>,
    #[serde(default)]
    pub rotation_period_msgs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub last_seen_ts: Option<u64>,
    #[serde(default)]
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CryptoRoomInformation {
    #[serde(default)]
    pub algorithm: Option<String>,
    #[serde(default)]
    pub rotation_period_ms: Option<u64>,
    #[serde(default)]
    pub rotation_period_msgs: Option<u64>,
    #[serde(default)]
    pub history_visibility: Option<String>,
}

#[async_trait]
pub trait ICryptoRoomInformation: Send + Sync {
    async fn is_room_encrypted(&self, room_id: &str) -> bool;
}

#[derive(Default, Clone)]
pub struct RoomTracker {
    rooms: Arc<RwLock<HashMap<String, CryptoRoomInformation>>>,
    client: Option<MatrixClient>,
}

impl RoomTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_client(mut self, client: MatrixClient) -> Self {
        self.client = Some(client);
        self
    }

    pub async fn set_encrypted(&self, room_id: &str, encrypted: bool) {
        let mut rooms = self.rooms.write().await;
        let entry = rooms.entry(room_id.to_owned()).or_default();
        if encrypted {
            if entry.algorithm.is_none() {
                entry.algorithm = Some("m.megolm.v1.aes-sha2".to_owned());
            }
        } else {
            entry.algorithm = None;
        }
    }

    pub async fn on_room_join(&self, room_id: &str) -> anyhow::Result<()> {
        self.queue_room_check(room_id).await
    }

    pub async fn on_room_event(&self, room_id: &str, event: &Value) -> anyhow::Result<()> {
        let state_key = event.get("state_key").and_then(Value::as_str);
        if state_key != Some("") {
            return Ok(());
        }
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event_type == "m.room.encryption" || event_type == "m.room.history_visibility" {
            self.queue_room_check(room_id).await?;
        }
        Ok(())
    }

    pub async fn queue_room_check(&self, room_id: &str) -> anyhow::Result<()> {
        let Some(client) = &self.client else {
            return Ok(());
        };

        let mut info = self
            .rooms
            .read()
            .await
            .get(room_id)
            .cloned()
            .unwrap_or_default();

        let encryption_state = client
            .get_room_state_event(room_id, "m.room.encryption", "")
            .await;

        match encryption_state {
            Ok(value) => {
                info.algorithm = value
                    .get("algorithm")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                info.rotation_period_ms = value.get("rotation_period_ms").and_then(Value::as_u64);
                info.rotation_period_msgs =
                    value.get("rotation_period_msgs").and_then(Value::as_u64);
            }
            Err(_) => {
                info.algorithm = None;
                info.rotation_period_ms = None;
                info.rotation_period_msgs = None;
            }
        }

        if let Ok(value) = client
            .get_room_state_event(room_id, "m.room.history_visibility", "")
            .await
        {
            info.history_visibility = value
                .get("history_visibility")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }

        self.rooms.write().await.insert(room_id.to_owned(), info);
        Ok(())
    }

    pub async fn get_room_crypto_config(
        &self,
        room_id: &str,
    ) -> anyhow::Result<CryptoRoomInformation> {
        if let Some(config) = self.rooms.read().await.get(room_id).cloned() {
            return Ok(config);
        }
        self.queue_room_check(room_id).await?;
        Ok(self
            .rooms
            .read()
            .await
            .get(room_id)
            .cloned()
            .unwrap_or_default())
    }
}

#[async_trait]
impl ICryptoRoomInformation for RoomTracker {
    async fn is_room_encrypted(&self, room_id: &str) -> bool {
        self.rooms
            .read()
            .await
            .get(room_id)
            .and_then(|info| info.algorithm.as_ref())
            .is_some()
    }
}

pub mod decorators {
    use super::CryptoClient;

    pub async fn encrypt_before_send(
        crypto: &CryptoClient,
        room_id: &str,
        plaintext: &str,
        room_encrypted: bool,
    ) -> anyhow::Result<String> {
        if room_encrypted {
            crypto.encrypt_message(room_id, plaintext).await
        } else {
            Ok(plaintext.to_owned())
        }
    }
}

#[derive(Clone)]
pub struct UnstableApis {
    client: MatrixClient,
}

impl UnstableApis {
    pub fn new(client: MatrixClient) -> Self {
        Self { client }
    }

    pub async fn get_room_aliases(&self, room_id: &str) -> anyhow::Result<Vec<String>> {
        let encoded = crate::client::encode_path_component(room_id);
        let response = self
            .client
            .raw_json(
                reqwest::Method::GET,
                &format!("/_matrix/client/unstable/org.matrix.msc2432/rooms/{encoded}/aliases"),
                None,
            )
            .await?;
        Ok(response
            .get("aliases")
            .and_then(Value::as_array)
            .map(|aliases| {
                aliases
                    .iter()
                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn add_reaction_to_event(
        &self,
        room_id: &str,
        event_id: &str,
        emoji: &str,
    ) -> anyhow::Result<String> {
        let content = serde_json::json!({
            "m.relates_to": {
                "event_id": event_id,
                "key": emoji,
                "rel_type": "m.annotation",
            },
        });
        self.client
            .send_raw_event(room_id, "m.reaction", &content, None)
            .await
    }

    pub async fn get_room_hierarchy(&self, room_id: &str) -> anyhow::Result<Value> {
        let encoded = crate::client::encode_path_component(room_id);
        self.client
            .raw_json(
                reqwest::Method::GET,
                &format!("/_matrix/client/unstable/org.matrix.msc3266/rooms/{encoded}/hierarchy"),
                None,
            )
            .await
    }

    pub async fn knock_room(
        &self,
        room_id_or_alias: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<Value> {
        let encoded = crate::client::encode_path_component(room_id_or_alias);
        let body = reason
            .map(|r| serde_json::json!({ "reason": r }))
            .unwrap_or_default();
        self.client
            .raw_json(
                reqwest::Method::POST,
                &format!("/_matrix/client/unstable/knock/{encoded}"),
                Some(body),
            )
            .await
    }

    pub async fn get_threads(&self, room_id: &str, include: Option<&str>) -> anyhow::Result<Value> {
        let encoded = crate::client::encode_path_component(room_id);
        let mut endpoint =
            format!("/_matrix/client/unstable/org.matrix.msc3440/rooms/{encoded}/threads");
        if let Some(inc) = include {
            endpoint.push_str(&format!("?include={}", inc));
        }
        self.client
            .raw_json(reqwest::Method::GET, &endpoint, None)
            .await
    }

    pub async fn get_relations_for_event(
        &self,
        room_id: &str,
        event_id: &str,
        relation_type: Option<&str>,
        event_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        let room_enc = crate::client::encode_path_component(room_id);
        let event_enc = crate::client::encode_path_component(event_id);

        let mut endpoint =
            format!("/_matrix/client/unstable/rooms/{room_enc}/relations/{event_enc}");
        if let Some(rel_type) = relation_type {
            endpoint.push('/');
            endpoint.push_str(&crate::client::encode_path_component(rel_type));
        }
        if let Some(kind) = event_type {
            endpoint.push('/');
            endpoint.push_str(&crate::client::encode_path_component(kind));
        }

        self.client
            .raw_json(reqwest::Method::GET, &endpoint, None)
            .await
    }

    pub async fn get_related_events(&self, room_id: &str, event_id: &str) -> anyhow::Result<Value> {
        let room_enc = crate::client::encode_path_component(room_id);
        let event_enc = crate::client::encode_path_component(event_id);
        self.client
            .raw_json(
                reqwest::Method::GET,
                &format!("/_matrix/client/unstable/org.matrix.msc2675/rooms/{room_enc}/relations/{event_enc}"),
                None,
            )
            .await
    }

    pub async fn space_summary(
        &self,
        room_id: &str,
        max_rooms_per_space: Option<u32>,
    ) -> anyhow::Result<Value> {
        let encoded = crate::client::encode_path_component(room_id);
        let mut endpoint =
            format!("/_matrix/client/unstable/org.matrix.msc2946/rooms/{encoded}/hierarchy");
        if let Some(max) = max_rooms_per_space {
            endpoint.push_str(&format!("?max_rooms_per_space={}", max));
        }
        self.client
            .raw_json(reqwest::Method::GET, &endpoint, None)
            .await
    }

    pub async fn get_media_info(
        &self,
        mxc_url: &str,
    ) -> anyhow::Result<crate::models::unstable::MSC2380MediaInfo> {
        let stripped = mxc_url
            .strip_prefix("mxc://")
            .or_else(|| mxc_url.strip_prefix("MXC://"))
            .ok_or_else(|| anyhow::anyhow!("'mxc_url' does not begin with mxc://"))?;
        let (domain, media_id) = stripped
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("missing domain or media ID"))?;
        if domain.is_empty() || media_id.is_empty() {
            anyhow::bail!("missing domain or media ID");
        }

        let endpoint = format!(
            "/_matrix/media/unstable/info/{}/{}",
            crate::client::encode_path_component(domain),
            crate::client::encode_path_component(media_id)
        );
        let response = self
            .client
            .raw_json(reqwest::Method::GET, &endpoint, None)
            .await?;
        Ok(serde_json::from_value(response)?)
    }

    pub async fn get_media_config(&self) -> anyhow::Result<Value> {
        self.client
            .raw_json(reqwest::Method::GET, "/_matrix/media/unstable/config", None)
            .await
    }
}
