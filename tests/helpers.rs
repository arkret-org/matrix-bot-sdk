use matrix_bot_sdk::helpers::{MentionPill, Permalinks};

#[test]
fn permalink_for_room_id() {
    let room_id = "!test:example.org";
    let expected = format!("https://matrix.to/#/{room_id}");
    assert_eq!(Permalinks::room(room_id), expected);
}

#[test]
fn permalink_for_room_alias() {
    let alias = "#test:example.org";
    let expected = format!("https://matrix.to/#/{alias}");
    assert_eq!(Permalinks::room(alias), expected);
}

#[test]
fn permalink_for_room_with_via() {
    let room_id = "!test:example.org";
    let via = ["one.example.org", "two.example.org"];
    let result = Permalinks::room_with_via(room_id, &via);
    assert!(result.starts_with("https://matrix.to/#/!test:example.org?"));
    assert!(result.contains("via=one.example.org"));
    assert!(result.contains("via=two.example.org"));
}

#[test]
fn permalink_for_event() {
    let room_id = "!test:example.org";
    let event_id = "$test:example.org";
    let expected = format!("https://matrix.to/#/{room_id}/{event_id}");
    assert_eq!(Permalinks::event(room_id, event_id), expected);
}

#[test]
fn permalink_for_event_with_room_alias() {
    let alias = "#test:example.org";
    let event_id = "$test:example.org";
    let expected = format!("https://matrix.to/#/{alias}/{event_id}");
    assert_eq!(Permalinks::event(alias, event_id), expected);
}

#[test]
fn permalink_for_event_with_via() {
    let room_id = "!test:example.org";
    let event_id = "$test:example.org";
    let via = ["one.example.org", "two.example.org"];
    let result = Permalinks::event_with_via(room_id, event_id, &via);
    assert!(result.starts_with("https://matrix.to/#/!test:example.org/$test:example.org?"));
    assert!(result.contains("via=one.example.org"));
    assert!(result.contains("via=two.example.org"));
}

#[test]
fn permalink_for_user() {
    let user_id = "@test:example.org";
    let expected = format!("https://matrix.to/#/{user_id}");
    assert_eq!(Permalinks::user(user_id), expected);
}

#[test]
fn permalink_parse_user_url() {
    let url = "https://matrix.to/#/@test:example.org";
    let parsed = Permalinks::parse(url).expect("should parse");
    assert_eq!(parsed.user_id, Some("@test:example.org".to_owned()));
    assert_eq!(parsed.room_id_or_alias, None);
    assert_eq!(parsed.event_id, None);
    assert!(parsed.via_servers.is_empty());
}

#[test]
fn permalink_parse_room_alias_url() {
    let url = "https://matrix.to/#/#test:example.org";
    let parsed = Permalinks::parse(url).expect("should parse");
    assert_eq!(
        parsed.room_id_or_alias,
        Some("#test:example.org".to_owned())
    );
    assert_eq!(parsed.user_id, None);
    assert_eq!(parsed.event_id, None);
    assert!(parsed.via_servers.is_empty());
}

#[test]
fn permalink_parse_room_id_url() {
    let url = "https://matrix.to/#/!test:example.org";
    let parsed = Permalinks::parse(url).expect("should parse");
    assert_eq!(
        parsed.room_id_or_alias,
        Some("!test:example.org".to_owned())
    );
    assert_eq!(parsed.user_id, None);
    assert_eq!(parsed.event_id, None);
    assert!(parsed.via_servers.is_empty());
}

#[test]
fn permalink_parse_event_url() {
    let url = "https://matrix.to/#/!test:example.org/$event:example.org";
    let parsed = Permalinks::parse(url).expect("should parse");
    assert_eq!(
        parsed.room_id_or_alias,
        Some("!test:example.org".to_owned())
    );
    assert_eq!(parsed.event_id, Some("$event:example.org".to_owned()));
    assert_eq!(parsed.user_id, None);
    assert!(parsed.via_servers.is_empty());
}

#[test]
fn permalink_parse_event_url_with_alias() {
    let url = "https://matrix.to/#/#test:example.org/$event:example.org";
    let parsed = Permalinks::parse(url).expect("should parse");
    assert_eq!(
        parsed.room_id_or_alias,
        Some("#test:example.org".to_owned())
    );
    assert_eq!(parsed.event_id, Some("$event:example.org".to_owned()));
}

#[test]
fn permalink_parse_url_with_via_servers() {
    let url = "https://matrix.to/#/!test:example.org/$event:example.org?via=one.example.org&via=two.example.org";
    let parsed = Permalinks::parse(url).expect("should parse");
    assert_eq!(
        parsed.room_id_or_alias,
        Some("!test:example.org".to_owned())
    );
    assert_eq!(parsed.event_id, Some("$event:example.org".to_owned()));
    assert_eq!(
        parsed.via_servers,
        vec!["one.example.org", "two.example.org"]
    );
}

#[test]
fn permalink_parse_room_url_with_via_servers() {
    let url = "https://matrix.to/#/!example:example.org?via=example.org";
    let parsed = Permalinks::parse(url).expect("should parse");
    assert_eq!(
        parsed.room_id_or_alias,
        Some("!example:example.org".to_owned())
    );
    assert_eq!(parsed.event_id, None);
    assert_eq!(parsed.via_servers, vec!["example.org"]);
}

#[test]
fn permalink_parse_returns_none_for_invalid_url() {
    assert!(Permalinks::parse("https://example.org").is_none());
    assert!(Permalinks::parse("not a url").is_none());
}

