use std::sync::Arc;

use async_trait::async_trait;
use matrix_bot_sdk::appservice::{Appservice, AppserviceHandler};
use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
use matrix_bot_sdk::preprocessors::IPreprocessor;
use salvo::Service;
use salvo::http::StatusCode;
use salvo::test::{ResponseExt, TestClient};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use url::Url;
use wiremock::matchers::{body_json, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct CaptureHandler {
    captured: Arc<Mutex<Option<Value>>>,
}

#[async_trait]
impl AppserviceHandler for CaptureHandler {
    async fn on_transaction(&self, _txn_id: &str, body: &Value) -> anyhow::Result<()> {
        *self.captured.lock().await = Some(body.clone());
        Ok(())
    }
}

struct MarkPreprocessor;

#[async_trait]
impl IPreprocessor for MarkPreprocessor {
    async fn process(&self, event: &mut Value) -> anyhow::Result<()> {
        if let Some(obj) = event.as_object_mut() {
            obj.insert("preprocessed".to_owned(), Value::Bool(true));
        }
        Ok(())
    }
}

fn test_appservice(homeserver: &str) -> Appservice {
    let client = MatrixClient::new(
        Url::parse(homeserver).expect("valid homeserver url"),
        MatrixAuth::new("as_token"),
    );

    Appservice::new("hs_token", "as_token", client)
        .with_homeserver_name("example.org")
        .with_user_prefix("@bridge_")
        .with_alias_prefix("#bridge_")
}

#[test]
fn namespace_helpers_follow_ts_style_rules() {
    let appservice = test_appservice("http://localhost");

    assert_eq!(appservice.get_user_id("alice"), "@alice:example.org");
    assert_eq!(
        appservice
            .get_user_id_for_suffix("alice")
            .expect("suffix to user id"),
        "@bridge_alice:example.org"
    );
    assert_eq!(
        appservice
            .get_suffix_for_user_id("@bridge_alice:example.org")
            .expect("suffix parse"),
        Some("alice".to_string())
    );
    assert!(appservice.is_namespaced_user("@bridge_alice:example.org"));
    assert!(!appservice.is_namespaced_user("@alice:example.org"));

    assert_eq!(appservice.get_alias("room"), "#room:example.org");
    assert_eq!(
        appservice
            .get_alias_for_suffix("room")
            .expect("suffix to alias"),
        "#bridge_room:example.org"
    );
    assert_eq!(
        appservice
            .get_alias_localpart_for_suffix("room")
            .expect("localpart"),
        "bridge_room"
    );
    assert_eq!(
        appservice
            .get_suffix_for_alias("#bridge_room:example.org")
            .expect("alias suffix"),
        Some("room".to_string())
    );
    assert!(appservice.is_namespaced_alias("#bridge_room:example.org"));
    assert!(!appservice.is_namespaced_alias("#room:example.org"));
}

#[tokio::test]
async fn set_room_directory_visibility_hits_correct_endpoint() {
    let homeserver = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(body_json(json!({ "visibility": "public" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&homeserver)
        .await;

    let appservice = test_appservice(&homeserver.uri());
    appservice
        .set_room_directory_visibility("irc", "!room:example.org", "public")
        .await
        .expect("set visibility");

    let requests = homeserver
        .received_requests()
        .await
        .expect("captured requests");
    assert_eq!(requests.len(), 1);
    let path = requests[0].url.path().to_owned();
    assert!(
        path.ends_with("/_matrix/client/v3/directory/list/appservice/irc/%21room%3Aexample.org")
            || path.ends_with(
                "/_matrix/client/v3/directory/list/appservice/irc/%21room%3Aexample%2Eorg"
            )
            || path.ends_with("/_matrix/client/v3/directory/list/appservice/irc/!room:example.org"),
        "unexpected path: {path}"
    );
}

#[tokio::test]
async fn transaction_preprocessors_run_before_handler() {
    let captured = Arc::new(Mutex::new(None));
    let handler = Arc::new(CaptureHandler {
        captured: Arc::clone(&captured),
    });
    let appservice = test_appservice("http://localhost").with_handler(handler);
    appservice
        .add_preprocessor(Arc::new(MarkPreprocessor))
        .await;

    let service = Service::new(appservice.router());
    let mut res = TestClient::put("http://local/_matrix/app/v1/transactions/txn1")
        .query("access_token", "hs_token")
        .json(&json!({
            "events": [
                {
                    "type": "m.room.message",
                    "content": { "body": "hi", "msgtype": "m.text" }
                }
            ]
        }))
        .send(&service)
        .await;

    assert_eq!(res.status_code, Some(StatusCode::OK));
    let _: Value = res.take_json().await.expect("json body");

    let body = captured
        .lock()
        .await
        .clone()
        .expect("captured transaction body");
    assert_eq!(body["events"][0]["preprocessed"], true);
}
