use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;

#[async_trait]
pub trait IStorageProvider: Send + Sync {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Value>>;
    async fn set(&self, key: &str, value: Value) -> anyhow::Result<()>;
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
    async fn keys(&self) -> anyhow::Result<Vec<String>>;
}

#[async_trait]
pub trait IAppserviceStorageProvider: IStorageProvider {
    async fn register_user(&self, user_id: &str) -> anyhow::Result<()>;
    async fn is_user_registered(&self, user_id: &str) -> anyhow::Result<bool>;
}

#[async_trait]
pub trait ICryptoStorageProvider: IStorageProvider {
    async fn store_session_key(&self, room_id: &str, session_key: &str) -> anyhow::Result<()>;
    async fn session_key(&self, room_id: &str) -> anyhow::Result<Option<String>>;
}

#[derive(Debug, Default)]
pub struct MemoryStorageProvider {
    entries: RwLock<HashMap<String, Value>>,
}

impl MemoryStorageProvider {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl IStorageProvider for MemoryStorageProvider {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Value>> {
        Ok(self.entries.read().await.get(key).cloned())
    }

    async fn set(&self, key: &str, value: Value) -> anyhow::Result<()> {
        self.entries.write().await.insert(key.to_owned(), value);
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.entries.write().await.remove(key);
        Ok(())
    }

    async fn keys(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.entries.read().await.keys().cloned().collect())
    }
}

#[async_trait]
impl IAppserviceStorageProvider for MemoryStorageProvider {
    async fn register_user(&self, user_id: &str) -> anyhow::Result<()> {
        self.set(
            &format!("appservice:registered:{user_id}"),
            Value::Bool(true),
        )
        .await
    }

    async fn is_user_registered(&self, user_id: &str) -> anyhow::Result<bool> {
        Ok(self
            .get(&format!("appservice:registered:{user_id}"))
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }
}

#[async_trait]
impl ICryptoStorageProvider for MemoryStorageProvider {
    async fn store_session_key(&self, room_id: &str, session_key: &str) -> anyhow::Result<()> {
        self.set(
            &format!("crypto:session:{room_id}"),
            Value::String(session_key.to_owned()),
        )
        .await
    }

    async fn session_key(&self, room_id: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .get(&format!("crypto:session:{room_id}"))
            .await?
            .and_then(|v| v.as_str().map(ToOwned::to_owned)))
    }
}

#[derive(Debug)]
pub struct SimpleFsStorageProvider {
    file_path: PathBuf,
    entries: RwLock<HashMap<String, Value>>,
}

impl SimpleFsStorageProvider {
    pub async fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let file_path = path.as_ref().to_path_buf();
        let entries = if tokio::fs::try_exists(&file_path).await? {
            let content = tokio::fs::read_to_string(&file_path).await?;
            serde_json::from_str::<HashMap<String, Value>>(&content)?
        } else {
            HashMap::new()
        };
        Ok(Self {
            file_path,
            entries: RwLock::new(entries),
        })
    }

    async fn flush(&self) -> anyhow::Result<()> {
        let serialized = serde_json::to_string_pretty(&*self.entries.read().await)?;
        tokio::fs::write(&self.file_path, serialized).await?;
        Ok(())
    }
}

#[async_trait]
impl IStorageProvider for SimpleFsStorageProvider {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Value>> {
        Ok(self.entries.read().await.get(key).cloned())
    }

    async fn set(&self, key: &str, value: Value) -> anyhow::Result<()> {
        self.entries.write().await.insert(key.to_owned(), value);
        self.flush().await
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.entries.write().await.remove(key);
        self.flush().await
    }

    async fn keys(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.entries.read().await.keys().cloned().collect())
    }
}

#[async_trait]
impl IAppserviceStorageProvider for SimpleFsStorageProvider {
    async fn register_user(&self, user_id: &str) -> anyhow::Result<()> {
        self.set(
            &format!("appservice:registered:{user_id}"),
            Value::Bool(true),
        )
        .await
    }

