use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use url::Url;
use uuid::Uuid;

use crate::http::MatrixHttp;
use crate::models::events::{Event, Membership, MembershipEvent, converter};
use crate::models::{Account, CreateRoom, MXCUrl, MatrixProfile, Presence};
use crate::preprocessors::IPreprocessor;
use crate::storage::IStorageProvider;

pub trait IFilter: Send + Sync {
    fn matches(&self, event: &Event) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct MatrixAuth {
    pub access_token: String,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
}

impl MatrixAuth {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            user_id: None,
            device_id: None,
        }
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = Some(device_id.into());
        self
    }
}

#[derive(Clone)]
pub struct MatrixClient {
    http: MatrixHttp,
    auth: Arc<RwLock<MatrixAuth>>,
    storage: Option<Arc<dyn IStorageProvider>>,
    preprocessors: Arc<RwLock<Vec<Arc<dyn IPreprocessor>>>>,
    filters: Arc<RwLock<Vec<Arc<dyn IFilter>>>>,
    impersonated_user_id: Arc<RwLock<Option<String>>>,
    impersonated_device_id: Arc<RwLock<Option<String>>>,
    join_strategy: Arc<RwLock<Option<Arc<dyn crate::strategies::JoinRoomStrategy>>>>,
    stop_syncing: Arc<AtomicBool>,
}

impl MatrixClient {
    pub fn new(homeserver: Url, auth: MatrixAuth) -> Self {
        Self {
            http: MatrixHttp::new(homeserver),
            auth: Arc::new(RwLock::new(auth)),
            storage: None,
            preprocessors: Arc::new(RwLock::new(Vec::new())),
            filters: Arc::new(RwLock::new(Vec::new())),
            impersonated_user_id: Arc::new(RwLock::new(None)),
            impersonated_device_id: Arc::new(RwLock::new(None)),
            join_strategy: Arc::new(RwLock::new(None)),
            stop_syncing: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_storage(mut self, storage: Arc<dyn IStorageProvider>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn homeserver(&self) -> &Url {
        self.http.homeserver()
    }

    pub async fn access_token(&self) -> Option<String> {
        let auth = self.auth.read().await;
        if auth.access_token.is_empty() {
            None
        } else {
            Some(auth.access_token.clone())
        }
    }

    pub async fn auth(&self) -> MatrixAuth {
        self.auth.read().await.clone()
    }

    pub async fn set_auth(&self, auth: MatrixAuth) {
        *self.auth.write().await = auth;
    }

    pub async fn add_preprocessor(&self, preprocessor: Arc<dyn IPreprocessor>) {
        self.preprocessors.write().await.push(preprocessor);
    }

    pub async fn add_filter(&self, filter: Arc<dyn IFilter>) {
        self.filters.write().await.push(filter);
    }

    pub async fn impersonate_user_id(
        &self,
        user_id: Option<impl Into<String>>,
        device_id: Option<impl Into<String>>,
    ) {
        let mut uid = self.impersonated_user_id.write().await;
        let mut did = self.impersonated_device_id.write().await;
        *uid = user_id.map(Into::into);
        *did = device_id.map(Into::into);
    }

    pub async fn set_join_strategy(&self, strategy: Arc<dyn crate::strategies::JoinRoomStrategy>) {
        *self.join_strategy.write().await = Some(strategy);
    }

    pub fn unstable_apis(&self) -> crate::e2ee::UnstableApis {
        crate::e2ee::UnstableApis::new(self.clone())
    }

    pub async fn get_identity_server_client(
        &self,
        identity_server_name: &str,
    ) -> anyhow::Result<crate::identity::IdentityClient> {
        let oidc_token = self.get_openid_connect_token().await?;
        let access_token = oidc_token
            .get("access_token")
            .and_then(Value::as_str)
            .context("missing access_token in OpenID token response")?;
        let server_url = Url::parse(&format!("https://{identity_server_name}"))?;
        Ok(crate::identity::IdentityClient::new(
            access_token,
            server_url,
            Arc::new(self.clone()),
        ))
    }

    pub async fn raw_json(
        &self,
        method: Method,
        endpoint: &str,
        body: Option<Value>,
    ) -> anyhow::Result<Value> {
        self.http
            .send_json(method, endpoint, Some(&self.token().await), body.as_ref())
            .await
    }

    pub async fn do_request(
        &self,
        method: Method,
        endpoint: &str,
        query: Option<&BTreeMap<String, String>>,
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let url = if let Some(q) = query {
            let qs = q
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            format!("{}?{}", endpoint, qs)
        } else {
            endpoint.to_owned()
        };
        self.raw_json(method, &url, body.cloned()).await
    }

    async fn token(&self) -> String {
        self.auth.read().await.access_token.clone()
    }
}

impl MatrixClient {
    pub async fn password_login(
        &self,
        username: &str,
        password: &str,
        device_name: Option<&str>,
    ) -> anyhow::Result<MatrixAuth> {
        let mut body = json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": username
            },
            "password": password,
        });
        if let Some(name) = device_name {
            body["initial_device_display_name"] = json!(name);
        }
        let response = self
            .http
            .send_json(Method::POST, "/_matrix/client/v3/login", None, Some(&body))
            .await?;

        let access_token = response
            .get("access_token")
            .and_then(Value::as_str)
            .context("missing access_token in login response")?
            .to_owned();
        let user_id = response
            .get("user_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let device_id = response
            .get("device_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        Ok(MatrixAuth {
            access_token,
            user_id,
            device_id,
        })
    }

    pub async fn password_register(
        &self,
        localpart: &str,
        password: &str,
        device_name: Option<&str>,
    ) -> anyhow::Result<MatrixAuth> {
        let mut body = json!({
            "username": localpart,
            "password": password,
        });
        if let Some(name) = device_name {
            body["initial_device_display_name"] = json!(name);
        }

        let response = self
            .http
            .send_json(
                Method::POST,
                "/_matrix/client/v3/register",
                None,
                Some(&body),
            )
            .await?;

        let access_token = response
            .get("access_token")
            .and_then(Value::as_str)
            .context("missing access_token in register response")?
            .to_owned();
        let user_id = response
            .get("user_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let device_id = response
            .get("device_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        Ok(MatrixAuth {
            access_token,
            user_id,
            device_id,
        })
    }

    pub async fn login_with_token(&self, auth: MatrixAuth) {
        *self.auth.write().await = auth;
    }
}

impl MatrixClient {
    pub async fn sync_once(
        &self,
        since: Option<&str>,
    ) -> anyhow::Result<Vec<crate::models::events::RoomEvent>> {
        let (events, _) = self.sync_once_internal(since).await?;
        Ok(events)
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.stop_syncing.store(false, Ordering::Relaxed);
        let mut since: Option<String> = None;
        while !self.stop_syncing.load(Ordering::Relaxed) {
            let (_, next_batch) = self.sync_once_internal(since.as_deref()).await?;
            if let Some(next_batch) = next_batch {
                since = Some(next_batch);
            }
        }
        Ok(())
    }

    pub fn stop(&self) {
        self.stop_syncing.store(true, Ordering::Relaxed);
    }

