use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
use matrix_bot_sdk::models::events::EventKind;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn send_message_hits_matrix_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "event_id": "$ok"
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    client
        .send_message("!room:example.org", "hello")
        .await
        .expect("send succeeds");
}

#[tokio::test]
async fn sync_once_parses_timeline_events() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/_matrix/client/v3/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "next_batch": "s123",
            "rooms": {
                "join": {
                    "!room:example.org": {
                        "timeline": {
                            "events": [
                                {
                                    "event_id": "$e1",
                                    "type": "m.room.message",
                                    "sender": "@alice:example.org",
                                    "origin_server_ts": 1,
                                    "content": {
                                        "msgtype": "m.text",
                                        "body": "hello"
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    let events = client.sync_once(None).await.expect("sync succeeds");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.kind, EventKind::RoomMessage);
}

#[tokio::test]
async fn get_openid_token_uses_real_user_id_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/_matrix/client/v3/account/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": "@bot:example.org"
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(
            r"^/_matrix/client/v3/user/.*/openid/request_token$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "oidc-token"
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    let token = client
        .get_openid_token()
        .await
        .expect("openid request should succeed");
    assert_eq!(token["access_token"], "oidc-token");
}

#[tokio::test]
async fn room_members_by_membership_sends_query_params() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(query_param("membership", "join"))
        .and(query_param("at", "s123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "chunk": [{
                "state_key": "@alice:example.org",
                "content": {
                    "membership": "join"
                }
            }]
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    let members = client
        .get_room_members_by_membership(
            "!room:example.org",
            matrix_bot_sdk::models::events::Membership::Join,
            Some("s123"),
        )
        .await
        .expect("members request should succeed");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].user_id, "@alice:example.org");
}

#[tokio::test]
async fn get_key_backup_version_returns_none_for_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/_matrix/client/v3/room_keys/version"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "errcode": "M_NOT_FOUND",
            "error": "No backup found"
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    let result = client
        .get_key_backup_version()
        .await
        .expect("key backup check should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn set_user_power_level_updates_power_levels_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/_matrix/client/v3/rooms/.*/state/.*/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "users": {
                "@alice:example.org": 50
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path_regex(r"^/_matrix/client/v3/rooms/.*/state/.*/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "event_id": "$pl"
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );
    client
        .set_user_power_level("@bob:example.org", "!room:example.org", 75)
        .await
        .expect("set power level should succeed");
}
