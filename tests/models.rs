use std::str::FromStr;
use std::time::Duration;

use matrix_bot_sdk::helpers::{
    MatrixEntity, MatrixGlob, ProfileCache, RichReply, RoomAlias, RoomID, UnpaddedBase64, UserID,
    parse_entity,
};
use matrix_bot_sdk::models::events::{
    AliasesEvent, CreateEvent, EncryptionEvent, EventKind, EventRedactedError, JoinRule,
    JoinRulesEvent, Membership, MembershipEvent, MessageEvent, PowerLevelsEvent, RedactionEvent,
    RoomAvatarEvent, RoomNameEvent, RoomTopicEvent, converter,
};
use matrix_bot_sdk::models::{
    MXCUrl, MatrixError, MatrixProfile, PLManager, PowerLevels, Presence, ROOM_ADMIN_PL,
    ROOM_CREATOR_PL, Space,
};
use serde_json::json;

#[test]
fn mxc_url_accepts_valid_prefix() {
    let parsed = MXCUrl::from_str("mxc://example.org/media-id").expect("valid mxc url");
    assert_eq!(parsed.as_str(), "mxc://example.org/media-id");
}

#[test]
fn mxc_url_rejects_invalid_prefix() {
    let result = MXCUrl::from_str("https://example.org/media-id");
    assert!(result.is_err());
}

#[test]
fn entity_parser_detects_user_id() {
    assert_eq!(
        parse_entity("@alice:example.org"),
        MatrixEntity::UserId("@alice:example.org".to_owned())
    );
}

#[test]
fn entity_parser_detects_room_id() {
    assert_eq!(
        parse_entity("!room:example.org"),
        MatrixEntity::RoomId("!room:example.org".to_owned())
    );
}

#[test]
fn entity_parser_detects_room_alias() {
    assert_eq!(
        parse_entity("#alias:example.org"),
        MatrixEntity::RoomAlias("#alias:example.org".to_owned())
    );
}

#[test]
fn entity_parser_detects_event_id() {
    assert_eq!(
        parse_entity("$event:example.org"),
        MatrixEntity::EventId("$event:example.org".to_owned())
    );
}

#[test]
fn entity_parser_returns_unknown_for_invalid() {
    assert_eq!(
        parse_entity("invalid"),
        MatrixEntity::Unknown("invalid".to_owned())
    );
}

#[test]
fn glob_matches_wildcards() {
    let glob = MatrixGlob::new("@*:example.org").expect("glob compiles");
    assert!(glob.is_match("@alice:example.org"));
    assert!(!glob.is_match("@alice:other.org"));
}

#[test]
fn glob_matches_leading_wildcard() {
    let glob = MatrixGlob::new("*example.org").expect("glob compiles");
    assert!(glob.is_match("123.example.org"));
    assert!(!glob.is_match("example.orgnot"));
}

#[test]
fn glob_matches_trailing_wildcard() {
    let glob = MatrixGlob::new("example.org*").expect("glob compiles");
    assert!(glob.is_match("example.org.123"));
    assert!(!glob.is_match("notexample.org"));
}

#[test]
fn glob_matches_middle_wildcard() {
    let glob = MatrixGlob::new("example*.org").expect("glob compiles");
    assert!(glob.is_match("example123.org"));
    assert!(!glob.is_match("notexample.org"));
}

#[test]
fn glob_matches_question_mark_single_char() {
    let glob = MatrixGlob::new("?example.org").expect("glob compiles");
    assert!(glob.is_match("1example.org"));
    assert!(!glob.is_match("12example.org"));
}

#[test]
fn glob_matches_no_special_chars() {
    let glob = MatrixGlob::new("example.org").expect("glob compiles");
    assert!(glob.is_match("example.org"));
    assert!(!glob.is_match("notexample.org"));
}

#[test]
fn base64_roundtrip_without_padding() {
    let raw = b"matrix-bot-sdk";
    let encoded = UnpaddedBase64::encode(raw);
    let decoded = UnpaddedBase64::decode(&encoded).expect("valid base64");
    assert_eq!(decoded, raw);
}

#[test]
fn base64_encodes_empty_string() {
    assert_eq!(UnpaddedBase64::encode(b""), "");
}

#[test]
fn base64_encodes_single_char() {
    assert_eq!(UnpaddedBase64::encode(b"f"), "Zg");
}