    async fn sync_once_internal(
        &self,
        since: Option<&str>,
    ) -> anyhow::Result<(Vec<crate::models::events::RoomEvent>, Option<String>)> {
        let endpoint = match since {
            Some(since_token) => format!("/_matrix/client/v3/sync?since={since_token}"),
            None => "/_matrix/client/v3/sync".to_owned(),
        };

        let payload = self
            .http
            .send_json(Method::GET, &endpoint, Some(&self.token().await), None)
            .await?;

        let preprocessors = self.preprocessors.read().await.clone();
        let filters = self.filters.read().await.clone();

        let mut events = Vec::new();
        let Some(joined_rooms) = payload
            .get("rooms")
            .and_then(|v| v.get("join"))
            .and_then(Value::as_object)
        else {
            let next_batch = payload
                .get("next_batch")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            return Ok((events, next_batch));
        };

        for (room_id, room) in joined_rooms {
            let Some(timeline) = room
                .get("timeline")
                .and_then(|t| t.get("events"))
                .and_then(Value::as_array)
            else {
                continue;
            };

            for raw in timeline {
                let mut mutable = raw.clone();
                for preprocessor in &preprocessors {
                    preprocessor.process(&mut mutable).await?;
                }
                let room_event = converter::parse_room_event(&mutable, room_id.clone())?;
                if filters
                    .iter()
                    .all(|filter| filter.matches(&room_event.event))
                {
                    events.push(room_event);
                }
            }
        }

        let next_batch = payload
            .get("next_batch")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        Ok((events, next_batch))
    }
}

impl MatrixClient {
    pub async fn get_user_id(&self) -> anyhow::Result<String> {
        let account = self.whoami().await?;
        Ok(account.user_id)
    }

    pub async fn profile(&self, user_id: &str) -> anyhow::Result<MatrixProfile> {
        let user_id_encoded = encode_path_component(user_id);
        let response = self
            .http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/profile/{user_id_encoded}"),
                Some(&self.token().await),
                None,
            )
            .await?;

