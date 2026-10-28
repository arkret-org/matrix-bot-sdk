use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub mod events;
pub mod unstable;

/// Power level required to be considered a room admin.
pub const ROOM_ADMIN_PL: i64 = 100;

/// Power level assigned to the room creator.
pub const ROOM_CREATOR_PL: i64 = 100;

/// Manages power levels for a Matrix room.
///
/// Provides convenience methods for checking and manipulating power levels
/// beyond what the raw `PowerLevels` struct offers.
#[derive(Debug, Clone)]
pub struct PLManager {
    pub power_levels: PowerLevels,
}

impl PLManager {
    pub fn new(power_levels: PowerLevels) -> Self {
        Self { power_levels }
    }

    /// Returns the effective power level of the given user.
    pub fn user_level(&self, user_id: &str) -> i64 {
        self.power_levels
            .users
            .get(user_id)
            .copied()
            .unwrap_or(self.power_levels.users_default)
    }

    /// Returns true if the user can send the given event type.
    pub fn can_send_event(&self, user_id: &str, event_type: &str) -> bool {
        let user_pl = self.user_level(user_id);
        let required = self
            .power_levels
            .events
            .get(event_type)
            .copied()
            .unwrap_or(self.power_levels.events_default);
        user_pl >= required
    }

    /// Returns true if the user has at least the given power level.
    pub fn user_has_pl(&self, user_id: &str, required: i64) -> bool {
        self.user_level(user_id) >= required
    }

    /// Returns true if the user can ban members from the room.
    pub fn can_ban(&self, user_id: &str) -> bool {
        self.user_level(user_id) >= self.power_levels.ban
    }

    /// Returns true if the user can kick members from the room.
    pub fn can_kick(&self, user_id: &str) -> bool {
        self.user_level(user_id) >= self.power_levels.kick
    }

    /// Returns true if the user can redact events.
    pub fn can_redact(&self, user_id: &str) -> bool {
        self.user_level(user_id) >= self.power_levels.redact
    }

    /// Returns true if the user can invite members.
    pub fn can_invite(&self, user_id: &str) -> bool {
        self.user_level(user_id) >= self.power_levels.invite
    }

    /// Returns true if the user can set state events.
    pub fn can_set_state(&self, user_id: &str) -> bool {
        self.user_level(user_id) >= self.power_levels.state_default
    }
}

/// Represents a Matrix Space, which is a special type of room used to group rooms.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Space {
    /// The room ID of this space.
    pub room_id: String,
    /// Child room IDs that belong to this space.
    #[serde(default)]
    pub children: Vec<String>,
    /// Parent space room IDs for this space.
    #[serde(default)]
    pub parents: Vec<String>,
    /// Suggested child rooms to show to new members.
    #[serde(default)]
    pub suggested: Vec<String>,
}

impl Space {
    pub fn new(room_id: impl Into<String>) -> Self {
        Self {
            room_id: room_id.into(),
            children: Vec::new(),
            parents: Vec::new(),
            suggested: Vec::new(),
        }
    }

    /// Adds a child room. If `suggested` is true, it will also appear in the suggested list.
    pub fn add_child(&mut self, room_id: impl Into<String>, suggested: bool) {
        let room_id = room_id.into();
        if !self.children.contains(&room_id) {
            self.children.push(room_id.clone());
        }
        if suggested && !self.suggested.contains(&room_id) {
            self.suggested.push(room_id);
        }
    }

    /// Removes a child room from the space (including from suggested list).
    pub fn remove_child(&mut self, room_id: &str) {
        self.children.retain(|id| id != room_id);
        self.suggested.retain(|id| id != room_id);
    }

    /// Returns true if the given room_id is a child of this space.
    pub fn has_child(&self, room_id: &str) -> bool {
        self.children.contains(&room_id.to_owned())
    }
}

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, PartialEq, Eq)]
#[error("{errcode}: {error}")]
pub struct MatrixError {
    pub errcode: String,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MatrixProfile {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

impl MatrixProfile {
    pub fn display_name(&self) -> &str {
        if let Some(ref name) = self.displayname
            && !name.is_empty()
        {
            return name;
        }
        if let Some(colon_pos) = self.user_id.find(':')
            && colon_pos > 1
        {
            return &self.user_id[1..colon_pos];
        }
        &self.user_id
    }

    pub fn avatar_url_resolved(&self) -> Option<&str> {
        self.avatar_url.as_ref().and_then(|url| {
            if url.is_empty() {
                None
            } else {
                Some(url.as_str())
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Online,
    Unavailable,
    #[default]
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PowerLevels {
    #[serde(default = "default_zero")]
    pub users_default: i64,
    #[serde(default = "default_fifty")]
    pub events_default: i64,
    #[serde(default = "default_fifty")]
    pub state_default: i64,
    #[serde(default = "default_fifty")]
    pub ban: i64,
    #[serde(default = "default_fifty")]
    pub redact: i64,
    #[serde(default = "default_fifty")]
    pub kick: i64,
    #[serde(default = "default_fifty")]
    pub invite: i64,
    #[serde(default)]
    pub users: BTreeMap<String, i64>,
    #[serde(default)]
    pub events: BTreeMap<String, i64>,
    #[serde(default)]
    pub notifications: BTreeMap<String, i64>,
}

fn default_zero() -> i64 {
    0
}

fn default_fifty() -> i64 {
    50
}

impl PowerLevels {
    pub fn notify_whole_room_level(&self) -> i64 {
        self.notifications.get("room").copied().unwrap_or(50)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct EventContext {
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
    #[serde(default)]
    pub events_before: Vec<events::Event>,
    #[serde(default)]
    pub event: Option<events::Event>,
    #[serde(default)]
    pub events_after: Vec<events::Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OpenIDConnect {
    pub access_token: String,
    pub token_type: String,
    pub matrix_server_name: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Threepid {
    pub medium: String,
    pub address: String,
    pub validated_at: Option<u64>,
    pub added_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Spaces {
    pub room_id: String,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Crypto {
    pub room_id: String,
    pub algorithm: String,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Account {
    pub user_id: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub is_guest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ServerVersions {
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub unstable_features: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateRoom {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_alias_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invite: Vec<String>,
    #[serde(default)]
    pub is_direct: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct MXCUrl(String);

impl MXCUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for MXCUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for MXCUrl {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with("mxc://") {
            anyhow::bail!("invalid MXC URL");
        }
        Ok(Self(s.to_owned()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MSC2176 {
    pub event_id: String,
    pub sequence: u64,
    pub chunk_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MSC2716BatchSendResponse {
    #[serde(default)]
    pub event_ids: Vec<String>,
    #[serde(default)]
    pub next_batch_id: String,
    pub state_events: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Policies {
    #[serde(default)]
    pub user_rules: Vec<String>,
    #[serde(default)]
    pub room_rules: Vec<String>,
    #[serde(default)]
    pub server_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Capabilities {
    #[serde(default)]
    pub room_versions: Vec<String>,
    pub can_change_password: bool,
    pub can_set_displayname: bool,
    pub can_set_avatar_url: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct KeyBackup {
    pub version: String,
    pub algorithm: String,
    pub etag: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IdentityServerModels {
    pub id_server: String,
    #[serde(default)]
    pub terms_of_service_url: Option<String>,
}
