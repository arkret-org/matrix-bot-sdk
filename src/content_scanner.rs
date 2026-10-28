use serde::{Deserialize, Serialize};

use crate::client::MatrixClient;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContentScannerResult {
    pub info: String,
    pub clean: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedFile {
    pub url: String,
    // Other fields omitted for brevity; this matches common matrix spec
}

/// API client for https://github.com/element-hq/matrix-content-scanner-python.
pub struct MatrixContentScannerClient<'a> {
    pub client: &'a MatrixClient,
}

impl<'a> MatrixContentScannerClient<'a> {
    pub fn new(client: &'a MatrixClient) -> Self {
        Self { client }
    }

    pub async fn scan_content(
        &self,
        domain: &str,
        media_id: &str,
    ) -> anyhow::Result<ContentScannerResult> {
        let path = format!("/_matrix/media_proxy/unstable/scan/{}/{}", domain, media_id);
        let resp = self
            .client
            .do_request(reqwest::Method::GET, &path, None, None)
            .await?;
        Ok(serde_json::from_value(resp)?)
    }

    pub async fn scan_content_encrypted(
        &self,
        file: &EncryptedFile,
    ) -> anyhow::Result<ContentScannerResult> {
        let path = "/_matrix/media_proxy/unstable/scan_encrypted".to_string();
        let body = serde_json::json!({ "file": file });
        let resp = self
            .client
            .do_request(reqwest::Method::POST, &path, None, Some(&body))
            .await?;
        Ok(serde_json::from_value(resp)?)
    }

    pub async fn download_content(&self, domain: &str, media_id: &str) -> anyhow::Result<Vec<u8>> {
        let endpoint = format!(
            "/_matrix/media_proxy/unstable/download/{}/{}",
            domain, media_id
        );

        let url = self.client.homeserver().join(&endpoint)?;
        let client = reqwest::Client::new();
        let mut req = client.request(reqwest::Method::GET, url);
        if let Some(token) = self.client.access_token().await {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Error fetching file: {}", resp.status());
        }

        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn download_encrypted_content(
        &self,
        file: &EncryptedFile,
    ) -> anyhow::Result<Vec<u8>> {
        let endpoint = "/_matrix/media_proxy/unstable/download_encrypted";
        let url = self.client.homeserver().join(endpoint)?;

        let client = reqwest::Client::new();
        let mut req = client.request(reqwest::Method::POST, url);
        if let Some(token) = self.client.access_token().await {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let body = serde_json::json!({ "file": file });
        req = req.json(&body);

        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Error fetching encrypted file: {}", resp.status());
        }

        Ok(resp.bytes().await?.to_vec())
    }
}