        Ok(MatrixProfile {
            user_id: user_id.to_owned(),
            displayname: response
                .get("displayname")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            avatar_url: response
                .get("avatar_url")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        })
    }

    pub async fn set_display_name(&self, display_name: &str) -> anyhow::Result<()> {
        let user_id = self.get_user_id().await?;
        let user_id_encoded = encode_path_component(&user_id);
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/profile/{user_id_encoded}/displayname"),
                Some(&self.token().await),
                Some(&json!({ "displayname": display_name })),
            )
            .await?;
        Ok(())
    }

    pub async fn set_avatar_url(&self, avatar_url: &str) -> anyhow::Result<()> {
        let user_id = self.get_user_id().await?;
        let user_id_encoded = encode_path_component(&user_id);
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/profile/{user_id_encoded}/avatar_url"),
                Some(&self.token().await),
                Some(&json!({ "avatar_url": avatar_url })),
            )
            .await?;
        Ok(())
    }

    pub async fn get_presence_status(&self, user_id: &str) -> anyhow::Result<Presence> {
        let user_id_encoded = encode_path_component(user_id);
        let response = self
            .http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/presence/{user_id_encoded}/status"),
                Some(&self.token().await),
                None,
            )
            .await?;

        let presence_str = response
            .get("presence")
            .and_then(Value::as_str)
            .unwrap_or("offline");
        match presence_str {
            "online" => Ok(Presence::Online),
            "unavailable" => Ok(Presence::Unavailable),
            _ => Ok(Presence::Offline),
        }
    }

    pub async fn get_presence_status_for(&self, user_id: &str) -> anyhow::Result<Presence> {
        self.get_presence_status(user_id).await
    }

    pub async fn set_presence_status(
        &self,
        status: Presence,
        status_msg: Option<&str>,
    ) -> anyhow::Result<()> {
        let user_id = self.get_user_id().await?;
        let user_id_encoded = encode_path_component(&user_id);
        let mut body = json!({ "presence": match status {
            Presence::Online => "online",
            Presence::Unavailable => "unavailable",
            Presence::Offline => "offline",
        }});
        if let Some(msg) = status_msg {
            body["status_msg"] = json!(msg);
        }
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/presence/{user_id_encoded}/status"),
                Some(&self.token().await),
                Some(&body),
            )
            .await?;
        Ok(())
    }

    pub async fn whoami(&self) -> anyhow::Result<Account> {
        let response = self
            .http
            .send_json(
                Method::GET,
                "/_matrix/client/v3/account/whoami",
                Some(&self.token().await),
                None,
            )
            .await?;

        Ok(Account {
            user_id: response
                .get("user_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            device_id: response
                .get("device_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            is_guest: response
                .get("is_guest")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub async fn get_who_am_i(&self) -> anyhow::Result<Account> {
        self.whoami().await
    }

    pub async fn get_user_profile(&self, user_id: &str) -> anyhow::Result<MatrixProfile> {
        self.profile(user_id).await
    }
}

impl MatrixClient {
    pub async fn get_joined_rooms(&self) -> anyhow::Result<Vec<String>> {
        let response = self
            .http
            .send_json(
                Method::GET,
                "/_matrix/client/v3/joined_rooms",
                Some(&self.token().await),
                None,
            )
            .await?;

        let rooms = response
            .get("joined_rooms")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        Ok(rooms)
    }

    pub async fn get_joined_room_members(&self, room_id: &str) -> anyhow::Result<Vec<String>> {
        let room_id = encode_path_component(room_id);
        let response = self
            .http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/joined_members"),
                Some(&self.token().await),
                None,
            )
            .await?;

        let members = response
            .get("joined")
            .and_then(Value::as_object)
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        Ok(members)
    }

    pub async fn get_joined_room_members_with_profiles(
        &self,
        room_id: &str,
    ) -> anyhow::Result<BTreeMap<String, JoinedMemberProfile>> {
        let room_id = encode_path_component(room_id);
        let response = self
            .http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/joined_members"),
                Some(&self.token().await),
                None,
            )
            .await?;

        let profiles = response
            .get("joined")
            .and_then(Value::as_object)
            .map(|obj| {
                obj.iter()
                    .map(|(user_id, profile)| {
                        let profile = JoinedMemberProfile {
                            display_name: profile
                                .get("display_name")
                                .and_then(Value::as_str)
                                .map(ToOwned::to_owned),
                            avatar_url: profile
                                .get("avatar_url")
                                .and_then(Value::as_str)
                                .map(ToOwned::to_owned),
                        };
                        (user_id.clone(), profile)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(profiles)
    }

    pub async fn get_room_members(
        &self,
        room_id: &str,
        membership: Option<Membership>,
        not_membership: Option<Membership>,
    ) -> anyhow::Result<Vec<MembershipEvent>> {
        self.get_room_members_at(room_id, membership, not_membership, None)
            .await
    }

    pub async fn get_all_room_members(
        &self,
        room_id: &str,
        at_token: Option<&str>,
    ) -> anyhow::Result<Vec<MembershipEvent>> {
        self.get_room_members_at(room_id, None, None, at_token)
            .await
    }

    pub async fn get_room_members_by_membership(
        &self,
        room_id: &str,
        membership: Membership,
        at_token: Option<&str>,
    ) -> anyhow::Result<Vec<MembershipEvent>> {
        self.get_room_members_at(room_id, Some(membership), None, at_token)
            .await
    }

    pub async fn get_room_members_without_membership(
        &self,
        room_id: &str,
        not_membership: Membership,
        at_token: Option<&str>,
    ) -> anyhow::Result<Vec<MembershipEvent>> {
        self.get_room_members_at(room_id, None, Some(not_membership), at_token)
            .await
    }

    pub async fn get_room_members_at(
        &self,
        room_id: &str,
        membership: Option<Membership>,
        not_membership: Option<Membership>,
        at_token: Option<&str>,
    ) -> anyhow::Result<Vec<MembershipEvent>> {
        let room_id_enc = encode_path_component(room_id);
        let mut query = Vec::new();
        if let Some(m) = membership {
            query.push(format!("membership={}", membership_to_query_value(m)));
        }
        if let Some(m) = not_membership {
            query.push(format!("not_membership={}", membership_to_query_value(m)));
        }
        if let Some(token) = at_token {
            query.push(format!("at={}", encode_path_component(token)));
        }
        let endpoint = if query.is_empty() {
            format!("/_matrix/client/v3/rooms/{room_id_enc}/members")
        } else {
            format!(
                "/_matrix/client/v3/rooms/{room_id_enc}/members?{}",
                query.join("&")
            )
        };

        let response = self
            .http
            .send_json(Method::GET, &endpoint, Some(&self.token().await), None)
            .await?;

        let members = response
            .get("chunk")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        let user_id = v.get("state_key").and_then(Value::as_str).or_else(|| {
                            v.get("content")
                                .and_then(|c| c.get("user_id"))
                                .and_then(Value::as_str)
                        })?;
                        let membership_str = v
                            .get("content")?
                            .get("membership")?
                            .as_str()
                            .unwrap_or("leave");
                        let membership = match membership_str {
                            "invite" => Membership::Invite,
                            "join" => Membership::Join,
                            "knock" => Membership::Knock,
                            "ban" => Membership::Ban,
                            _ => Membership::Leave,
                        };
                        Some(MembershipEvent {
                            user_id: user_id.to_owned(),
                            membership,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(members)
    }

    pub async fn get_room_state(&self, room_id: &str) -> anyhow::Result<Vec<Value>> {
        let room_id = encode_path_component(room_id);
        let response = self
            .http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/state"),
                Some(&self.token().await),
                None,
            )
            .await?;

        response
            .as_array()
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    pub async fn get_room_state_events(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> anyhow::Result<Value> {
        self.get_room_state_event_content(room_id, event_type, state_key)
            .await
    }

    pub async fn get_room_create_event(&self, room_id: &str) -> anyhow::Result<Value> {
        self.get_room_state_event_body(room_id, "m.room.create", "")
            .await
    }

    pub async fn user_has_power_level_for(
        &self,
        user_id: &str,
        room_id: &str,
        event_type: &str,
        is_state: bool,
    ) -> anyhow::Result<bool> {
        let power_levels_value = self
            .get_room_state_event_content(room_id, "m.room.power_levels", "")
            .await?;
        let power_levels: crate::models::PowerLevels = serde_json::from_value(power_levels_value)
            .unwrap_or_else(|_| crate::models::PowerLevels::default());
        let manager = crate::models::PLManager::new(power_levels);

        let mut required_power = if is_state {
            manager.power_levels.state_default
        } else {
            manager.power_levels.events_default
        };
        if let Some(required_event_level) = manager.power_levels.events.get(event_type) {
            required_power = *required_event_level;
        }

        Ok(manager.user_level(user_id) >= required_power)
    }

    pub async fn user_has_power_level_for_action(
        &self,
        user_id: &str,
        room_id: &str,
        action: PowerLevelAction,
    ) -> anyhow::Result<bool> {
        let power_levels_value = self
            .get_room_state_event_content(room_id, "m.room.power_levels", "")
            .await?;
        let power_levels: crate::models::PowerLevels = serde_json::from_value(power_levels_value)
            .unwrap_or_else(|_| crate::models::PowerLevels::default());
        let manager = crate::models::PLManager::new(power_levels);
        let required_power = match action {
            PowerLevelAction::Ban => manager.power_levels.ban,
            PowerLevelAction::Invite => manager.power_levels.invite,
            PowerLevelAction::Kick => manager.power_levels.kick,
            PowerLevelAction::RedactEvents => manager.power_levels.redact,
            PowerLevelAction::NotifyRoom => manager.power_levels.notify_whole_room_level(),
        };
        Ok(manager.user_level(user_id) >= required_power)
    }

    pub async fn calculate_power_level_change_bounds_on(
        &self,
        target_user_id: &str,
        room_id: &str,
    ) -> anyhow::Result<PowerLevelBounds> {
        let my_user_id = self.get_user_id().await?;
        let can_change = self
            .user_has_power_level_for(&my_user_id, room_id, "m.room.power_levels", true)
            .await?;
        if !can_change {
            return Ok(PowerLevelBounds {
                can_modify: false,
                maximum_possible_level: 0,
            });
        }

        let power_levels_value = self
            .get_room_state_event_content(room_id, "m.room.power_levels", "")
            .await?;
        let power_levels: crate::models::PowerLevels = serde_json::from_value(power_levels_value)
            .unwrap_or_else(|_| crate::models::PowerLevels::default());
        let manager = crate::models::PLManager::new(power_levels);

        let target_user_power = manager.user_level(target_user_id);
        let my_user_power = manager.user_level(&my_user_id);

        if my_user_id == target_user_id {
            return Ok(PowerLevelBounds {
                can_modify: true,
                maximum_possible_level: my_user_power,
            });
        }
        if target_user_power >= my_user_power {
            return Ok(PowerLevelBounds {
                can_modify: false,
                maximum_possible_level: my_user_power,
            });
        }
        Ok(PowerLevelBounds {
            can_modify: true,
            maximum_possible_level: my_user_power,
        })
    }

    pub async fn set_user_power_level(
        &self,
        user_id: &str,
        room_id: &str,
        new_level: i64,
    ) -> anyhow::Result<()> {
        let mut current_levels = self
            .get_room_state_event_content(room_id, "m.room.power_levels", "")
            .await
            .unwrap_or_else(|_| json!({}));
        if !current_levels.is_object() {
            current_levels = json!({});
        }
        let Some(obj) = current_levels.as_object_mut() else {
            anyhow::bail!("power levels state is not a JSON object");
        };
        let users = obj
            .entry("users")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let Some(users_obj) = users.as_object_mut() else {
            anyhow::bail!("power levels users is not a JSON object");
        };
        users_obj.insert(user_id.to_owned(), Value::from(new_level));

        self.send_state_event(room_id, "m.room.power_levels", "", &current_levels)
            .await?;
        Ok(())
    }

    pub async fn get_room_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> anyhow::Result<Value> {
        let room_id = encode_path_component(room_id);
        let event_type = encode_path_component(event_type);
        let state_key = encode_path_component(state_key);
        self.http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}"),
                Some(&self.token().await),
                None,
            )
            .await
    }

    pub async fn get_room_state_event_content(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> anyhow::Result<Value> {
        self.get_room_state_event(room_id, event_type, state_key)
            .await
    }

    pub async fn get_room_state_event_body(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> anyhow::Result<Value> {
        if self
            .does_server_support_version("v1.16")
            .await
            .unwrap_or(false)
        {
            let mut query = BTreeMap::new();
            query.insert("format".to_owned(), "event".to_owned());
            let room_id = encode_path_component(room_id);
            let event_type = encode_path_component(event_type);
            let state_key = encode_path_component(state_key);
            return self
                .do_request(
                    Method::GET,
                    &format!("/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}"),
                    Some(&query),
                    None,
                )
                .await;
        }

        let state = self.get_room_state(room_id).await?;
        state
            .into_iter()
            .find(|event| {
                event
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind == event_type)
                    && event
                        .get("state_key")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        == state_key
            })
            .ok_or_else(|| anyhow::anyhow!("state event not found"))
    }

    pub async fn send_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        content: &Value,
    ) -> anyhow::Result<String> {
        let room_id = encode_path_component(room_id);
        let event_type = encode_path_component(event_type);
        let state_key = encode_path_component(state_key);
        let response = self
            .http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}"),
                Some(&self.token().await),
                Some(content),
            )
            .await?;
        response
            .get("event_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("missing event_id in response")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoomDirectoryLookupResponse {
    pub room_id: String,
    pub servers: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JoinedMemberProfile {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadedContent {
    pub data: Vec<u8>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerLevelAction {
    Ban,
    Invite,
    Kick,
    RedactEvents,
    NotifyRoom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PowerLevelBounds {
    pub can_modify: bool,
    pub maximum_possible_level: i64,
}

impl MatrixClient {
    pub async fn join_room(&self, room_id_or_alias: &str) -> anyhow::Result<String> {
        let strategy = self.join_strategy.read().await.clone();
        if let Some(strat) = strategy {
            strat.join_room(self, room_id_or_alias).await
        } else {
            let encoded = encode_path_component(room_id_or_alias);
            let response = self
                .http
                .send_json(
                    Method::POST,
                    &format!("/_matrix/client/v3/join/{encoded}"),
                    Some(&self.token().await),
                    Some(&json!({})),
                )
                .await?;
            response
                .get("room_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .context("missing room_id in join response")
        }
    }

    pub async fn create_space(&self, options: &CreateRoom) -> anyhow::Result<String> {
        let mut opt = options.clone();
        opt.creation_content = Some(json!({
            "type": "m.space"
        }));

        // spaces generally don't have direct flag or same defaults
        let space_id = self.create_room(&opt).await?;
        Ok(space_id)
    }

    pub async fn get_space(&self, room_id: &str) -> anyhow::Result<crate::models::Space> {
        let mut space = crate::models::Space::new(room_id);

        let state = self.get_room_state(room_id).await?;
        for ev in state {
            if let Some(obj) = ev.as_object()
                && let (Some(ev_type), Some(state_key), Some(content)) = (
                    obj.get("type").and_then(Value::as_str),
                    obj.get("state_key").and_then(Value::as_str),
                    obj.get("content").and_then(Value::as_object),
                )
            {
                if ev_type == "m.space.child" {
                    if content.is_empty() || content.get("via").is_none() {
                        space.remove_child(state_key);
                    } else {
                        let suggested = content
                            .get("suggested")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        space.add_child(state_key, suggested);
                    }
                } else if ev_type == "m.space.parent"
                    && !content.is_empty()
                    && content.get("via").is_some()
                    && !space.parents.contains(&state_key.to_string())
                {
                    space.parents.push(state_key.to_string());
                }
            }
        }

        Ok(space)
    }

    pub async fn leave_room(&self, room_id: &str, reason: Option<&str>) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        let body = reason.map(|r| json!({ "reason": r }));
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/leave"),
                Some(&self.token().await),
                body.as_ref(),
            )
            .await?;
        Ok(())
    }

    pub async fn forget_room(&self, room_id: &str) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/forget"),
                Some(&self.token().await),
                None,
            )
            .await?;
        Ok(())
    }

    pub async fn invite_user(&self, user_id: &str, room_id: &str) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/invite"),
                Some(&self.token().await),
                Some(&json!({ "user_id": user_id })),
            )
            .await?;
        Ok(())
    }

    pub async fn kick_user(
        &self,
        user_id: &str,
        room_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        let mut body = json!({ "user_id": user_id });
        if let Some(r) = reason {
            body["reason"] = json!(r);
        }
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/kick"),
                Some(&self.token().await),
                Some(&body),
            )
            .await?;
        Ok(())
    }

    pub async fn ban_user(
        &self,
        user_id: &str,
        room_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        let mut body = json!({ "user_id": user_id });
        if let Some(r) = reason {
            body["reason"] = json!(r);
        }
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/ban"),
                Some(&self.token().await),
                Some(&body),
            )
            .await?;
        Ok(())
    }

    pub async fn unban_user(&self, user_id: &str, room_id: &str) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/unban"),
                Some(&self.token().await),
                Some(&json!({ "user_id": user_id })),
            )
            .await?;
        Ok(())
    }

    pub async fn upgrade_room(&self, room_id: &str, new_version: &str) -> anyhow::Result<String> {
        let room_id = encode_path_component(room_id);
        let response = self
            .http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/upgrade"),
                Some(&self.token().await),
                Some(&json!({ "new_version": new_version })),
            )
            .await?;

        response
            .get("replacement_room")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("missing replacement_room in upgrade response")
    }

    pub async fn get_room_upgrade_history(&self, room_id: &str) -> anyhow::Result<Vec<Value>> {
        let mut history = Vec::new();
        let mut current_room = room_id.to_string();

        loop {
            let create_event = match self.get_room_create_event(&current_room).await {
                Ok(ev) => ev,
                Err(_) => break, // Can't go back further
            };

            history.push(create_event.clone());

            if let Some(predecessor) = create_event.get("predecessor").and_then(Value::as_object)
                && let Some(prev_room) = predecessor.get("room_id").and_then(Value::as_str)
            {
                current_room = prev_room.to_string();
                continue;
            }
            break;
        }

        Ok(history)
    }
}

impl MatrixClient {
    pub async fn send_message(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        self.send_text(room_id, body).await
    }

    pub async fn send_text(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        let content = json!({
            "msgtype": "m.text",
            "body": body
        });
        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn send_html_text(
        &self,
        room_id: &str,
        body: &str,
        html: &str,
    ) -> anyhow::Result<String> {
        let content = json!({
            "msgtype": "m.text",
            "body": body,
            "format": "org.matrix.custom.html",
            "formatted_body": html
        });
        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn send_notice(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        let content = json!({
            "msgtype": "m.notice",
            "body": body
        });
        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn send_html_notice(
        &self,
        room_id: &str,
        body: &str,
        html: &str,
    ) -> anyhow::Result<String> {
        let content = json!({
            "msgtype": "m.notice",
            "body": body,
            "format": "org.matrix.custom.html",
            "formatted_body": html
        });
        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn reply_text(
        &self,
        room_id: &str,
        event: &Event,
        text: &str,
    ) -> anyhow::Result<String> {
        let mut content = json!({
            "msgtype": "m.text",
            "body": text,
        });
        // Simplistic reply fallback logic. Real bots usually format full fallback HTML,
        // but adding simple m.in_reply_to block for basic reply relationship.
        let m_relates_to = json!({
            "m.in_reply_to": {
                "event_id": event.event_id
            }
        });
        content
            .as_object_mut()
            .unwrap()
            .insert("m.relates_to".to_string(), m_relates_to);

        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn reply_html_text(
        &self,
        room_id: &str,
        event: &Event,
        html: &str,
    ) -> anyhow::Result<String> {
        let body = plain_text_from_html(html);
        let mut content = json!({
            "msgtype": "m.text",
            "body": body,
            "format": "org.matrix.custom.html",
            "formatted_body": html,
        });
        let m_relates_to = json!({
            "m.in_reply_to": {
                "event_id": event.event_id
            }
        });
        content
            .as_object_mut()
            .unwrap()
            .insert("m.relates_to".to_string(), m_relates_to);

        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn reply_notice(
        &self,
        room_id: &str,
        event: &Event,
        text: &str,
    ) -> anyhow::Result<String> {
        let mut content = json!({
            "msgtype": "m.notice",
            "body": text,
        });
        let m_relates_to = json!({
            "m.in_reply_to": {
                "event_id": event.event_id
            }
        });
        content
            .as_object_mut()
            .unwrap()
            .insert("m.relates_to".to_string(), m_relates_to);

        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn reply_html_notice(
        &self,
        room_id: &str,
        event: &Event,
        html: &str,
    ) -> anyhow::Result<String> {
        let body = plain_text_from_html(html);
        let mut content = json!({
            "msgtype": "m.notice",
            "body": body,
            "format": "org.matrix.custom.html",
            "formatted_body": html,
        });
        let m_relates_to = json!({
            "m.in_reply_to": {
                "event_id": event.event_id
            }
        });
        content
            .as_object_mut()
            .unwrap()
            .insert("m.relates_to".to_string(), m_relates_to);

        self.send_event(room_id, "m.room.message", &content).await
    }

    pub async fn send_event(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
    ) -> anyhow::Result<String> {
        let room_id = encode_path_component(room_id);
        let event_type = encode_path_component(event_type);
        let txn_id = Uuid::new_v4().to_string();
        let response = self
            .http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}"),
                Some(&self.token().await),
                Some(content),
            )
            .await?;
        response
            .get("event_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("missing event_id in send response")
    }

    pub async fn send_raw_event(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
        txn_id: Option<&str>,
    ) -> anyhow::Result<String> {
        let room_id = encode_path_component(room_id);
        let event_type = encode_path_component(event_type);
        let txn_id = txn_id
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let response = self
            .http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}"),
                Some(&self.token().await),
                Some(content),
            )
            .await?;
        response
            .get("event_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("missing event_id in send response")
    }

    pub async fn redact_event(
        &self,
        room_id: &str,
        event_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<String> {
        let room_id = encode_path_component(room_id);
        let event_id = encode_path_component(event_id);
        let txn_id = Uuid::new_v4().to_string();
        let body = reason.map(|r| json!({ "reason": r }));
        let response = self
            .http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}"),
                Some(&self.token().await),
                body.as_ref(),
            )
            .await?;
        response
            .get("event_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("missing event_id in redact response")
    }

    pub async fn send_read_receipt(&self, room_id: &str, event_id: &str) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        let event_id = encode_path_component(event_id);
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/receipt/m.read/{event_id}"),
                Some(&self.token().await),
                None,
            )
            .await?;
        Ok(())
    }

    pub async fn set_typing(
        &self,
        room_id: &str,
        user_id: &str,
        typing: bool,
        timeout: Option<u64>,
    ) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        let user_id = encode_path_component(user_id);
        let mut body = json!({ "typing": typing });
        if let Some(t) = timeout {
            body["timeout"] = json!(t);
        }
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/typing/{user_id}"),
                Some(&self.token().await),
                Some(&body),
            )
            .await?;
        Ok(())
    }

    pub async fn get_event(&self, room_id: &str, event_id: &str) -> anyhow::Result<Value> {
        let room_id = encode_path_component(room_id);
        let event_id = encode_path_component(event_id);
        self.http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/event/{event_id}"),
                Some(&self.token().await),
                None,
            )
            .await
    }
}

impl MatrixClient {
    pub async fn create_room(&self, options: &CreateRoom) -> anyhow::Result<String> {
        let payload = serde_json::to_value(options)?;
        let response = self
            .http
            .send_json(
                Method::POST,
                "/_matrix/client/v3/createRoom",
                Some(&self.token().await),
                Some(&payload),
            )
            .await?;
        response
            .get("room_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("missing room_id in createRoom response")
    }

    pub async fn create_room_alias(&self, alias: &str, room_id: &str) -> anyhow::Result<()> {
        let alias = encode_path_component(alias);
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/directory/room/{alias}"),
                Some(&self.token().await),
                Some(&json!({ "room_id": room_id })),
            )
            .await?;
        Ok(())
    }

    pub async fn delete_room_alias(&self, alias: &str) -> anyhow::Result<()> {
        let alias = encode_path_component(alias);
        self.http
            .send_json(
                Method::DELETE,
                &format!("/_matrix/client/v3/directory/room/{alias}"),
                Some(&self.token().await),
                None,
            )
            .await?;
        Ok(())
    }

    pub async fn lookup_room_alias(
        &self,
        alias: &str,
    ) -> anyhow::Result<RoomDirectoryLookupResponse> {
        let alias = encode_path_component(alias);
        let response = self
            .http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/directory/room/{alias}"),
                Some(&self.token().await),
                None,
            )
            .await?;
        serde_json::from_value(response).context("invalid room directory lookup response")
    }

    pub async fn set_directory_visibility(
        &self,
        room_id: &str,
        visibility: &str,
    ) -> anyhow::Result<()> {
        let room_id = encode_path_component(room_id);
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/directory/list/room/{room_id}"),
                Some(&self.token().await),
                Some(&json!({ "visibility": visibility })),
            )
            .await?;
        Ok(())
    }

    pub async fn get_directory_visibility(&self, room_id: &str) -> anyhow::Result<String> {
        let room_id = encode_path_component(room_id);
        let response = self
            .http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/directory/list/room/{room_id}"),
                Some(&self.token().await),
                None,
            )
            .await?;
        response
            .get("visibility")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("missing visibility in response")
    }

    pub async fn resolve_room(&self, room_id_or_alias: &str) -> anyhow::Result<String> {
        if room_id_or_alias.starts_with('!') {
            Ok(room_id_or_alias.to_owned())
        } else if room_id_or_alias.starts_with('#') {
            let res = self.lookup_room_alias(room_id_or_alias).await?;
            Ok(res.room_id)
        } else {
            anyhow::bail!("Invalid room ID or alias: {}", room_id_or_alias)
        }
    }

    pub async fn get_published_alias(
        &self,
        room_id_or_alias: &str,
    ) -> anyhow::Result<Option<String>> {
        let room_id = match self.resolve_room(room_id_or_alias).await {
            Ok(room_id) => room_id,
            Err(_) => return Ok(None),
        };
        let event = match self
            .get_room_state_event_content(&room_id, "m.room.canonical_alias", "")
            .await
        {
            Ok(event) => event,
            Err(_) => return Ok(None),
        };
        let canonical = event
            .get("alias")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        if canonical.is_some() {
            return Ok(canonical);
        }
        Ok(event
            .get("alt_aliases")
            .and_then(Value::as_array)
            .and_then(|aliases| {
                aliases
                    .iter()
                    .find_map(|a| a.as_str().map(ToOwned::to_owned))
            }))
    }
}