    async fn is_user_registered(&self, user_id: &str) -> anyhow::Result<bool> {
        Ok(self
            .get(&format!("appservice:registered:{user_id}"))
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }
}

#[async_trait]
impl ICryptoStorageProvider for SimpleFsStorageProvider {
    async fn store_session_key(&self, room_id: &str, session_key: &str) -> anyhow::Result<()> {
        self.set(
            &format!("crypto:session:{room_id}"),
            Value::String(session_key.to_owned()),
        )
        .await
    }

    async fn session_key(&self, room_id: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .get(&format!("crypto:session:{room_id}"))
            .await?
            .and_then(|v| v.as_str().map(ToOwned::to_owned)))
    }
}

#[cfg(feature = "storage-postgres")]
#[derive(Debug)]
pub struct SimplePostgresStorageProvider {
    client: tokio_postgres::Client,
}

#[cfg(feature = "storage-postgres")]
impl SimplePostgresStorageProvider {
    pub async fn connect(conn_str: &str) -> anyhow::Result<Self> {
        let (client, connection) = tokio_postgres::connect(conn_str, tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::error!("postgres connection error: {error}");
            }
        });

        client
            .execute(
                "CREATE TABLE IF NOT EXISTS matrix_sdk_store (key TEXT PRIMARY KEY, value JSONB NOT NULL)",
                &[],
            )
            .await?;

        Ok(Self { client })
    }
}

#[cfg(feature = "storage-postgres")]
#[async_trait]
impl IStorageProvider for SimplePostgresStorageProvider {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Value>> {
        let row = self
            .client
            .query_opt("SELECT value FROM matrix_sdk_store WHERE key = $1", &[&key])
            .await?;
        Ok(row.map(|r| r.get(0)))
    }

    async fn set(&self, key: &str, value: Value) -> anyhow::Result<()> {
        self.client
            .execute(
                "INSERT INTO matrix_sdk_store (key, value) VALUES ($1, $2) \
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
                &[&key, &value],
            )
            .await?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .execute("DELETE FROM matrix_sdk_store WHERE key = $1", &[&key])
            .await?;
        Ok(())
    }

    async fn keys(&self) -> anyhow::Result<Vec<String>> {
        let rows = self
            .client
            .query("SELECT key FROM matrix_sdk_store ORDER BY key", &[])
            .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }
}

#[cfg(not(feature = "storage-postgres"))]
#[derive(Debug, Default)]
pub struct SimplePostgresStorageProvider;

#[cfg(not(feature = "storage-postgres"))]
impl SimplePostgresStorageProvider {
    pub async fn connect(_conn_str: &str) -> anyhow::Result<Self> {
        anyhow::bail!("feature 'storage-postgres' is not enabled");
    }
}

#[derive(Clone)]
pub struct RustSdkCryptoStorageProvider {
    inner: Arc<dyn IStorageProvider>,
}

impl RustSdkCryptoStorageProvider {
    pub fn new(inner: Arc<dyn IStorageProvider>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl IStorageProvider for RustSdkCryptoStorageProvider {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Value>> {
        self.inner.get(&format!("rust_crypto:{key}")).await
    }

    async fn set(&self, key: &str, value: Value) -> anyhow::Result<()> {
        self.inner.set(&format!("rust_crypto:{key}"), value).await
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.inner.delete(&format!("rust_crypto:{key}")).await
    }

    async fn keys(&self) -> anyhow::Result<Vec<String>> {
        let prefixed = self.inner.keys().await?;
        Ok(prefixed
            .into_iter()
            .filter_map(|k| k.strip_prefix("rust_crypto:").map(ToOwned::to_owned))
            .collect())
    }
}

#[async_trait]
impl ICryptoStorageProvider for RustSdkCryptoStorageProvider {
    async fn store_session_key(&self, room_id: &str, session_key: &str) -> anyhow::Result<()> {
        self.set(
            &format!("session:{room_id}"),
            Value::String(session_key.to_owned()),
        )
        .await
    }

    async fn session_key(&self, room_id: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .get(&format!("session:{room_id}"))
            .await?
            .and_then(|v| v.as_str().map(ToOwned::to_owned)))
    }
}