#[test]
fn base64_encodes_two_chars() {
    assert_eq!(UnpaddedBase64::encode(b"fo"), "Zm8");
}

#[test]
fn base64_encodes_three_chars() {
    assert_eq!(UnpaddedBase64::encode(b"foo"), "Zm9v");
}

#[test]
fn base64_encodes_six_chars() {
    assert_eq!(UnpaddedBase64::encode(b"foobar"), "Zm9vYmFy");
}

#[test]
fn rich_reply_contains_plain_and_html() {
    let reply = RichReply::new("@alice:example.org", "hello", "hi there");
    assert!(reply.body.contains("hi there"));
    assert!(reply.formatted_body.contains("<mx-reply>"));
}

#[test]
fn rich_reply_contains_sender() {
    let reply = RichReply::new("@alice:example.org", "hello", "hi there");
    assert!(reply.body.contains("@alice:example.org"));
    assert!(reply.formatted_body.contains("@alice:example.org"));
}

#[test]
fn rich_reply_html_escapes_content() {
    let reply = RichReply::new("@alice:example.org", "<script>", "hi");
    assert!(reply.formatted_body.contains("&lt;script&gt;"));
}

#[tokio::test]
async fn profile_cache_expires_entries() {
    let cache = ProfileCache::new(Duration::from_millis(5));
    cache
        .put(MatrixProfile {
            user_id: "@alice:example.org".to_owned(),
            displayname: Some("Alice".to_owned()),
            avatar_url: None,
        })
        .await;
    assert!(cache.get("@alice:example.org").await.is_some());
    tokio::time::sleep(Duration::from_millis(8)).await;
    assert!(cache.get("@alice:example.org").await.is_none());
}

#[tokio::test]
async fn profile_cache_stores_and_retrieves() {
    let cache = ProfileCache::new(Duration::from_secs(60));
    let profile = MatrixProfile {
        user_id: "@alice:example.org".to_owned(),
        displayname: Some("Alice".to_owned()),
        avatar_url: Some("mxc://example.org/avatar".to_owned()),
    };
    cache.put(profile.clone()).await;
    let cached = cache.get("@alice:example.org").await;
    assert!(cached.is_some());
    let cached = cached.unwrap();
    assert_eq!(cached.displayname, Some("Alice".to_owned()));
    assert_eq!(
        cached.avatar_url,
        Some("mxc://example.org/avatar".to_owned())
    );
}

#[test]
fn event_converter_maps_room_message_kind() {
    let raw = json!({
        "event_id": "$event",
        "type": "m.room.message",
        "sender": "@alice:example.org",
        "origin_server_ts": 42,
        "content": {
            "msgtype": "m.text",
            "body": "hello"
        }
    });

    let event = converter::parse_event(&raw).expect("event parses");
    assert_eq!(event.kind, EventKind::RoomMessage);
}

#[test]
fn event_converter_maps_room_member_kind() {
    let raw = json!({
        "event_id": "$event",
        "type": "m.room.member",
        "sender": "@alice:example.org",
        "content": {
            "membership": "join"
        }
    });

    let event = converter::parse_event(&raw).expect("event parses");
    assert_eq!(event.kind, EventKind::RoomMember);
}

#[test]
fn event_converter_maps_room_create_kind() {
    let raw = json!({
        "type": "m.room.create",
        "sender": "@alice:example.org",
        "content": {
            "creator": "@alice:example.org"
        }
    });

    let event = converter::parse_event(&raw).expect("event parses");
    assert_eq!(event.kind, EventKind::RoomCreate);
}

#[test]
fn event_converter_maps_custom_kind() {
    let raw = json!({
        "type": "org.example.custom",
        "content": {}
    });

    let event = converter::parse_event(&raw).expect("event parses");
    assert_eq!(
        event.kind,
        EventKind::Custom("org.example.custom".to_owned())
    );
}

#[test]
fn matrix_error_formats_message() {
    let err = MatrixError {
        errcode: "M_TEST".to_owned(),
        error: "Test error".to_owned(),
        retry_after_ms: None,
    };
    assert_eq!(format!("{err}"), "M_TEST: Test error");
}

#[test]
fn matrix_error_serializes_correctly() {
    let err = MatrixError {
        errcode: "M_TEST".to_owned(),
        error: "Test error".to_owned(),
        retry_after_ms: Some(1000),
    };
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("M_TEST"));
    assert!(json.contains("Test error"));
    assert!(json.contains("retry_after_ms"));
}