impl MatrixClient {
    pub async fn set_account_data(&self, event_type: &str, content: &Value) -> anyhow::Result<()> {
        let user_id = self.get_user_id().await?;
        let user_id = encode_path_component(&user_id);
        let event_type = encode_path_component(event_type);
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/user/{user_id}/account_data/{event_type}"),
                Some(&self.token().await),
                Some(content),
            )
            .await?;
        Ok(())
    }

    pub async fn get_account_data(&self, event_type: &str) -> anyhow::Result<Value> {
        let user_id = self.get_user_id().await?;
        let user_id = encode_path_component(&user_id);
        let event_type = encode_path_component(event_type);
        self.http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/user/{user_id}/account_data/{event_type}"),
                Some(&self.token().await),
                None,
            )
            .await
    }

    pub async fn set_room_account_data(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
    ) -> anyhow::Result<()> {
        let user_id = self.get_user_id().await?;
        let user_id = encode_path_component(&user_id);
        let room_id = encode_path_component(room_id);
        let event_type = encode_path_component(event_type);
        self.http
            .send_json(
                Method::PUT,
                &format!(
                    "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{event_type}"
                ),
                Some(&self.token().await),
                Some(content),
            )
            .await?;
        Ok(())
    }

    pub async fn get_room_account_data(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> anyhow::Result<Value> {
        let user_id = self.get_user_id().await?;
        let user_id = encode_path_component(&user_id);
        let room_id = encode_path_component(room_id);
        let event_type = encode_path_component(event_type);
        self.http
            .send_json(
                Method::GET,
                &format!(
                    "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{event_type}"
                ),
                Some(&self.token().await),
                None,
            )
            .await
    }

    pub async fn get_openid_token(&self) -> anyhow::Result<Value> {
        let user_id = self.get_user_id().await?;
        let user_id = encode_path_component(&user_id);
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/user/{user_id}/openid/request_token"),
                Some(&self.token().await),
                Some(&json!({})),
            )
            .await
    }

    pub async fn get_openid_connect_token(&self) -> anyhow::Result<Value> {
        self.get_openid_token().await
    }
}

