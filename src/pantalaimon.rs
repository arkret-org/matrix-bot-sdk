use std::sync::Arc;

use crate::client::{MatrixAuth, MatrixClient};
use crate::storage::IStorageProvider;

const ACCESS_TOKEN_STORAGE_KEY: &str = "pantalaimon_access_token";

/// Supporting functions for interacting with a Pantalaimon instance.
pub struct PantalaimonClient {
    homeserver_url: reqwest::Url,
    storage_provider: Arc<dyn IStorageProvider>,
}

impl PantalaimonClient {
    pub fn new(homeserver_url: reqwest::Url, storage_provider: Arc<dyn IStorageProvider>) -> Self {
        Self {
            homeserver_url,
            storage_provider,
        }
    }

    /// Authenticates and creates the Pantalaimon-capable client.
    /// The username and password given are only used if the storage provider does not reveal an
    /// access token.
    pub async fn create_client_with_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> anyhow::Result<MatrixClient> {
        let access_token = self
            .storage_provider
            .get(ACCESS_TOKEN_STORAGE_KEY)
            .await?
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        if let Some(token) = access_token {
            let client = MatrixClient::new(self.homeserver_url.clone(), MatrixAuth::new(&token))
                .with_storage(self.storage_provider.clone());
            return Ok(client);
        }

        let mut auth = MatrixAuth::new("");
        let temp_client = MatrixClient::new(self.homeserver_url.clone(), auth.clone());
        let authed_token = temp_client
            .password_login(username, password, None)
            .await?
            .access_token;
        auth = MatrixAuth::new(&authed_token);

        self.storage_provider
            .set(
                ACCESS_TOKEN_STORAGE_KEY,
                serde_json::Value::String(authed_token.clone()),
            )
            .await?;

        let final_client = MatrixClient::new(self.homeserver_url.clone(), auth)
            .with_storage(self.storage_provider.clone());
        Ok(final_client)
    }
}
