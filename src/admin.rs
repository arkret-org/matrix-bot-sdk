use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::MatrixClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoisInfo {
    pub user_id: String,
    pub devices: std::collections::HashMap<String, WhoisDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoisDevice {
    pub sessions: Vec<WhoisSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoisSession {
    pub connections: Vec<WhoisConnectionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoisConnectionInfo {
    pub ip: String,
    pub last_seen: u64,
    pub user_agent: String,
}

#[derive(Clone)]
pub struct AdminApis {
    client: MatrixClient,
}

impl AdminApis {
    pub fn new(client: MatrixClient) -> Self {
        Self { client }
    }

    pub fn synapse(&self) -> SynapseAdminApis {
        SynapseAdminApis::new(self.client.clone())
    }

    pub async fn whois_user(&self, user_id: &str) -> anyhow::Result<WhoisInfo> {
        let encoded = encode_path(user_id);
        let endpoint = format!("/_matrix/client/v3/admin/whois/{encoded}");
        let response = self.client.raw_json(Method::GET, &endpoint, None).await?;
        serde_json::from_value(response).map_err(Into::into)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
    pub next_token: Option<String>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub name: String,
    #[serde(default)]
    pub displayname: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub admin: Option<bool>,
    #[serde(default)]
    pub deactivated: Option<bool>,
    #[serde(default)]
    pub user_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomListResponse {
    pub rooms: Vec<RoomInfo>,
    #[serde(default)]
    pub offset: Option<String>,
    pub next_batch: Option<String>,
    #[serde(default)]
    pub prev_batch: Option<String>,
    pub total_rooms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: String,
    pub name: Option<String>,
    pub canonical_alias: Option<String>,
    pub joined_members: Option<u64>,
    pub joined_local_members: Option<u64>,
    pub version: Option<String>,
    pub creator: Option<String>,
    pub encryption: Option<String>,
    pub federatable: Option<bool>,
    pub public: Option<bool>,
    pub join_rules: Option<String>,
    pub guest_access: Option<String>,
    pub history_visibility: Option<String>,
    pub state_events: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SynapseRoomProperty {
    Name,
    CanonicalAlias,
    JoinedMembers,
    JoinedLocalMembers,
    Version,
    Creator,
    Encryption,
    Federatable,
    Public,
    JoinRules,
    GuestAccess,
    HistoryVisibility,
    StateEvents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatsResponse {
    pub total_users: Option<u64>,
    pub total_nonbridged_users: Option<u64>,
    pub total_room_count: Option<u64>,
    pub daily_active_users: Option<u64>,
    pub monthly_active_users: Option<u64>,
    #[serde(default)]
    pub r30_users: Option<u64>,
    #[serde(default)]
    pub r30v2_users: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SynapseUserProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logout_devices: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseRegistrationToken {
    pub token: String,
    pub uses_allowed: Option<u64>,
    pub pending: u64,
    pub completed: u64,
    pub expiry_time: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SynapseRegistrationTokenUpdateOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses_allowed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_time: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SynapseRegistrationTokenOptions {
    #[serde(flatten)]
    pub update_options: SynapseRegistrationTokenUpdateOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
}

#[derive(Clone)]
pub struct SynapseAdminApis {
    client: MatrixClient,
}

impl SynapseAdminApis {
    pub fn new(client: MatrixClient) -> Self {
        Self { client }
    }

    pub async fn get_user(&self, user_id: &str) -> anyhow::Result<SynapseUser> {
        let encoded = encode_path(user_id);
        let endpoint = format!("/_synapse/admin/v2/users/{encoded}");
        let response = self.client.raw_json(Method::GET, &endpoint, None).await?;
        serde_json::from_value(response).map_err(Into::into)
    }

    pub async fn upsert_user(
        &self,
        user_id: &str,
        opts: &SynapseUserProperties,
    ) -> anyhow::Result<SynapseUser> {
        let encoded = encode_path(user_id);
        let endpoint = format!("/_synapse/admin/v2/users/{encoded}");
        let body = serde_json::to_value(opts)?;
        let response = self
            .client
            .raw_json(Method::PUT, &endpoint, Some(body))
            .await?;
        serde_json::from_value(response).map_err(Into::into)
    }

    pub async fn list_users(
        &self,
        from: Option<&str>,
        limit: Option<u64>,
        name: Option<&str>,
        guests: bool,
        deactivated: bool,
    ) -> anyhow::Result<UserListResponse> {
        let mut params = vec![
            format!("guests={guests}"),
            format!("deactivated={deactivated}"),
        ];
        if let Some(v) = from {
            params.push(format!(
                "from={}",
                percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(v) = limit {
            params.push(format!("limit={v}"));
        }
        if let Some(v) = name {
            params.push(format!(
                "name={}",
                percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        let endpoint = format!("/_synapse/admin/v2/users?{}", params.join("&"));
        let response = self.client.raw_json(Method::GET, &endpoint, None).await?;
        serde_json::from_value(response).map_err(Into::into)
    }

    pub async fn is_admin(&self, user_id: &str) -> anyhow::Result<bool> {
        let encoded = encode_path(user_id);
        let endpoint = format!("/_synapse/admin/v1/users/{encoded}/admin");
        let response = self.client.raw_json(Method::GET, &endpoint, None).await?;
        Ok(response
            .get("admin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    pub async fn is_self_admin(&self) -> anyhow::Result<bool> {
        let user_id = self.client.get_user_id().await?;
        match self.is_admin(&user_id).await {
            Ok(is_admin) => Ok(is_admin),
            Err(e) => {
                // If the error is a MatrixError M_FORBIDDEN (403), return false instead of failing
                if e.to_string().contains("M_FORBIDDEN") {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn get_room_members(&self, room_id: &str) -> anyhow::Result<Value> {
        let encoded = encode_path(room_id);
        let endpoint = format!("/_synapse/admin/v1/rooms/{encoded}/members");
        self.client.raw_json(Method::GET, &endpoint, None).await
    }

    pub async fn list_rooms(
        &self,
        search_term: Option<&str>,
        from: Option<&str>,
        limit: Option<u64>,
        order_by: Option<SynapseRoomProperty>,
        reverse_order: bool,
    ) -> anyhow::Result<RoomListResponse> {
        let mut params = Vec::new();
        if let Some(v) = from {
            params.push(format!(
                "from={}",
                percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(v) = limit {
            params.push(format!("limit={v}"));
        }
        if let Some(v) = search_term {
            params.push(format!(
                "search_term={}",
                percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(v) = order_by {
            let field = serde_json::to_value(v)?
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid order_by field"))?
                .to_owned();
            params.push(format!(
                "order_by={}",
                percent_encoding::utf8_percent_encode(&field, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        params.push(format!("dir={}", if reverse_order { "b" } else { "f" }));

        let endpoint = format!("/_synapse/admin/v1/rooms?{}", params.join("&"));
        let response = self.client.raw_json(Method::GET, &endpoint, None).await?;
        serde_json::from_value(response).map_err(Into::into)
    }

    pub async fn get_room_state(&self, room_id: &str) -> anyhow::Result<Value> {
        let encoded = encode_path(room_id);
        let endpoint = format!("/_synapse/admin/v1/rooms/{encoded}/state");
        self.client.raw_json(Method::GET, &endpoint, None).await
    }

    pub async fn make_room_admin(
        &self,
        room_id: &str,
        user_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let encoded = encode_path(room_id);
        let endpoint = format!("/_synapse/admin/v1/rooms/{encoded}/make_room_admin");
        let body = if let Some(uid) = user_id {
            json!({ "user_id": uid })
        } else {
            json!({})
        };
        self.client
            .raw_json(Method::POST, &endpoint, Some(body))
            .await?;
        Ok(())
    }

    pub async fn get_server_stats(&self) -> anyhow::Result<ServerStatsResponse> {
        let response = self
            .client
            .raw_json(Method::GET, "/_synapse/admin/v1/statistics", None)
            .await?;
        serde_json::from_value(response).map_err(Into::into)
    }

    pub async fn get_room_details(&self, room_id: &str) -> anyhow::Result<Value> {
        let encoded = encode_path(room_id);
        let endpoint = format!("/_synapse/admin/v1/rooms/{encoded}");
        self.client.raw_json(Method::GET, &endpoint, None).await
    }

    pub async fn delete_room(
        &self,
        room_id: &str,
        purge: bool,
        force_purge: bool,
        block: bool,
    ) -> anyhow::Result<String> {
        let encoded = encode_path(room_id);
        let endpoint = format!("/_synapse/admin/v2/rooms/{encoded}/delete");
        let body = json!({
            "purge": purge,
            "force_purge": force_purge,
            "block": block,
        });
        let response = self
            .client
            .raw_json(Method::POST, &endpoint, Some(body))
            .await?;
        response
            .get("delete_id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("missing delete_id in response"))
    }

    pub async fn get_delete_room_status(&self, delete_id: &str) -> anyhow::Result<Value> {
        let encoded = encode_path(delete_id);
        let endpoint = format!("/_synapse/admin/v2/rooms/delete/{encoded}/status");
        self.client.raw_json(Method::GET, &endpoint, None).await
    }

    pub async fn get_delete_room_state(&self, room_id: &str) -> anyhow::Result<Vec<Value>> {
        let encoded = encode_path(room_id);
        let endpoint = format!("/_synapse/admin/v2/rooms/{encoded}/delete_status");
        let response = self.client.raw_json(Method::GET, &endpoint, None).await?;
        Ok(response
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    pub async fn get_user_media(
        &self,
        user_id: &str,
        limit: Option<u64>,
        from: Option<&str>,
    ) -> anyhow::Result<Value> {
        let encoded = encode_path(user_id);
        let mut endpoint = format!("/_synapse/admin/v1/users/{encoded}/media");
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }
        if let Some(f) = from {
            params.push(format!("from={}", f));
        }
        if !params.is_empty() {
            endpoint.push('?');
            endpoint.push_str(&params.join("&"));
        }
        self.client.raw_json(Method::GET, &endpoint, None).await
    }

    pub async fn get_user_joined_rooms(&self, user_id: &str) -> anyhow::Result<Value> {
        let encoded = encode_path(user_id);
        let endpoint = format!("/_synapse/admin/v1/users/{encoded}/joined_rooms");
        self.client.raw_json(Method::GET, &endpoint, None).await
    }

    pub async fn shadow_ban(&self, user_id: &str) -> anyhow::Result<()> {
        let encoded = encode_path(user_id);
        let endpoint = format!("/_synapse/admin/v1/users/{encoded}/shadow_ban");
        self.client
            .raw_json(Method::POST, &endpoint, Some(json!({})))
            .await?;
        Ok(())
    }

    pub async fn unshadow_ban(&self, user_id: &str) -> anyhow::Result<()> {
        let encoded = encode_path(user_id);
        let endpoint = format!("/_synapse/admin/v1/users/{encoded}/shadow_ban/unban");
        self.client
            .raw_json(Method::POST, &endpoint, Some(json!({})))
            .await?;
        Ok(())
    }

    pub async fn get_server_version(&self) -> anyhow::Result<Value> {
        self.client
            .raw_json(Method::GET, "/_synapse/admin/v1/server_version", None)
            .await
    }

    pub async fn purge_history(
        &self,
        room_id: &str,
        purge_up_to_ts: Option<u64>,
        delete_local_events: bool,
    ) -> anyhow::Result<String> {
        let encoded = encode_path(room_id);
        let endpoint = format!("/_synapse/admin/v1/purge_history/{encoded}");
        let mut body = json!({ "delete_local_events": delete_local_events });
        if let Some(ts) = purge_up_to_ts {
            body["purge_up_to_ts"] = json!(ts);
        }
        let response = self
            .client
            .raw_json(Method::POST, &endpoint, Some(body))
            .await?;
        response
            .get("purge_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("missing purge_id in response"))
    }

    pub async fn get_purge_status(&self, purge_id: &str) -> anyhow::Result<Value> {
        let encoded = encode_path(purge_id);
        let endpoint = format!("/_synapse/admin/v1/purge_history_status/{encoded}");
        self.client.raw_json(Method::GET, &endpoint, None).await
    }

    pub async fn list_registration_tokens(
        &self,
        valid: Option<bool>,
    ) -> anyhow::Result<Vec<SynapseRegistrationToken>> {
        let endpoint = "/_synapse/admin/v1/registration_tokens";
        let endpoint = if let Some(v) = valid {
            format!("{endpoint}?valid={v}")
        } else {
            endpoint.to_string()
        };
        let response = self.client.raw_json(Method::GET, &endpoint, None).await?;
        Ok(response
            .get("registration_tokens")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    pub async fn get_registration_token(
        &self,
        token: &str,
    ) -> anyhow::Result<Option<SynapseRegistrationToken>> {
        let encoded = encode_path(token);
        let endpoint = format!("/_synapse/admin/v1/registration_tokens/{encoded}");
        match self.client.raw_json(Method::GET, &endpoint, None).await {
            Ok(response) => Ok(Some(serde_json::from_value(response)?)),
            Err(e) if e.to_string().contains("404") => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn create_registration_token(
        &self,
        options: &SynapseRegistrationTokenOptions,
    ) -> anyhow::Result<SynapseRegistrationToken> {
        let endpoint = "/_synapse/admin/v1/registration_tokens/new";
        let body = serde_json::to_value(options)?;
        let response = self
            .client
            .raw_json(Method::POST, endpoint, Some(body))
            .await?;
        serde_json::from_value(response).map_err(Into::into)
    }

    pub async fn update_registration_token(
        &self,
        token: &str,
        options: &SynapseRegistrationTokenUpdateOptions,
    ) -> anyhow::Result<SynapseRegistrationToken> {
        let encoded = encode_path(token);
        let endpoint = format!("/_synapse/admin/v1/registration_tokens/{encoded}");
        let body = serde_json::to_value(options)?;
        let response = self
            .client
            .raw_json(Method::PUT, &endpoint, Some(body))
            .await?;
        serde_json::from_value(response).map_err(Into::into)
    }

    pub async fn delete_registration_token(&self, token: &str) -> anyhow::Result<()> {
        let encoded = encode_path(token);
        let endpoint = format!("/_synapse/admin/v1/registration_tokens/{encoded}");
        self.client
            .raw_json(Method::DELETE, &endpoint, None)
            .await?;
        Ok(())
    }

    pub async fn get_event_nearest_to_timestamp(
        &self,
        room_id: &str,
        ts: u64,
        dir: &str,
    ) -> anyhow::Result<Value> {
        let encoded = encode_path(room_id);
        let endpoint =
            format!("/_synapse/admin/v1/rooms/{encoded}/timestamp_to_event?ts={ts}&dir={dir}");
        self.client.raw_json(Method::GET, &endpoint, None).await
    }
}

fn encode_path(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}