impl MatrixClient {
    pub async fn get_own_devices(&self) -> anyhow::Result<Vec<Value>> {
        let response = self
            .http
            .send_json(
                Method::GET,
                "/_matrix/client/v3/devices",
                Some(&self.token().await),
                None,
            )
            .await?;
        response
            .get("devices")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing devices in response"))
    }

    pub async fn get_user_devices(&self, user_ids: &[&str]) -> anyhow::Result<Value> {
        let user_ids_str = user_ids.join(",");
        self.http
            .send_json(
                Method::POST,
                &format!("/_matrix/client/v3/keys/query?user_ids={}", user_ids_str),
                Some(&self.token().await),
                Some(&json!({})),
            )
            .await
    }

    pub async fn upload_device_one_time_keys(&self, keys: &Value) -> anyhow::Result<Value> {
        self.http
            .send_json(
                Method::POST,
                "/_matrix/client/v3/keys/upload",
                Some(&self.token().await),
                Some(&json!({ "one_time_keys": keys })),
            )
            .await
    }

    pub async fn claim_one_time_keys(
        &self,
        devices: &Value,
        key_algorithm: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut req_body = json!({
            "one_time_keys": devices
        });
        if let Some(algo) = key_algorithm {
            req_body["key_algorithm"] = json!(algo);
        }
        self.http
            .send_json(
                Method::POST,
                "/_matrix/client/v3/keys/claim",
                Some(&self.token().await),
                Some(&req_body),
            )
            .await
    }

