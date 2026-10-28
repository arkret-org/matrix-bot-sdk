use reqwest::Method;
use serde_json::Value;
use url::Url;

use crate::http::MatrixHttp;

#[derive(Clone)]
pub struct PantalaimonClient {
    http: MatrixHttp,
    access_token: String,
}

impl PantalaimonClient {
    pub fn new(base_url: Url, access_token: impl Into<String>) -> Self {
        Self {
            http: MatrixHttp::new(base_url),
            access_token: access_token.into(),
        }
    }

    pub async fn health(&self) -> anyhow::Result<Value> {
        self.http
            .send_json(Method::GET, "/health", Some(&self.access_token), None)
            .await
    }
}

#[derive(Clone)]
pub struct IdentityClient {
    http: MatrixHttp,
    access_token: String,
}

impl IdentityClient {
    pub fn new(base_url: Url, access_token: impl Into<String>) -> Self {
        Self {
            http: MatrixHttp::new(base_url),
            access_token: access_token.into(),
        }
    }

    pub async fn lookup_3pid(&self, medium: &str, address: &str) -> anyhow::Result<Value> {
        let endpoint = format!("/_matrix/identity/v2/lookup?medium={medium}&address={address}");
        self.http
            .send_json(Method::GET, &endpoint, Some(&self.access_token), None)
            .await
    }
}

#[derive(Clone)]
pub struct MatrixContentScannerClient {
    http: MatrixHttp,
    access_token: String,
}

impl MatrixContentScannerClient {
    pub fn new(base_url: Url, access_token: impl Into<String>) -> Self {
        Self {
            http: MatrixHttp::new(base_url),
            access_token: access_token.into(),
        }
    }

    pub async fn scan_event(&self, event: Value) -> anyhow::Result<Value> {
        self.http
            .send_json(
                Method::POST,
                "/_matrix/client/unstable/org.example.content_scanner/scan",
                Some(&self.access_token),
                Some(&event),
            )
            .await
    }
}