#[test]
fn matrix_error_deserializes_correctly() {
    let json = r#"{"errcode":"M_FORBIDDEN","error":"You are not allowed"}"#;
    let err: MatrixError = serde_json::from_str(json).unwrap();
    assert_eq!(err.errcode, "M_FORBIDDEN");
    assert_eq!(err.error, "You are not allowed");
    assert_eq!(err.retry_after_ms, None);
}

#[test]
fn matrix_profile_serializes_correctly() {
    let profile = MatrixProfile {
        user_id: "@alice:example.org".to_owned(),
        displayname: Some("Alice".to_owned()),
        avatar_url: None,
    };
    let json = serde_json::to_string(&profile).unwrap();
    assert!(json.contains("@alice:example.org"));
    assert!(json.contains("Alice"));
}

#[test]
fn matrix_profile_deserializes_correctly() {
    let json = r#"{"user_id":"@bob:example.org","displayname":"Bob"}"#;
    let profile: MatrixProfile = serde_json::from_str(json).unwrap();
    assert_eq!(profile.user_id, "@bob:example.org");
    assert_eq!(profile.displayname, Some("Bob".to_owned()));
    assert_eq!(profile.avatar_url, None);
}

#[test]
fn presence_serializes_correctly() {
    assert_eq!(
        serde_json::to_string(&Presence::Online).unwrap(),
        "\"online\""
    );
    assert_eq!(
        serde_json::to_string(&Presence::Unavailable).unwrap(),
        "\"unavailable\""
    );
    assert_eq!(
        serde_json::to_string(&Presence::Offline).unwrap(),
        "\"offline\""
    );
}

#[test]
fn presence_deserializes_correctly() {
    let online: Presence = serde_json::from_str("\"online\"").unwrap();
    assert_eq!(online, Presence::Online);

    let unavailable: Presence = serde_json::from_str("\"unavailable\"").unwrap();
    assert_eq!(unavailable, Presence::Unavailable);

    let offline: Presence = serde_json::from_str("\"offline\"").unwrap();
    assert_eq!(offline, Presence::Offline);
}

#[test]
fn power_levels_serializes_correctly() {
    let pl = PowerLevels {
        users_default: 0,
        events_default: 0,
        state_default: 50,
        ban: 50,
        redact: 50,
        kick: 50,
        invite: 50,
        users: [("@alice:example.org".to_owned(), 100)]
            .into_iter()
            .collect(),
        events: Default::default(),
        notifications: Default::default(),
    };
    let json = serde_json::to_string(&pl).unwrap();
    assert!(json.contains("users_default"));
    assert!(json.contains("state_default"));
}

#[test]
fn power_levels_deserializes_correctly() {
    let json = r#"{
        "users_default": 0,
        "events_default": 0,
        "state_default": 50,
        "ban": 50,
        "redact": 50,
        "kick": 50,
        "invite": 50,
        "users": {"@alice:example.org": 100}
    }"#;
    let pl: PowerLevels = serde_json::from_str(json).unwrap();
    assert_eq!(pl.users_default, 0);
    assert_eq!(pl.state_default, 50);
    assert_eq!(pl.users.get("@alice:example.org"), Some(&100));
}

#[test]
fn message_event_parses_correctly() {
    let json = json!({
        "msgtype": "m.text",
        "body": "Hello world"
    });
    let msg: MessageEvent = serde_json::from_value(json).unwrap();
    assert_eq!(msg.msgtype, "m.text");
    assert_eq!(msg.body, "Hello world");
}

#[test]
fn message_event_with_formatted_body() {
    let json = json!({
        "msgtype": "m.text",
        "body": "Hello",
        "format": "org.matrix.custom.html",
        "formatted_body": "<b>Hello</b>"
    });
    let msg: MessageEvent = serde_json::from_value(json).unwrap();
    assert_eq!(msg.format, Some("org.matrix.custom.html".to_owned()));
    assert_eq!(msg.formatted_body, Some("<b>Hello</b>".to_owned()));
}

#[test]
fn membership_event_parses_correctly() {
    let json = json!({
        "user_id": "@alice:example.org",
        "membership": "join"
    });
    let membership: MembershipEvent = serde_json::from_value(json).unwrap();
    assert_eq!(membership.user_id, "@alice:example.org");
    assert_eq!(membership.membership, Membership::Join);
}

