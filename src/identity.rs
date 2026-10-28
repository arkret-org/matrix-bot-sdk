use std::sync::Arc;

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::client::MatrixClient;
use crate::metrics::Metrics;
use crate::models::{OpenIDConnect, Threepid};

/// A way to access an Identity Server using the Identity Service API from a `MatrixClient`.
#[derive(Clone)]
pub struct IdentityClient {
    pub metrics: Arc<Metrics>,
    pub brand: Option<String>,
    pub access_token: String,
    pub server_url: Url,
    pub matrix_client: Arc<MatrixClient>,
}

impl IdentityClient {
    pub fn new(access_token: &str, server_url: Url, matrix_client: Arc<MatrixClient>) -> Self {
        Self {
            metrics: Arc::new(Metrics::default()),
            brand: None,
            access_token: access_token.to_string(),
            server_url,
            matrix_client,
        }
    }

    /// Performs a web request to the server, applying appropriate authorization headers for this
    /// client.
    pub async fn do_request(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        qs: Option<&[(&str, &str)]>,
        body: Option<Value>,
    ) -> reqwest::Result<reqwest::Response> {
        let mut url = self.server_url.join(endpoint).unwrap();

        if let Some(qs) = qs {
            let mut serializer = url.query_pairs_mut();
            for (key, val) in qs {
                serializer.append_pair(key, val);
            }
        }

        let client = reqwest::Client::new();
        let mut req = client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.access_token));

        if let Some(body) = body {
            req = req.json(&body);
        }

        req.send().await
    }

    /// Gets account information for the logged in user.
    pub async fn get_account(&self) -> reqwest::Result<Value> {
        let resp = self
            .do_request(
                reqwest::Method::GET,
                "/_matrix/identity/v2/account",
                None,
                None,
            )
            .await?;
        resp.json().await
    }

    /// Gets the terms of service for which the identity server has.
    pub async fn get_terms_of_service(&self) -> reqwest::Result<Value> {
        let resp = self
            .do_request(
                reqwest::Method::GET,
                "/_matrix/identity/v2/terms",
                None,
                None,
            )
            .await?;
        resp.json().await
    }

    /// Accepts a given set of URLs from Policy objects returned by the server.
    pub async fn accept_terms(&self, terms_urls: Vec<String>) -> reqwest::Result<()> {
        let body = serde_json::json!({
            "user_accepts": terms_urls
        });
        self.do_request(
            reqwest::Method::POST,
            "/_matrix/identity/v2/terms",
            None,
            Some(body),
        )
        .await?;
        Ok(())
    }

    /// Accepts all terms of service offered by the identity server.
    pub async fn accept_all_terms(&self) -> anyhow::Result<()> {
        let terms = self.get_terms_of_service().await?;
        let mut accepted = Vec::new();

        if let Some(policies) = terms.get("policies").and_then(Value::as_object) {
            for policy in policies.values() {
                if let Some(policy_obj) = policy.as_object() {
                    if let Some(url) = policy_obj
                        .get("en")
                        .and_then(Value::as_object)
                        .and_then(|lang| lang.get("url"))
                        .and_then(Value::as_str)
                    {
                        accepted.push(url.to_owned());
                        continue;
                    }

                    for (key, value) in policy_obj {
                        if key == "version" {
                            continue;
                        }
                        if let Some(url) = value
                            .as_object()
                            .and_then(|lang| lang.get("url"))
                            .and_then(Value::as_str)
                        {
                            accepted.push(url.to_owned());
                            break;
                        }
                    }
                }
            }
        }

        // Keep ordering stable but remove duplicates.
        accepted.sort();
        accepted.dedup();
        self.accept_terms(accepted).await?;
        Ok(())
    }

    /// Looks up a series of third party identifiers.
    pub async fn lookup(
        &self,
        addresses: Vec<String>,
        algorithm: &str,
        pepper: &str,
    ) -> reqwest::Result<Value> {
        let body = serde_json::json!({
            "algorithm": algorithm,
            "pepper": pepper,
            "addresses": addresses
        });
        let resp = self
            .do_request(
                reqwest::Method::POST,
                "/_matrix/identity/v2/lookup",
                None,
                Some(body),
            )
            .await?;
        resp.json().await
    }

    /// Looks up third-party identifiers and returns mxids in the same order as input.
    ///
    /// Mirrors the TypeScript SDK behaviour: it negotiates hashing via `/hash_details`,
    /// prefers `sha256`, and can optionally fall back to plaintext (`none`).
    pub async fn lookup_threepids(
        &self,
        identifiers: &[Threepid],
        allow_plaintext: bool,
    ) -> anyhow::Result<Vec<Option<String>>> {
        let hash_details_resp = self
            .do_request(
                reqwest::Method::GET,
                "/_matrix/identity/v2/hash_details",
                None,
                None,
            )
            .await?;
        let hash_details: Value = hash_details_resp.json().await?;

        let algorithms = hash_details
            .get("algorithms")
            .and_then(Value::as_array)
            .context("Server not supported: missing algorithms")?;

        let has_sha256 = algorithms.iter().any(|a| a.as_str() == Some("sha256"));
        let has_none = algorithms.iter().any(|a| a.as_str() == Some("none"));
        let algorithm = if has_sha256 {
            "sha256"
        } else if allow_plaintext && has_none {
            "none"
        } else {
            anyhow::bail!("No supported hashing algorithm found");
        };

        let pepper = hash_details
            .get("lookup_pepper")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let mut transformed = Vec::with_capacity(identifiers.len());
        for pid in identifiers {
            let source = format!("{} {}", pid.address.to_lowercase(), pid.medium);
            let encoded = match algorithm {
                "none" => source,
                "sha256" => {
                    let mut hasher = Sha256::new();
                    hasher.update(format!("{source} {pepper}"));
                    URL_SAFE_NO_PAD.encode(hasher.finalize())
                }
                _ => anyhow::bail!("Unsupported hashing algorithm"),
            };
            transformed.push(encoded);
        }

        let lookup_response = self.lookup(transformed.clone(), algorithm, pepper).await?;
        let mappings = lookup_response
            .get("mappings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        Ok(transformed
            .iter()
            .map(|k| {
                mappings
                    .get(k)
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect())
    }

    /// Creates a third party email invite.
    pub async fn make_email_invite(
        &self,
        email_address: &str,
        room_id: &str,
    ) -> reqwest::Result<Value> {
        let sender = self.matrix_client.get_user_id().await.unwrap_or_default();

        let body = serde_json::json!({
            "address": email_address,
            "room_id": room_id,
            "medium": "email",
            "sender": sender,
        });

        let mut qs = Vec::new();
        if let Some(brand) = &self.brand {
            qs.push(("brand", brand.as_str()));
        }

        let qs_slice = if qs.is_empty() {
            None
        } else {
            Some(qs.as_slice())
        };

        let resp = self
            .do_request(
                reqwest::Method::POST,
                "/_matrix/identity/v2/store-invite",
                qs_slice,
                Some(body),
            )
            .await?;

        resp.json().await
    }

    /// Registers an OpenID Connect token against an identity server and acquires a client.
    pub async fn acquire(
        oidc: &OpenIDConnect,
        server_url: Url,
        matrix_client: Arc<MatrixClient>,
    ) -> anyhow::Result<Self> {
        let client = reqwest::Client::new();
        let endpoint = server_url.join("/_matrix/identity/v2/account/register")?;
        let response = client.post(endpoint).json(oidc).send().await?;
        let body: Value = response.json().await?;
        let token = body
            .get("token")
            .and_then(Value::as_str)
            .context("Identity server registration response missing token")?;

        Ok(Self::new(token, server_url, matrix_client))
    }
}