    pub async fn check_one_time_key_counts(&self) -> anyhow::Result<Value> {
        let response = self
            .http
            .send_json(
                Method::GET,
                "/_matrix/client/v3/sync?timeout=0",
                Some(&self.token().await),
                None,
            )
            .await?;

        response
            .get("device_one_time_keys_count")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing device_one_time_keys_count in sync response"))
    }

    pub async fn upload_fallback_key(&self, key: &Value) -> anyhow::Result<Value> {
        self.http
            .send_json(
                Method::POST,
                "/_matrix/client/v3/keys/upload",
                Some(&self.token().await),
                Some(&json!({ "fallback_keys": key })),
            )
            .await
    }

    pub async fn send_to_devices(&self, event_type: &str, messages: &Value) -> anyhow::Result<()> {
        let txn_id = Uuid::new_v4().to_string();
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/sendToDevice/{event_type}/{txn_id}"),
                Some(&self.token().await),
                Some(messages),
            )
            .await?;
        Ok(())
    }

    pub async fn get_key_backup_version(&self) -> anyhow::Result<Option<Value>> {
        let response = self
            .http
            .send_json(
                Method::GET,
                "/_matrix/client/v3/room_keys/version",
                Some(&self.token().await),
                None,
            )
            .await?;
        if response
            .get("errcode")
            .and_then(Value::as_str)
            .is_some_and(|code| code == "M_NOT_FOUND")
        {
            return Ok(None);
        }
        Ok(Some(response))
    }

    pub async fn sign_and_create_key_backup_version(&self, info: &Value) -> anyhow::Result<Value> {
        self.http
            .send_json(
                Method::POST,
                "/_matrix/client/v3/room_keys/version",
                Some(&self.token().await),
                Some(info),
            )
            .await
    }

    pub async fn update_key_backup_version(
        &self,
        version: &str,
        info: &Value,
    ) -> anyhow::Result<()> {
        let version = encode_path_component(version);
        self.http
            .send_json(
                Method::PUT,
                &format!("/_matrix/client/v3/room_keys/version/{version}"),
                Some(&self.token().await),
                Some(info),
            )
            .await?;
        Ok(())
    }

    pub async fn enable_key_backup(&self, _info: &Value) -> anyhow::Result<()> {
        anyhow::bail!("key backup enable requires full crypto integration")
    }

    pub async fn disable_key_backup(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn export_room_keys_for_session(
        &self,
        _room_id: &str,
        _session_id: &str,
    ) -> anyhow::Result<Vec<Value>> {
        anyhow::bail!("room key export requires full crypto integration")
    }
}

impl MatrixClient {
    pub async fn get_server_versions(&self) -> anyhow::Result<Value> {
        self.http
            .send_json(Method::GET, "/_matrix/client/versions", None, None)
            .await
    }

    pub async fn get_capabilities(&self) -> anyhow::Result<Value> {
        self.http
            .send_json(
                Method::GET,
                "/_matrix/client/v3/capabilities",
                Some(&self.token().await),
                None,
            )
            .await
    }

    pub async fn does_server_support_version(&self, version: &str) -> anyhow::Result<bool> {
        let versions = self.get_server_versions().await?;
        let supported = versions
            .get("versions")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).any(|v| v == version))
            .unwrap_or(false);
        Ok(supported)
    }

    pub async fn does_server_support_any_one_version(
        &self,
        versions: &[&str],
    ) -> anyhow::Result<bool> {
        for version in versions {
            if self.does_server_support_version(version).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn does_server_support_unstable_feature(
        &self,
        feature: &str,
    ) -> anyhow::Result<bool> {
        let versions = self.get_server_versions().await?;
        let supported = versions
            .get("unstable_features")
            .and_then(|m| m.get(feature))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(supported)
    }

    pub async fn get_raw_event(&self, room_id: &str, event_id: &str) -> anyhow::Result<Value> {
        let room_id = encode_path_component(room_id);
        let event_id = encode_path_component(event_id);
        self.http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/event/{event_id}"),
                Some(&self.token().await),
                None,
            )
            .await
    }

    pub async fn get_event_context(&self, room_id: &str, event_id: &str) -> anyhow::Result<Value> {
        let room_id = encode_path_component(room_id);
        let event_id = encode_path_component(event_id);
        self.http
            .send_json(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/context/{event_id}"),
                Some(&self.token().await),
                None,
            )
            .await
    }

    pub async fn get_event_nearest_to_timestamp(
        &self,
        room_id: &str,
        ts: u64,
        dir: &str,
    ) -> anyhow::Result<Value> {
        let room_id = encode_path_component(room_id);
        let endpoint =
            format!("/_matrix/client/v1/rooms/{room_id}/timestamp_to_event?ts={ts}&dir={dir}");
        self.http
            .send_json(Method::GET, &endpoint, Some(&self.token().await), None)
            .await
    }

    pub async fn get_relations_for_event(
        &self,
        room_id: &str,
        event_id: &str,
        rel_type: Option<&str>,
        event_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        let room_id_enc = encode_path_component(room_id);
        let event_id_enc = encode_path_component(event_id);

        // Build base endpoint
        let mut endpoint =
            format!("/_matrix/client/v3/rooms/{room_id_enc}/relations/{event_id_enc}");
        if let Some(rt) = rel_type {
            endpoint.push('/');
            endpoint.push_str(&encode_path_component(rt));
            if let Some(et) = event_type {
                endpoint.push('/');
                endpoint.push_str(&encode_path_component(et));
            }
        }

        self.http
            .send_json(Method::GET, &endpoint, Some(&self.token().await), None)
            .await
    }
}