#[test]
fn membership_enum_serializes_correctly() {
    assert_eq!(
        serde_json::to_string(&Membership::Invite).unwrap(),
        "\"invite\""
    );
    assert_eq!(
        serde_json::to_string(&Membership::Join).unwrap(),
        "\"join\""
    );
    assert_eq!(
        serde_json::to_string(&Membership::Leave).unwrap(),
        "\"leave\""
    );
    assert_eq!(serde_json::to_string(&Membership::Ban).unwrap(), "\"ban\"");
    assert_eq!(
        serde_json::to_string(&Membership::Knock).unwrap(),
        "\"knock\""
    );
}

#[test]
fn create_event_parses_correctly() {
    let json = json!({
        "creator": "@alice:example.org",
        "room_version": "6"
    });
    let create: CreateEvent = serde_json::from_value(json).unwrap();
    assert_eq!(create.creator, "@alice:example.org");
    assert_eq!(create.room_version, Some("6".to_owned()));
}

#[test]
fn create_event_defaults_room_version() {
    let json = json!({
        "creator": "@alice:example.org"
    });
    let create: CreateEvent = serde_json::from_value(json).unwrap();
    assert_eq!(create.room_version, None);
}

#[test]
fn encryption_event_parses_correctly() {
    let json = json!({
        "algorithm": "m.megolm.v1.aes-sha2",
        "rotation_period_ms": 604800000
    });
    let enc: EncryptionEvent = serde_json::from_value(json).unwrap();
    assert_eq!(enc.algorithm, "m.megolm.v1.aes-sha2");
    assert_eq!(enc.rotation_period_ms, Some(604800000));
}

#[test]
fn aliases_event_parses_correctly() {
    let json = json!({
        "aliases": ["#alias1:example.org", "#alias2:example.org"]
    });
    let aliases: AliasesEvent = serde_json::from_value(json).unwrap();
    assert_eq!(aliases.aliases.len(), 2);
    assert!(aliases.aliases.contains(&"#alias1:example.org".to_owned()));
}

#[test]
fn join_rules_event_parses_correctly() {
    let json = json!({
        "join_rule": "public"
    });
    let rules: JoinRulesEvent = serde_json::from_value(json).unwrap();
    assert_eq!(rules.join_rule, JoinRule::Public);
}

#[test]
fn join_rule_serializes_correctly() {
    assert_eq!(
        serde_json::to_string(&JoinRule::Public).unwrap(),
        "\"public\""
    );
    assert_eq!(
        serde_json::to_string(&JoinRule::Invite).unwrap(),
        "\"invite\""
    );
    assert_eq!(
        serde_json::to_string(&JoinRule::Knock).unwrap(),
        "\"knock\""
    );
    assert_eq!(
        serde_json::to_string(&JoinRule::Private).unwrap(),
        "\"private\""
    );
}

#[test]
fn redaction_event_parses_correctly() {
    let json = json!({
        "redacts": "$event:example.org",
        "reason": "spam"
    });
    let redaction: RedactionEvent = serde_json::from_value(json).unwrap();
    assert_eq!(redaction.redacts, "$event:example.org");
    assert_eq!(redaction.reason, Some("spam".to_owned()));
}

#[test]
fn room_avatar_event_parses_correctly() {
    let json = json!({
        "url": "mxc://example.org/avatar"
    });
    let avatar: RoomAvatarEvent = serde_json::from_value(json).unwrap();
    assert_eq!(avatar.url, Some("mxc://example.org/avatar".to_owned()));
}

#[test]
fn room_name_event_parses_correctly() {
    let json = json!({
        "name": "Test Room"
    });
    let name: RoomNameEvent = serde_json::from_value(json).unwrap();
    assert_eq!(name.name, Some("Test Room".to_owned()));
}

#[test]
fn room_topic_event_parses_correctly() {
    let json = json!({
        "topic": "This is a test room"
    });
    let topic: RoomTopicEvent = serde_json::from_value(json).unwrap();
    assert_eq!(topic.topic, Some("This is a test room".to_owned()));
}

#[test]
fn power_levels_event_parses_correctly() {
    let json = json!({
        "users_default": 0,
        "events_default": 0,
        "state_default": 50,
        "ban": 50,
        "redact": 50,
        "kick": 50,
        "invite": 50,
        "users": {
            "@alice:example.org": 100
        }
    });
    let pl: PowerLevelsEvent = serde_json::from_value(json).unwrap();
    assert_eq!(pl.power_levels.users_default, 0);
    assert_eq!(pl.power_levels.state_default, 50);
}

