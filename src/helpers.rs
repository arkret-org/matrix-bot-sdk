use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use regex::Regex;
use tokio::sync::RwLock;

use crate::models::MatrixProfile;

pub type UserId = String;
pub type RoomId = String;
pub type EventId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatrixEntity {
    UserId(String),
    RoomId(String),
    RoomAlias(String),
    EventId(String),
    Unknown(String),
}

pub fn parse_entity(value: &str) -> MatrixEntity {
    if value.starts_with('@') {
        MatrixEntity::UserId(value.to_owned())
    } else if value.starts_with('!') {
        MatrixEntity::RoomId(value.to_owned())
    } else if value.starts_with('#') {
        MatrixEntity::RoomAlias(value.to_owned())
    } else if value.starts_with('$') {
        MatrixEntity::EventId(value.to_owned())
    } else {
        MatrixEntity::Unknown(value.to_owned())
    }
}

fn parse_localpart_and_domain(sigil: char, value: &str) -> Option<(String, String)> {
    if !value.starts_with(sigil) {
        return None;
    }
    let rest = &value[1..];
    let colon_pos = rest.find(':')?;
    let localpart = rest[..colon_pos].to_owned();
    let domain = rest[colon_pos + 1..].to_owned();
    Some((localpart, domain))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserID {
    value: String,
    localpart: String,
    domain: String,
}

impl UserID {
    pub fn new(value: &str) -> Option<Self> {
        let (localpart, domain) = parse_localpart_and_domain('@', value)?;
        Some(Self {
            value: value.to_owned(),
            localpart,
            domain,
        })
    }

    pub fn localpart(&self) -> &str {
        &self.localpart
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }
}

impl std::fmt::Display for UserID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomAlias {
    value: String,
    localpart: String,
    domain: String,
}

impl RoomAlias {
    pub fn new(value: &str) -> Option<Self> {
        let (localpart, domain) = parse_localpart_and_domain('#', value)?;
        Some(Self {
            value: value.to_owned(),
            localpart,
            domain,
        })
    }

    pub fn localpart(&self) -> &str {
        &self.localpart
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }
}

impl std::fmt::Display for RoomAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomID {
    value: String,
    localpart: String,
    domain: String,
}

impl RoomID {
    pub fn new(value: &str) -> Option<Self> {
        let (localpart, domain) = parse_localpart_and_domain('!', value)?;
        Some(Self {
            value: value.to_owned(),
            localpart,
            domain,
        })
    }

    pub fn localpart(&self) -> &str {
        &self.localpart
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }
}

impl std::fmt::Display for RoomID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

#[derive(Debug, Clone)]
pub struct MatrixGlob {
    pattern: String,
    regex: Regex,
}

impl MatrixGlob {
    pub fn new(pattern: impl Into<String>) -> anyhow::Result<Self> {
        let pattern = pattern.into();
        let escaped = regex::escape(&pattern)
            .replace("\\*", ".*")
            .replace("\\?", ".");
        let regex = Regex::new(&format!("^{escaped}$"))?;
        Ok(Self { pattern, regex })
    }

    pub fn is_match(&self, value: &str) -> bool {
        self.regex.is_match(value)
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

#[derive(Debug, Clone)]
pub struct MentionPillResult {
    pub html: String,
    pub text: String,
}

pub struct MentionPill;

impl MentionPill {
    pub fn html(user_id: &str, display_name: Option<&str>) -> String {
        let label = display_name.unwrap_or(user_id);
        format!(r#"<a href="https://matrix.to/#/{user_id}">{label}</a>"#)
    }

    pub fn for_user(user_id: &str) -> MentionPillResult {
        MentionPillResult {
            html: Self::html(user_id, None),
            text: user_id.to_owned(),
        }
    }

    pub fn for_user_with_display_name(user_id: &str, display_name: &str) -> MentionPillResult {
        MentionPillResult {
            html: Self::html(user_id, Some(display_name)),
            text: display_name.to_owned(),
        }
    }

    pub fn for_room(room_id_or_alias: &str) -> MentionPillResult {
        MentionPillResult {
            html: format!(
                r#"<a href="https://matrix.to/#/{room_id_or_alias}">{room_id_or_alias}</a>"#
            ),
            text: room_id_or_alias.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PermalinkParts {
    pub room_id_or_alias: Option<String>,
    pub event_id: Option<String>,
    pub user_id: Option<String>,
    pub via_servers: Vec<String>,
}

pub struct Permalinks;

impl Permalinks {
    pub fn room(room_id: &str) -> String {
        format!("https://matrix.to/#/{room_id}")
    }

    pub fn room_with_via(room_id: &str, via: &[&str]) -> String {
        if via.is_empty() {
            Self::room(room_id)
        } else {
            let via_str = via
                .iter()
                .map(|v| format!("via={v}"))
                .collect::<Vec<_>>()
                .join("&");
            format!("https://matrix.to/#/{room_id}?{via_str}")
        }
    }

    pub fn event(room_id: &str, event_id: &str) -> String {
        format!("https://matrix.to/#/{room_id}/{event_id}")
    }

    pub fn event_with_via(room_id: &str, event_id: &str, via: &[&str]) -> String {
        if via.is_empty() {
            Self::event(room_id, event_id)
        } else {
            let via_str = via
                .iter()
                .map(|v| format!("via={v}"))
                .collect::<Vec<_>>()
                .join("&");
            format!("https://matrix.to/#/{room_id}/{event_id}?{via_str}")
        }
    }

    pub fn user(user_id: &str) -> String {
        format!("https://matrix.to/#/{user_id}")
    }

    pub fn parse(url: &str) -> Option<PermalinkParts> {
        let prefix = "https://matrix.to/#/";
        if !url.starts_with(prefix) {
            return None;
        }
        let rest = &url[prefix.len()..];
        let (path, via_servers) = if let Some(qpos) = rest.find('?') {
            let path = &rest[..qpos];
            let query = &rest[qpos + 1..];
            let via: Vec<String> = query
                .split('&')
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    if parts.next() == Some("via") {
                        parts.next().map(ToOwned::to_owned)
                    } else {
                        None
                    }
                })
                .collect();
            (path, via)
        } else {
            (rest, Vec::new())
        };

        let path = path.trim_end_matches('/');

        if path.starts_with('@') {
            return Some(PermalinkParts {
                user_id: Some(path.to_owned()),
                room_id_or_alias: None,
                event_id: None,
                via_servers,
            });
        }

        let parts: Vec<&str> = path.splitn(2, '/').collect();
        let room_id_or_alias = parts.first().map(|s| (*s).to_owned());
        let event_id = parts.get(1).map(|s| (*s).to_owned());

        Some(PermalinkParts {
            room_id_or_alias,
            event_id,
            user_id: None,
            via_servers,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RichReply {
    pub body: String,
    pub formatted_body: String,
}

impl RichReply {
    pub fn new(original_sender: &str, original_body: &str, reply_body: &str) -> Self {
        let plain_prefix = format!("> <{original_sender}> {original_body}\n\n");
        let html_prefix = format!(
            "<mx-reply><blockquote><a href=\"{}\">In reply to</a> <a href=\"{}\">{}</a><br />{}</blockquote></mx-reply>",
            Permalinks::room("!unknown:example.org"),
            Permalinks::user(original_sender),
            original_sender,
            html_escape(original_body),
        );

        Self {
            body: format!("{plain_prefix}{reply_body}"),
            formatted_body: format!("{html_prefix}{}", html_escape(reply_body)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileCache {
    ttl: Duration,
    entries: Arc<RwLock<HashMap<String, CachedProfile>>>,
}

#[derive(Debug, Clone)]
struct CachedProfile {
    profile: MatrixProfile,
    expires_at: Instant,
}

impl ProfileCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, user_id: &str) -> Option<MatrixProfile> {
        let mut guard = self.entries.write().await;
        match guard.get(user_id) {
            Some(entry) if entry.expires_at > Instant::now() => Some(entry.profile.clone()),
            Some(_) => {
                guard.remove(user_id);
                None
            }
            None => None,
        }
    }

    pub async fn put(&self, profile: MatrixProfile) {
        let key = profile.user_id.clone();
        self.entries.write().await.insert(
            key,
            CachedProfile {
                profile,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    /// Returns the cached profile for the given user ID, or fetches it using the provided
    /// async callback if not cached (or expired). The fetched profile is then stored in the cache.
    pub async fn get_or_fetch<F, Fut>(&self, user_id: &str, fetch: F) -> Option<MatrixProfile>
    where
        F: FnOnce(&str) -> Fut,
        Fut: std::future::Future<Output = Option<MatrixProfile>>,
    {
        if let Some(profile) = self.get(user_id).await {
            return Some(profile);
        }
        if let Some(profile) = fetch(user_id).await {
            self.put(profile.clone()).await;
            return Some(profile);
        }
        None
    }
}

pub struct UnpaddedBase64;

impl UnpaddedBase64 {
    pub fn encode(bytes: &[u8]) -> String {
        STANDARD_NO_PAD.encode(bytes)
    }

    pub fn decode(value: &str) -> anyhow::Result<Vec<u8>> {
        Ok(STANDARD_NO_PAD.decode(value)?)
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Validates a Matrix Space order string.
///
/// A valid order string must:
/// - Be non-empty
/// - Be at most 50 characters long
/// - Contain only ASCII printable characters (0x20–0x7E)
pub fn validate_space_order_string(s: &str) -> bool {
    if s.is_empty() || s.len() > 50 {
        return false;
    }
    s.chars().all(|c| c.is_ascii() && (' '..='~').contains(&c))
}