impl MatrixClient {
    pub async fn upload_media(
        &self,
        bytes: Vec<u8>,
        content_type: &str,
        filename: Option<&str>,
    ) -> anyhow::Result<MXCUrl> {
        let endpoint = match filename {
            Some(name) => format!("/_matrix/media/v3/upload?filename={name}"),
            None => "/_matrix/media/v3/upload".to_owned(),
        };

        let url = self.homeserver().join(endpoint.trim_start_matches('/'))?;
        let response = reqwest::Client::new()
            .post(url)
            .bearer_auth(self.token().await)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes)
            .send()
            .await?;
        let payload: Value = response.json().await?;
        let mxc = payload
            .get("content_uri")
            .and_then(Value::as_str)
            .context("missing content_uri in upload response")?;
        std::str::FromStr::from_str(mxc)
    }

    pub async fn upload_content(
        &self,
        bytes: Vec<u8>,
        content_type: Option<&str>,
        filename: Option<&str>,
    ) -> anyhow::Result<String> {
        let mxc = self
            .upload_media(
                bytes,
                content_type.unwrap_or("application/octet-stream"),
                filename,
            )
            .await?;
        Ok(mxc.to_string())
    }

    pub async fn upload_content_from_url(&self, url: &str) -> anyhow::Result<String> {
        let response = reqwest::Client::new().get(url).send().await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let bytes = response.bytes().await?.to_vec();
        self.upload_content(bytes, Some(&content_type), None).await
    }

    pub async fn download_media(&self, mxc: &MXCUrl) -> anyhow::Result<Vec<u8>> {
        let Some(stripped) = mxc.as_str().strip_prefix("mxc://") else {
            anyhow::bail!("invalid mxc uri");
        };
        let mut parts = stripped.splitn(2, '/');
        let server = parts.next().context("missing mxc server name")?;
        let media_id = parts.next().context("missing mxc media id")?;

        let endpoint = format!(
            "/_matrix/media/v3/download/{}/{}",
            encode_path_component(server),
            encode_path_component(media_id)
        );
        let url = self.homeserver().join(endpoint.trim_start_matches('/'))?;
        let bytes = reqwest::Client::new()
            .get(url)
            .bearer_auth(self.token().await)
            .send()
            .await?
            .bytes()
            .await?;
        Ok(bytes.to_vec())
    }

    pub async fn download_content(
        &self,
        mxc_url: &str,
        allow_remote: bool,
    ) -> anyhow::Result<DownloadedContent> {
        let mxc = mxc_url
            .parse::<MXCUrl>()
            .map_err(|_| anyhow::anyhow!("invalid mxc uri"))?;
        let stripped = mxc
            .as_str()
            .strip_prefix("mxc://")
            .ok_or_else(|| anyhow::anyhow!("invalid mxc uri"))?;
        let (server, media_id) = stripped
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("missing mxc server or media id"))?;

        let endpoint = format!(
            "/_matrix/media/v3/download/{}/{}?allow_remote={allow_remote}",
            encode_path_component(server),
            encode_path_component(media_id),
        );
        let url = self.homeserver().join(endpoint.trim_start_matches('/'))?;
        let response = reqwest::Client::new()
            .get(url)
            .bearer_auth(self.token().await)
            .send()
            .await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        let bytes = response.bytes().await?;
        Ok(DownloadedContent {
            data: bytes.to_vec(),
            content_type,
        })
    }

    pub fn mxc_to_http(
        &self,
        mxc: &str,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Option<String> {
        let stripped = mxc.strip_prefix("mxc://")?;
        let (server, media_id) = stripped.split_once('/')?;

        let base = self.homeserver().as_str().trim_end_matches('/');
        if let (Some(w), Some(h)) = (width, height) {
            Some(format!(
                "{}/_matrix/media/v3/thumbnail/{}/{}/?width={}&height={}",
                base, server, media_id, w, h
            ))
        } else {
            Some(format!(
                "{}/_matrix/media/v3/download/{}/{}",
                base, server, media_id
            ))
        }
    }

    pub fn mxc_to_http_thumbnail(
        &self,
        mxc: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Option<String> {
        let stripped = mxc.strip_prefix("mxc://")?;
        let (server, media_id) = stripped.split_once('/')?;
        let method = match method {
            "crop" | "scale" => method,
            _ => "scale",
        };
        let base = self.homeserver().as_str().trim_end_matches('/');
        Some(format!(
            "{}/_matrix/media/v3/thumbnail/{}/{}?width={}&height={}&method={}",
            base, server, media_id, width, height, method
        ))
    }
}

pub struct SynchronousMatrixClient {
    runtime: Runtime,
    inner: MatrixClient,
}

pub trait SyncEventHandler: Send + Sync {
    fn on_account_data(&self, _event: &Event) {}
    fn on_room_account_data(&self, _room_id: &str, _event: &Event) {}
    fn on_room_leave(&self, _room_id: &str, _event: &Event) {}
    fn on_room_invite(&self, _room_id: &str, _event: &Event) {}
    fn on_room_join(&self, _room_id: &str, _event: &Event) {}
    fn on_room_archived(&self, _room_id: &str, _event: &Event) {}
    fn on_room_upgraded(&self, _room_id: &str, _event: &Event) {}
    fn on_room_message(&self, _room_id: &str, _event: &Event) {}
    fn on_room_event(&self, _room_id: &str, _event: &Event) {}
}

impl SynchronousMatrixClient {
    pub fn new(inner: MatrixClient) -> anyhow::Result<Self> {
        let runtime = Runtime::new()?;
        Ok(Self { runtime, inner })
    }

    pub fn sync_once(
        &self,
        since: Option<&str>,
    ) -> anyhow::Result<Vec<crate::models::events::RoomEvent>> {
        self.runtime.block_on(self.inner.sync_once(since))
    }

    pub fn start_sync(&self, handler: Arc<dyn SyncEventHandler>) -> anyhow::Result<()> {
        let since: Option<String> = None;
        loop {
            let events = self.sync_once(since.as_deref())?;
            for room_event in &events {
                let room_id = &room_event.room_id;
                let event = &room_event.event;

                handler.on_room_event(room_id, event);

                let kind_str = match &event.kind {
                    crate::models::events::EventKind::RoomMessage => "m.room.message",
                    crate::models::events::EventKind::RoomMember => "m.room.member",
                    crate::models::events::EventKind::RoomCreate => "m.room.create",
                    crate::models::events::EventKind::Custom(s) => s.as_str(),
                    _ => "",
                };

                match kind_str {
                    "m.room.message" => handler.on_room_message(room_id, event),
                    "m.room.member" => {
                        if let Some(content) = event.content.as_object()
                            && let Some(membership) =
                                content.get("membership").and_then(|v| v.as_str())
                        {
                            match membership {
                                "join" => handler.on_room_join(room_id, event),
                                "leave" => handler.on_room_leave(room_id, event),
                                "invite" => handler.on_room_invite(room_id, event),
                                _ => {}
                            }
                        }
                    }
                    "m.room.create" => handler.on_room_upgraded(room_id, event),
                    _ => {}
                }
            }
            if !events.is_empty() {
                // In reality we should be saving/updating the next_batch token
            }
        }
    }

    pub fn send_message(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        self.runtime
            .block_on(self.inner.send_message(room_id, body))
    }

    pub fn send_text(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        self.runtime.block_on(self.inner.send_text(room_id, body))
    }

    pub fn send_notice(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        self.runtime.block_on(self.inner.send_notice(room_id, body))
    }

    pub fn send_event(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
    ) -> anyhow::Result<String> {
        self.runtime
            .block_on(self.inner.send_event(room_id, event_type, content))
    }

    pub fn send_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        content: &Value,
    ) -> anyhow::Result<String> {
        self.runtime.block_on(
            self.inner
                .send_state_event(room_id, event_type, state_key, content),
        )
    }

    pub fn redact_event(
        &self,
        room_id: &str,
        event_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<String> {
        self.runtime
            .block_on(self.inner.redact_event(room_id, event_id, reason))
    }

    pub fn join_room(&self, room_id_or_alias: &str) -> anyhow::Result<String> {
        self.runtime
            .block_on(self.inner.join_room(room_id_or_alias))
    }

    pub fn leave_room(&self, room_id: &str, reason: Option<&str>) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.leave_room(room_id, reason))
    }

    pub fn invite_user(&self, user_id: &str, room_id: &str) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.invite_user(user_id, room_id))
    }

    pub fn kick_user(
        &self,
        user_id: &str,
        room_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.kick_user(user_id, room_id, reason))
    }

    pub fn ban_user(
        &self,
        user_id: &str,
        room_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.ban_user(user_id, room_id, reason))
    }

    pub fn unban_user(&self, user_id: &str, room_id: &str) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.unban_user(user_id, room_id))
    }

    pub fn get_joined_rooms(&self) -> anyhow::Result<Vec<String>> {
        self.runtime.block_on(self.inner.get_joined_rooms())
    }

    pub fn get_joined_room_members(&self, room_id: &str) -> anyhow::Result<Vec<String>> {
        self.runtime
            .block_on(self.inner.get_joined_room_members(room_id))
    }

    pub fn get_room_state(&self, room_id: &str) -> anyhow::Result<Vec<Value>> {
        self.runtime.block_on(self.inner.get_room_state(room_id))
    }

    pub fn get_room_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> anyhow::Result<Value> {
        self.runtime.block_on(
            self.inner
                .get_room_state_event(room_id, event_type, state_key),
        )
    }

    pub fn get_user_id(&self) -> anyhow::Result<String> {
        self.runtime.block_on(self.inner.get_user_id())
    }

    pub fn profile(&self, user_id: &str) -> anyhow::Result<MatrixProfile> {
        self.runtime.block_on(self.inner.profile(user_id))
    }

    pub fn set_display_name(&self, display_name: &str) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.set_display_name(display_name))
    }

    pub fn set_avatar_url(&self, avatar_url: &str) -> anyhow::Result<()> {
        self.runtime.block_on(self.inner.set_avatar_url(avatar_url))
    }

    pub fn get_presence_status(&self, user_id: &str) -> anyhow::Result<Presence> {
        self.runtime
            .block_on(self.inner.get_presence_status(user_id))
    }

    pub fn set_presence_status(
        &self,
        status: Presence,
        status_msg: Option<&str>,
    ) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.set_presence_status(status, status_msg))
    }

    pub fn create_room(&self, options: &CreateRoom) -> anyhow::Result<String> {
        self.runtime.block_on(self.inner.create_room(options))
    }

    pub fn create_room_alias(&self, alias: &str, room_id: &str) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.create_room_alias(alias, room_id))
    }

    pub fn delete_room_alias(&self, alias: &str) -> anyhow::Result<()> {
        self.runtime.block_on(self.inner.delete_room_alias(alias))
    }

    pub fn lookup_room_alias(&self, alias: &str) -> anyhow::Result<RoomDirectoryLookupResponse> {
        self.runtime.block_on(self.inner.lookup_room_alias(alias))
    }

    pub fn set_account_data(&self, event_type: &str, content: &Value) -> anyhow::Result<()> {
        self.runtime
            .block_on(self.inner.set_account_data(event_type, content))
    }

    pub fn get_account_data(&self, event_type: &str) -> anyhow::Result<Value> {
        self.runtime
            .block_on(self.inner.get_account_data(event_type))
    }

    pub fn upload_media(
        &self,
        bytes: Vec<u8>,
        content_type: &str,
        filename: Option<&str>,
    ) -> anyhow::Result<MXCUrl> {
        self.runtime
            .block_on(self.inner.upload_media(bytes, content_type, filename))
    }

    pub fn download_media(&self, mxc: &MXCUrl) -> anyhow::Result<Vec<u8>> {
        self.runtime.block_on(self.inner.download_media(mxc))
    }

    pub fn mxc_to_http(
        &self,
        mxc: &str,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Option<String> {
        self.inner.mxc_to_http(mxc, width, height)
    }

    pub fn get_event(&self, room_id: &str, event_id: &str) -> anyhow::Result<Value> {
        self.runtime
            .block_on(self.inner.get_event(room_id, event_id))
    }

    pub fn get_server_versions(&self) -> anyhow::Result<Value> {
        self.runtime.block_on(self.inner.get_server_versions())
    }

    pub fn get_capabilities(&self) -> anyhow::Result<Value> {
        self.runtime.block_on(self.inner.get_capabilities())
    }
}