#[test]
fn membership_effective_membership_join() {
    let membership = MembershipEvent {
        user_id: "@bob:example.org".to_owned(),
        membership: Membership::Join,
    };
    assert_eq!(membership.effective_membership(), Membership::Join);
}

#[test]
fn membership_effective_membership_invite() {
    let membership = MembershipEvent {
        user_id: "@bob:example.org".to_owned(),
        membership: Membership::Invite,
    };
    assert_eq!(membership.effective_membership(), Membership::Invite);
}

#[test]
fn membership_effective_membership_leave() {
    let membership = MembershipEvent {
        user_id: "@bob:example.org".to_owned(),
        membership: Membership::Leave,
    };
    assert_eq!(membership.effective_membership(), Membership::Leave);
}

#[test]
fn membership_effective_membership_ban_becomes_leave() {
    let membership = MembershipEvent {
        user_id: "@bob:example.org".to_owned(),
        membership: Membership::Ban,
    };
    assert_eq!(membership.effective_membership(), Membership::Leave);
}

#[test]
fn power_levels_defaults_correctly() {
    let json = json!({});
    let pl: PowerLevels = serde_json::from_value(json).unwrap();
    assert_eq!(pl.ban, 50);
    assert_eq!(pl.invite, 50);
    assert_eq!(pl.kick, 50);
    assert_eq!(pl.redact, 50);
    assert_eq!(pl.users_default, 0);
    assert_eq!(pl.state_default, 50);
    assert_eq!(pl.events_default, 50);
}

#[test]
fn power_levels_notify_whole_room_level_defaults_to_50() {
    let pl = PowerLevels::default();
    assert_eq!(pl.notify_whole_room_level(), 50);
}

#[test]
fn power_levels_notify_whole_room_level_from_json() {
    let json = json!({
        "notifications": {"room": 82}
    });
    let pl: PowerLevels = serde_json::from_value(json).unwrap();
    assert_eq!(pl.notify_whole_room_level(), 82);
}

#[test]
fn power_levels_event_with_notifications() {
    let json = json!({
        "ban": 75,
        "events_default": 76,
        "invite": 77,
        "kick": 78,
        "redact": 79,
        "state_default": 80,
        "users_default": 81,
        "notifications": {"room": 82}
    });
    let pl: PowerLevelsEvent = serde_json::from_value(json).unwrap();
    assert_eq!(pl.power_levels.ban, 75);
    assert_eq!(pl.power_levels.invite, 77);
    assert_eq!(pl.power_levels.kick, 78);
    assert_eq!(pl.power_levels.redact, 79);
    assert_eq!(pl.power_levels.notify_whole_room_level(), 82);
    assert_eq!(pl.power_levels.users_default, 81);
    assert_eq!(pl.power_levels.state_default, 80);
    assert_eq!(pl.power_levels.events_default, 76);
}

#[test]
fn user_id_parses_localpart_and_domain() {
    let user_id = UserID::new("@alice:example.org").expect("valid user id");
    assert_eq!(user_id.localpart(), "alice");
    assert_eq!(user_id.domain(), "example.org");
    assert_eq!(format!("{user_id}"), "@alice:example.org");
}

#[test]
fn user_id_parses_with_port() {
    let user_id = UserID::new("@test:example.org:8448").expect("valid user id");
    assert_eq!(user_id.localpart(), "test");
    assert_eq!(user_id.domain(), "example.org:8448");
}

#[test]
fn user_id_rejects_invalid_format() {
    assert!(UserID::new("invalid").is_none());
    assert!(UserID::new("@no-colon").is_none());
}

#[test]
fn room_alias_parses_localpart_and_domain() {
    let alias = RoomAlias::new("#general:example.org").expect("valid room alias");
    assert_eq!(alias.localpart(), "general");
    assert_eq!(alias.domain(), "example.org");
    assert_eq!(format!("{alias}"), "#general:example.org");
}

#[test]
fn room_alias_rejects_invalid_format() {
    assert!(RoomAlias::new("invalid").is_none());
    assert!(RoomAlias::new("#no-colon").is_none());
}

#[test]
fn room_id_parses_localpart_and_domain() {
    let room_id = RoomID::new("!abc123:example.org").expect("valid room id");
    assert_eq!(room_id.localpart(), "abc123");
    assert_eq!(room_id.domain(), "example.org");
    assert_eq!(format!("{room_id}"), "!abc123:example.org");
}

