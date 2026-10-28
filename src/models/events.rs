use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{PowerLevels, Presence};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    RoomMessage,
    RoomMember,
    RoomAliases,
    RoomCanonicalAlias,
    RoomCreate,
    RoomJoinRules,
    RoomPowerLevels,
    RoomRedaction,
    RoomPinnedEvents,
    RoomAvatar,
    RoomName,
    RoomTopic,
    SpaceChild,
    RoomEncryption,
    RoomEncrypted,
    Presence,
    Custom(String),
}

impl Default for EventKind {
    fn default() -> Self {
        Self::Custom("m.custom".to_owned())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Event {
    #[serde(default)]
    pub event_id: String,
    #[serde(rename = "type")]
    pub kind: EventKind,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub origin_server_ts: i64,
    #[serde(default)]
    pub content: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RoomEvent {
    pub room_id: String,
    #[serde(flatten)]
    pub event: Event,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StateEvent {
    #[serde(flatten)]
    pub room_event: RoomEvent,
    pub state_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Membership {
    Invite,
    Join,
    Knock,
    #[default]
    Leave,
    Ban,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MembershipEvent {
    pub user_id: String,
    pub membership: Membership,
}

impl MembershipEvent {
    pub fn effective_membership(&self) -> Membership {
        match self.membership {
            Membership::Ban => Membership::Leave,
            other => other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MessageEvent {
    pub msgtype: String,
    pub body: String,
    #[serde(default)]
    pub formatted_body: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub info: Option<serde_json::Value>,
    #[serde(default)]
    pub file: Option<EncryptedFile>,
}

/// File information for media messages.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FileInfo {
    #[serde(default)]
    pub mimetype: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

/// Thumbnail information for media messages.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ThumbnailInfo {
    #[serde(flatten)]
    #[serde(default)]
    pub info: FileInfo,
    #[serde(default)]
    pub width: Option<u64>,
    #[serde(default)]
    pub height: Option<u64>,
    #[serde(default)]
    pub orientation: Option<u64>,
}

/// Image information for media messages.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ImageInfo {
    #[serde(flatten)]
    #[serde(default)]
    pub info: FileInfo,
    #[serde(default)]
    pub width: Option<u64>,
    #[serde(default)]
    pub height: Option<u64>,
    #[serde(default)]
    pub orientation: Option<u64>,
}

/// Video information for media messages.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VideoInfo {
    #[serde(flatten)]
    #[serde(default)]
    pub info: ImageInfo,
    #[serde(default)]
    pub duration: Option<u64>,
}

/// Audio information for media messages.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AudioInfo {
    #[serde(flatten)]
    #[serde(default)]
    pub info: FileInfo,
    #[serde(default)]
    pub duration: Option<u64>,
}

/// Location information for location messages.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LocationInfo {
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Encrypted file information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedFile {
    pub url: String,
    pub key: JsonWebKey,
    pub iv: String,
    #[serde(default)]
    pub hashes: std::collections::HashMap<String, String>,
    pub version: String,
}

/// JSON Web Key for encrypted files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonWebKey {
    pub kty: String,
    #[serde(default)]
    pub alg: Option<String>,
    pub k: String,
    #[serde(default)]
    pub ext: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AliasesEvent {
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CanonicalAliasEvent {
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub alt_aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateEvent {
    pub creator: String,
    #[serde(default)]
    pub room_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum JoinRule {
    Public,
    #[default]
    Invite,
    Knock,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct JoinRulesEvent {
    pub join_rule: JoinRule,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PowerLevelsEvent {
    #[serde(flatten)]
    pub power_levels: PowerLevels,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RedactionEvent {
    pub redacts: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PinnedEventsEvent {
    #[serde(default)]
    pub pinned: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RoomAvatarEvent {
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RoomNameEvent {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RoomTopicEvent {
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SpaceChildEvent {
    pub child_room_id: String,
    #[serde(default)]
    pub via: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EncryptionEvent {
    pub algorithm: String,
    #[serde(default)]
    pub rotation_period_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EncryptedRoomEvent {
    pub algorithm: String,
    pub sender_key: String,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PresenceEvent {
    pub user_id: String,
    pub presence: Presence,
    #[serde(default)]
    pub status_msg: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidEventError {
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("invalid event kind")]
    InvalidType,
    #[error("invalid json: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

/// Error indicating that a Matrix event has been redacted and its content is no longer available.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, PartialEq, Eq)]
#[error("Event {event_id} was redacted")]
pub struct EventRedactedError {
    /// The ID of the redacted event.
    pub event_id: String,
    /// The reason for the redaction, if provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl EventRedactedError {
    pub fn new(event_id: impl Into<String>) -> Self {
        Self {
            event_id: event_id.into(),
            reason: None,
        }
    }

    pub fn with_reason(event_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            event_id: event_id.into(),
            reason: Some(reason.into()),
        }
    }
}

pub mod converter {
    use serde_json::Value;

    use super::{Event, EventKind, InvalidEventError, RoomEvent};

    pub fn parse_event(json: &Value) -> Result<Event, InvalidEventError> {
        let kind_raw = json
            .get("type")
            .and_then(Value::as_str)
            .ok_or(InvalidEventError::MissingField("type"))?;
        let kind = match kind_raw {
            "m.room.message" => EventKind::RoomMessage,
            "m.room.member" => EventKind::RoomMember,
            "m.room.aliases" => EventKind::RoomAliases,
            "m.room.canonical_alias" => EventKind::RoomCanonicalAlias,
            "m.room.create" => EventKind::RoomCreate,
            "m.room.join_rules" => EventKind::RoomJoinRules,
            "m.room.power_levels" => EventKind::RoomPowerLevels,
            "m.room.redaction" => EventKind::RoomRedaction,
            "m.room.pinned_events" => EventKind::RoomPinnedEvents,
            "m.room.avatar" => EventKind::RoomAvatar,
            "m.room.name" => EventKind::RoomName,
            "m.room.topic" => EventKind::RoomTopic,
            "m.space.child" => EventKind::SpaceChild,
            "m.room.encryption" => EventKind::RoomEncryption,
            "m.room.encrypted" => EventKind::RoomEncrypted,
            "m.presence" => EventKind::Presence,
            other => EventKind::Custom(other.to_owned()),
        };

        Ok(Event {
            event_id: json
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            kind,
            sender: json
                .get("sender")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            origin_server_ts: json
                .get("origin_server_ts")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            content: json.get("content").cloned().unwrap_or(Value::Null),
        })
    }

    pub fn parse_room_event(
        json: &Value,
        room_id: impl Into<String>,
    ) -> Result<RoomEvent, InvalidEventError> {
        Ok(RoomEvent {
            room_id: room_id.into(),
            event: parse_event(json)?,
        })
    }
}