#[derive(Clone)]
pub struct DMs {
    client: MatrixClient,
    cached: Arc<RwLock<BTreeMap<String, Vec<String>>>>,
}

impl DMs {
    pub fn new(client: MatrixClient) -> Self {
        Self {
            client,
            cached: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn update(&self) -> anyhow::Result<()> {
        let map = match self.client.get_account_data("m.direct").await {
            Ok(v) => v,
            Err(_) => json!({}),
        };

        let mut cached = self.cached.write().await;
        cached.clear();

        if let Some(obj) = map.as_object() {
            for (user_id, room_ids) in obj {
                if let Some(rooms) = room_ids.as_array() {
                    let rooms: Vec<String> = rooms
                        .iter()
                        .filter_map(|r| r.as_str().map(ToOwned::to_owned))
                        .collect();
                    if !rooms.is_empty() {
                        cached.insert(user_id.clone(), rooms);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_or_create_dm(&self, user_id: &str) -> anyhow::Result<String> {
        let cached = self.cached.read().await;
        if let Some(rooms) = cached.get(user_id)
            && let Some(room_id) = rooms.first()
        {
            return Ok(room_id.clone());
        }
        drop(cached);

        let create = CreateRoom {
            invite: vec![user_id.to_owned()],
            is_direct: true,
            ..CreateRoom::default()
        };
        let room_id = self.client.create_room(&create).await?;

        let mut cached = self.cached.write().await;
        cached.insert(user_id.to_owned(), vec![room_id.clone()]);

        let mut obj = serde_json::Map::new();
        for (uid, rooms) in cached.iter() {
            obj.insert(
                uid.clone(),
                Value::Array(rooms.iter().map(|r| json!(r)).collect()),
            );
        }
        self.client
            .set_account_data("m.direct", &Value::Object(obj))
            .await?;

        Ok(room_id)
    }

    pub async fn is_dm(&self, room_id: &str) -> bool {
        let cached = self.cached.read().await;
        cached
            .values()
            .any(|rooms| rooms.contains(&room_id.to_owned()))
    }

    pub async fn persist_cache(&self) -> anyhow::Result<()> {
        let cached = self.cached.read().await;
        let mut obj = serde_json::Map::new();
        for (uid, rooms) in cached.iter() {
            obj.insert(
                uid.clone(),
                Value::Array(rooms.iter().map(|r| json!(r)).collect()),
            );
        }
        self.client
            .set_account_data("m.direct", &Value::Object(obj))
            .await
    }

    pub async fn handle_invite(&self, room_id: &str, event: &Event) -> anyhow::Result<()> {
        let sender = &event.sender;

        let is_direct = event
            .content
            .get("is_direct")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if is_direct {
            let mut cached = self.cached.write().await;
            if let Some(rooms) = cached.get_mut(sender) {
                if !rooms.contains(&room_id.to_owned()) {
                    rooms.push(room_id.to_owned());
                }
            } else {
                cached.insert(sender.clone(), vec![room_id.to_owned()]);
            }
        }
        Ok(())
    }
}

fn membership_to_query_value(membership: Membership) -> &'static str {
    match membership {
        Membership::Invite => "invite",
        Membership::Join => "join",
        Membership::Knock => "knock",
        Membership::Leave => "leave",
        Membership::Ban => "ban",
    }
}

fn plain_text_from_html(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

pub fn encode_path_component(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}