#[test]
fn room_id_rejects_invalid_format() {
    assert!(RoomID::new("invalid").is_none());
    assert!(RoomID::new("!no-colon").is_none());
}

#[test]
fn matrix_profile_display_name_returns_displayname() {
    let profile = MatrixProfile {
        user_id: "@alice:example.org".to_owned(),
        displayname: Some("Alice".to_owned()),
        avatar_url: None,
    };
    assert_eq!(profile.display_name(), "Alice");
}

#[test]
fn matrix_profile_display_name_returns_localpart_when_no_displayname() {
    let profile = MatrixProfile {
        user_id: "@alice:example.org".to_owned(),
        displayname: None,
        avatar_url: None,
    };
    assert_eq!(profile.display_name(), "alice");
}

#[test]
fn matrix_profile_display_name_returns_localpart_when_empty_displayname() {
    let profile = MatrixProfile {
        user_id: "@bob:example.org".to_owned(),
        displayname: Some("".to_owned()),
        avatar_url: None,
    };
    assert_eq!(profile.display_name(), "bob");
}

#[test]
fn matrix_profile_avatar_url_resolved_returns_none_for_empty() {
    let profile = MatrixProfile {
        user_id: "@alice:example.org".to_owned(),
        displayname: Some("Alice".to_owned()),
        avatar_url: Some("".to_owned()),
    };
    assert_eq!(profile.avatar_url_resolved(), None);
}

#[test]
fn matrix_profile_avatar_url_resolved_returns_url_when_present() {
    let profile = MatrixProfile {
        user_id: "@alice:example.org".to_owned(),
        displayname: Some("Alice".to_owned()),
        avatar_url: Some("mxc://example.org/avatar".to_owned()),
    };
    assert_eq!(
        profile.avatar_url_resolved(),
        Some("mxc://example.org/avatar")
    );
}

// ---------------------------------------------------------------------------
// EventRedactedError tests
// ---------------------------------------------------------------------------

#[test]
fn event_redacted_error_basic() {
    let err = EventRedactedError::new("$abc:example.org");
    assert_eq!(err.event_id, "$abc:example.org");
    assert_eq!(err.reason, None);
}

#[test]
fn event_redacted_error_with_reason() {
    let err = EventRedactedError::with_reason("$abc:example.org", "spam");
    assert_eq!(err.event_id, "$abc:example.org");
    assert_eq!(err.reason, Some("spam".to_owned()));
}

#[test]
fn event_redacted_error_display() {
    let err = EventRedactedError::new("$abc:example.org");
    assert_eq!(format!("{err}"), "Event $abc:example.org was redacted");
}

#[test]
fn event_redacted_error_serializes_correctly() {
    let err = EventRedactedError::with_reason("$abc:example.org", "spam");
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("$abc:example.org"));
    assert!(json.contains("spam"));
}

#[test]
fn event_redacted_error_without_reason_omits_field() {
    let err = EventRedactedError::new("$abc:example.org");
    let json = serde_json::to_string(&err).unwrap();
    assert!(!json.contains("reason"));
}

// ---------------------------------------------------------------------------
// PLManager tests
// ---------------------------------------------------------------------------

#[test]
fn pl_manager_user_default_level() {
    let pl = PowerLevels {
        users_default: 0,
        ..Default::default()
    };
    let mgr = PLManager::new(pl);
    assert_eq!(mgr.user_level("@unknown:example.org"), 0);
}

#[test]
fn pl_manager_explicit_user_level() {
    let mut pl = PowerLevels::default();
    pl.users.insert("@alice:example.org".to_owned(), 100);
    let mgr = PLManager::new(pl);
    assert_eq!(mgr.user_level("@alice:example.org"), 100);
}

#[test]
fn pl_manager_user_has_pl_true() {
    let mut pl = PowerLevels::default();
    pl.users.insert("@alice:example.org".to_owned(), 100);
    let mgr = PLManager::new(pl);
    assert!(mgr.user_has_pl("@alice:example.org", 100));
    assert!(mgr.user_has_pl("@alice:example.org", 50));
}

#[test]
fn pl_manager_user_has_pl_false() {
    let pl = PowerLevels::default();
    let mgr = PLManager::new(pl);
    assert!(!mgr.user_has_pl("@user:example.org", 50));
}

