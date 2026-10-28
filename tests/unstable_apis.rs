use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
use serde_json::json;
use url::Url;
use wiremock::matchers::{body_json, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn unstable_get_room_aliases() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "aliases": ["#room:example.org"]
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    let aliases = client
        .unstable_apis()
        .get_room_aliases("!room:example.org")
        .await
        .expect("aliases request should succeed");
    assert_eq!(aliases, vec!["#room:example.org".to_owned()]);
}

#[tokio::test]
async fn unstable_add_reaction_to_event_sends_reaction_event() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(body_json(json!({
            "m.relates_to": {
                "event_id": "$event",
                "key": "👍",
                "rel_type": "m.annotation"
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "event_id": "$reaction"
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    let event_id = client
        .unstable_apis()
        .add_reaction_to_event("!room:example.org", "$event", "👍")
        .await
        .expect("reaction send should succeed");
    assert_eq!(event_id, "$reaction");
}

#[tokio::test]
async fn unstable_get_media_info() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content_type": "image/png",
            "width": 100,
            "height": 200,
            "size": 42,
            "thumbnails": [{
                "width": 10,
                "height": 20,
                "ready": true
            }]
        })))
        .mount(&server)
        .await;

    let client = MatrixClient::new(
        Url::parse(&server.uri()).expect("mock server url"),
        MatrixAuth::new("token"),
    );

    let info = client
        .unstable_apis()
        .get_media_info("mxc://example.org/media123")
        .await
        .expect("media info request should succeed");
    assert_eq!(info.content_type, "image/png");
    assert_eq!(info.size, 42);
    assert_eq!(info.thumbnails.len(), 1);
}