#[test]
fn permalink_roundtrip_user() {
    let user_id = "@test:example.org";
    let url = Permalinks::user(user_id);
    let parsed = Permalinks::parse(&url).expect("should parse");
    assert_eq!(parsed.user_id, Some(user_id.to_owned()));
}

#[test]
fn permalink_roundtrip_room() {
    let room_id = "!test:example.org";
    let url = Permalinks::room(room_id);
    let parsed = Permalinks::parse(&url).expect("should parse");
    assert_eq!(parsed.room_id_or_alias, Some(room_id.to_owned()));
}

#[test]
fn permalink_roundtrip_event() {
    let room_id = "!test:example.org";
    let event_id = "$event:example.org";
    let url = Permalinks::event(room_id, event_id);
    let parsed = Permalinks::parse(&url).expect("should parse");
    assert_eq!(parsed.room_id_or_alias, Some(room_id.to_owned()));
    assert_eq!(parsed.event_id, Some(event_id.to_owned()));
}

#[test]
fn mention_pill_html_basic() {
    let html = MentionPill::html("@alice:example.org", None);
    assert!(html.contains("@alice:example.org"));
    assert!(html.starts_with("<a href=\"https://matrix.to/#/@alice:example.org\">"));
    assert!(html.ends_with("</a>"));
}

#[test]
fn mention_pill_html_with_display_name() {
    let html = MentionPill::html("@alice:example.org", Some("Alice"));
    assert!(html.contains(">Alice<"));
    assert!(html.contains("https://matrix.to/#/@alice:example.org"));
}

#[test]
fn mention_pill_for_user() {
    let pill = MentionPill::for_user("@alice:example.org");
    assert!(pill.html.contains("@alice:example.org"));
    assert_eq!(pill.text, "@alice:example.org");
}

#[test]
fn mention_pill_for_user_with_display_name() {
    let pill = MentionPill::for_user_with_display_name("@alice:example.org", "Alice");
    assert!(pill.html.contains(">Alice<"));
    assert_eq!(pill.text, "Alice");
}

#[test]
fn mention_pill_for_room_id() {
    let pill = MentionPill::for_room("!room:example.org");
    assert!(pill.html.contains("!room:example.org"));
    assert_eq!(pill.text, "!room:example.org");
}

#[test]
fn mention_pill_for_room_alias() {
    let pill = MentionPill::for_room("#room:example.org");
    assert!(pill.html.contains("#room:example.org"));
    assert_eq!(pill.text, "#room:example.org");
}

// ---------------------------------------------------------------------------
// ProfileCache::get_or_fetch tests
// ---------------------------------------------------------------------------

use std::time::Duration;

use matrix_bot_sdk::helpers::ProfileCache;
use matrix_bot_sdk::models::MatrixProfile;

#[tokio::test]
async fn profile_cache_get_or_fetch_calls_fetch_when_missing() {
    let cache = ProfileCache::new(Duration::from_secs(60));

    let profile = cache
        .get_or_fetch("@alice:example.org", |_user_id| async move {
            Some(MatrixProfile {
                user_id: "@alice:example.org".to_owned(),
                displayname: Some("Alice".to_owned()),
                avatar_url: None,
            })
        })
        .await;

    assert!(profile.is_some());
    let p = profile.unwrap();
    assert_eq!(p.user_id, "@alice:example.org");
    assert_eq!(p.displayname, Some("Alice".to_owned()));
}

#[tokio::test]
async fn profile_cache_get_or_fetch_uses_cache_on_second_call() {
    let cache = ProfileCache::new(Duration::from_secs(60));

    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    // First call fetches
    cache
        .get_or_fetch("@bob:example.org", |_user_id| {
            let counter = call_count_clone.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Some(MatrixProfile {
                    user_id: "@bob:example.org".to_owned(),
                    displayname: Some("Bob".to_owned()),
                    avatar_url: None,
                })
            }
        })
        .await;

    let call_count_clone2 = call_count.clone();
    // Second call should use cache, not call fetch again
    cache
        .get_or_fetch("@bob:example.org", |_user_id| {
            let counter = call_count_clone2.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Some(MatrixProfile {
                    user_id: "@bob:example.org".to_owned(),
                    displayname: Some("Bob Updated".to_owned()),
                    avatar_url: None,
                })
            }
        })
        .await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn profile_cache_get_or_fetch_returns_none_when_fetch_returns_none() {
    let cache = ProfileCache::new(Duration::from_secs(60));

    let profile = cache
        .get_or_fetch("@unknown:example.org", |_user_id| async move { None })
        .await;

    assert!(profile.is_none());
}

// ---------------------------------------------------------------------------
// validate_space_order_string tests
// ---------------------------------------------------------------------------

use matrix_bot_sdk::helpers::validate_space_order_string;

#[test]
fn space_order_string_valid_simple() {
    assert!(validate_space_order_string("abc"));
}

#[test]
fn space_order_string_valid_max_length() {
    let s = "a".repeat(50);
    assert!(validate_space_order_string(&s));
}

#[test]
fn space_order_string_valid_printable_ascii() {
    assert!(validate_space_order_string("A1 !@#$"));
}

#[test]
fn space_order_string_invalid_empty() {
    assert!(!validate_space_order_string(""));
}

#[test]
fn space_order_string_invalid_too_long() {
    let s = "a".repeat(51);
    assert!(!validate_space_order_string(&s));
}

#[test]
fn space_order_string_invalid_non_ascii() {
    assert!(!validate_space_order_string("café"));
}

#[test]
fn space_order_string_invalid_newline() {
    assert!(!validate_space_order_string("abc\ndef"));
}

#[test]
fn space_order_string_invalid_tab() {
    assert!(!validate_space_order_string("abc\tdef"));
}