#[test]
fn pl_manager_can_ban_at_50() {
    let mut pl = PowerLevels {
        users_default: 0,
        events_default: 50,
        state_default: 50,
        ban: 50,
        redact: 50,
        kick: 50,
        invite: 50,
        users: Default::default(),
        events: Default::default(),
        notifications: Default::default(),
    };
    pl.users.insert("@mod:example.org".to_owned(), 50);
    let mgr = PLManager::new(pl);
    assert!(mgr.can_ban("@mod:example.org"));
    assert!(!mgr.can_ban("@user:example.org"));
}

#[test]
fn pl_manager_can_kick_and_redact() {
    let mut pl = PowerLevels {
        users_default: 0,
        events_default: 50,
        state_default: 50,
        ban: 50,
        redact: 50,
        kick: 50,
        invite: 50,
        users: Default::default(),
        events: Default::default(),
        notifications: Default::default(),
    };
    pl.users.insert("@mod:example.org".to_owned(), 50);
    let mgr = PLManager::new(pl);
    assert!(mgr.can_kick("@mod:example.org"));
    assert!(mgr.can_redact("@mod:example.org"));
    assert!(!mgr.can_kick("@user:example.org"));
}

#[test]
fn pl_manager_can_invite() {
    let mut pl = PowerLevels {
        users_default: 0,
        events_default: 50,
        state_default: 50,
        ban: 50,
        redact: 50,
        kick: 50,
        invite: 50,
        users: Default::default(),
        events: Default::default(),
        notifications: Default::default(),
    };
    pl.users.insert("@mod:example.org".to_owned(), 50);
    let mgr = PLManager::new(pl);
    assert!(mgr.can_invite("@mod:example.org"));
    assert!(!mgr.can_invite("@user:example.org"));
}

#[test]
fn pl_manager_can_send_event_default() {
    let pl = PowerLevels {
        users_default: 0,
        events_default: 0,
        ..Default::default()
    };
    let mgr = PLManager::new(pl);
    assert!(mgr.can_send_event("@user:example.org", "m.room.message"));
}

#[test]
fn pl_manager_can_send_event_restricted() {
    let mut pl = PowerLevels {
        users_default: 0,
        events_default: 50,
        ..Default::default()
    };
    pl.events.insert("m.room.message".to_owned(), 100);
    let mgr = PLManager::new(pl);
    assert!(!mgr.can_send_event("@user:example.org", "m.room.message"));
}

#[test]
fn pl_constants_correct() {
    assert_eq!(ROOM_ADMIN_PL, 100);
    assert_eq!(ROOM_CREATOR_PL, 100);
}

// ---------------------------------------------------------------------------
// Space tests
// ---------------------------------------------------------------------------

#[test]
fn space_new_empty() {
    let space = Space::new("!room:example.org");
    assert_eq!(space.room_id, "!room:example.org");
    assert!(space.children.is_empty());
    assert!(space.parents.is_empty());
    assert!(space.suggested.is_empty());
}

#[test]
fn space_add_child() {
    let mut space = Space::new("!space:example.org");
    space.add_child("!child:example.org", false);
    assert!(space.has_child("!child:example.org"));
    assert!(space.suggested.is_empty());
}

#[test]
fn space_add_child_suggested() {
    let mut space = Space::new("!space:example.org");
    space.add_child("!child:example.org", true);
    assert!(space.has_child("!child:example.org"));
    assert!(space.suggested.contains(&"!child:example.org".to_owned()));
}

#[test]
fn space_add_child_no_duplicate() {
    let mut space = Space::new("!space:example.org");
    space.add_child("!child:example.org", false);
    space.add_child("!child:example.org", false);
    assert_eq!(space.children.len(), 1);
}

#[test]
fn space_remove_child() {
    let mut space = Space::new("!space:example.org");
    space.add_child("!child:example.org", true);
    space.remove_child("!child:example.org");
    assert!(!space.has_child("!child:example.org"));
    assert!(space.suggested.is_empty());
}

#[test]
fn space_has_child_returns_false_for_missing() {
    let space = Space::new("!space:example.org");
    assert!(!space.has_child("!child:example.org"));
}

#[test]
fn space_serializes_correctly() {
    let mut space = Space::new("!space:example.org");
    space.add_child("!child:example.org", true);
    let json = serde_json::to_string(&space).unwrap();
    assert!(json.contains("!space:example.org"));
    assert!(json.contains("!child:example.org"));
}
