use anyhow::Context;
use async_trait::async_trait;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

#[derive(Debug, Clone)]
pub struct MatrixHttp {
    client: reqwest::Client,
    homeserver: Url,
}

impl MatrixHttp {
    pub fn new(homeserver: Url) -> Self {
        Self {
            client: reqwest::Client::new(),
            homeserver,
        }
    }

    pub fn homeserver(&self) -> &Url {
        &self.homeserver
    }

    pub async fn send_json(
        &self,
        method: Method,
        endpoint: &str,
        access_token: Option<&str>,
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let url = self.url(endpoint)?;
        let mut req = self.client.request(method, url);
        if let Some(token) = access_token {
            req = req.bearer_auth(token);
        }
        if let Some(payload) = body {
            req = req.json(payload);
        }
        let response = req.send().await?;
        let status = response.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let payload: Value = response
            .json()
            .await
            .with_context(|| format!("failed to decode Matrix response with status {status}"))?;
        Ok(payload)
    }

    fn url(&self, endpoint: &str) -> anyhow::Result<Url> {
        let normalized = endpoint.trim_start_matches('/');
        Ok(self.homeserver.join(normalized)?)
    }
}

#[async_trait]
pub trait Request {
    type Response: DeserializeOwned + Send + Sync + 'static;

    fn method(&self) -> Method;
    fn endpoint(&self) -> &str;
    fn body(&self) -> Option<Value> {
        None
    }
}

pub async fn request<R: Request>(
    http: &MatrixHttp,
    req: &R,
    access_token: Option<&str>,
) -> anyhow::Result<R::Response> {
    let json = http
        .send_json(
            req.method(),
            req.endpoint(),
            access_token,
            req.body().as_ref(),
        )
        .await?;
    Ok(serde_json::from_value(json)?)
}

pub mod b64 {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD_NO_PAD;

    pub fn encode_unpadded(bytes: &[u8]) -> String {
        STANDARD_NO_PAD.encode(bytes)
    }

    pub fn decode_unpadded(value: &str) -> anyhow::Result<Vec<u8>> {
        Ok(STANDARD_NO_PAD.decode(value)?)
    }
}

pub mod simple_validation {
    pub fn required_non_empty(name: &'static str, value: &str) -> anyhow::Result<()> {
        if value.trim().is_empty() {
            anyhow::bail!("{name} cannot be empty");
        }
        Ok(())
    }

    pub fn ensure_prefix(
        value: &str,
        prefix: &str,
        field_name: &'static str,
    ) -> anyhow::Result<()> {
        if !value.starts_with(prefix) {
            anyhow::bail!("{field_name} must start with '{prefix}'");
        }
        Ok(())
    }
}
