use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use matrix_bot_sdk::appservice::{Appservice, AppserviceHandler};
use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
use salvo::Service;
use salvo::http::StatusCode;
use salvo::test::{ResponseExt, TestClient};
use serde_json::{Value, json};
use tokio::time::sleep;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct UserQueryHandler;

#[async_trait]
impl AppserviceHandler for UserQueryHandler {
    async fn query_user(&self, _user_id: &str) -> anyhow::Result<Option<Value>> {
        Ok(Some(json!({
            "display_name": "Alice",
            "avatar_mxc": "mxc://example.org/avatar"
        })))
    }
}

struct RoomQueryHandler;

#[async_trait]
impl AppserviceHandler for RoomQueryHandler {
    async fn query_room_alias(&self, _room_alias: &str) -> anyhow::Result<Option<Value>> {
        Ok(Some(json!({
            "name": "Bridge Room"
        })))
    }
}

struct ThirdpartyQueryHandler;

#[async_trait]
impl AppserviceHandler for ThirdpartyQueryHandler {
    async fn thirdparty_user_remote(
        &self,
        protocol: &str,
        fields: &std::collections::HashMap<String, String>,
    ) -> anyhow::Result<Vec<Value>> {
        Ok(vec![json!({
            "protocol": protocol,
            "fields": fields,
            "userid": "@bridge_alice:example.org"
        })])
    }
}

struct KeyClaimHandler;

#[async_trait]
impl AppserviceHandler for KeyClaimHandler {
    async fn query_key_claim(&self, _body: &Value) -> anyhow::Result<Option<Value>> {
        Ok(Some(json!({
            "one_time_keys": {
                "@alice:example.org": {
                    "DEVICE": {
                        "signed_curve25519:test": {
                            "key": "abc"
                        }
                    }
                }
            }
        })))
    }
}

struct SlowTransactionHandler {
    counter: Arc<AtomicUsize>,
}

#[async_trait]
impl AppserviceHandler for SlowTransactionHandler {
    async fn on_transaction(&self, _txn_id: &str, _body: &Value) -> anyhow::Result<()> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(80)).await;
        Ok(())
    }
}

fn appservice_with(
    homeserver: &str,
    handler: Option<Arc<dyn AppserviceHandler>>,
    protocols: Vec<&str>,
) -> Appservice {
    let client = MatrixClient::new(
        Url::parse(homeserver).expect("homeserver url"),
        MatrixAuth::new("as_token"),
    );
    let mut appservice = Appservice::new("hs_token", "as_token", client)
        .with_appservice_id("bridge_appservice")
        .with_protocols(protocols);
    if let Some(handler) = handler {
        appservice = appservice.with_handler(handler);
    }
    appservice
}

#[tokio::test]
async fn user_route_requires_auth() {
    let appservice = appservice_with("http://localhost", Some(Arc::new(UserQueryHandler)), vec![]);
    let service = Service::new(appservice.router());

    let mut res = TestClient::get("http://local/_matrix/app/v1/users/%40alice%3Aexample.org")
        .send(&service)
        .await;

    assert_eq!(res.status_code, Some(StatusCode::UNAUTHORIZED));
    let body: Value = res.take_json().await.expect("json body");
    assert_eq!(body["errcode"], "AUTH_FAILED");
}

#[tokio::test]
async fn legacy_and_v1_user_routes_work() {
    let appservice = appservice_with("http://localhost", Some(Arc::new(UserQueryHandler)), vec![]);
    let service = Service::new(appservice.router());

    let res_v1 = TestClient::get("http://local/_matrix/app/v1/users/%40alice%3Aexample.org")
        .query("access_token", "hs_token")
        .send(&service)
        .await;
    assert_eq!(res_v1.status_code, Some(StatusCode::OK));

    let mut res_legacy = TestClient::get("http://local/users/%40alice%3Aexample.org")
        .query("access_token", "hs_token")
        .send(&service)
        .await;
    assert_eq!(res_legacy.status_code, Some(StatusCode::OK));

    let body: Value = res_legacy.take_json().await.expect("json body");
    assert_eq!(body["display_name"], "Alice");
}

#[tokio::test]
async fn room_alias_route_creates_room_and_returns_room_id() {
    let homeserver = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/_matrix/client/v3/createRoom"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "room_id": "!created:example.org"
        })))
        .mount(&homeserver)
        .await;

    let appservice = appservice_with(&homeserver.uri(), Some(Arc::new(RoomQueryHandler)), vec![]);
    let service = Service::new(appservice.router());

    let mut res = TestClient::get("http://local/_matrix/app/v1/rooms/%23bridge%3Aexample.org")
        .query("access_token", "hs_token")
        .send(&service)
        .await;

    assert_eq!(res.status_code, Some(StatusCode::OK));
    let body: Value = res.take_json().await.expect("json body");
    assert_eq!(body["room_alias_name"], "bridge");
    assert_eq!(body["__roomId"], "!created:example.org");
}

#[tokio::test]
async fn transactions_require_events_array() {
    let appservice = appservice_with("http://localhost", None, vec![]);
    let service = Service::new(appservice.router());

    let mut res = TestClient::put("http://local/_matrix/app/v1/transactions/txn_1")
        .query("access_token", "hs_token")
        .json(&json!({"hello": "world"}))
        .send(&service)
        .await;

    assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));
    let body: Value = res.take_json().await.expect("json body");
    assert_eq!(body["errcode"], "BAD_REQUEST");
}

#[tokio::test]
async fn transactions_are_deduplicated_while_pending() {
    let counter = Arc::new(AtomicUsize::new(0));
    let appservice = appservice_with(
        "http://localhost",
        Some(Arc::new(SlowTransactionHandler {
            counter: Arc::clone(&counter),
        })),
        vec![],
    );
    let service = Service::new(appservice.router());

    let req1 = TestClient::put("http://local/_matrix/app/v1/transactions/txn_dup")
        .query("access_token", "hs_token")
        .json(&json!({"events": []}))
        .send(&service);
    let req2 = TestClient::put("http://local/_matrix/app/v1/transactions/txn_dup")
        .query("access_token", "hs_token")
        .json(&json!({"events": []}))
        .send(&service);

    let (res1, res2) = tokio::join!(req1, req2);
    assert_eq!(res1.status_code, Some(StatusCode::OK));
    assert_eq!(res2.status_code, Some(StatusCode::OK));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn thirdparty_routes_handle_protocol_and_mappings() {
    let appservice = appservice_with(
        "http://localhost",
        Some(Arc::new(ThirdpartyQueryHandler)),
        vec!["irc"],
    );
    let service = Service::new(appservice.router());

    let mut handled = TestClient::get("http://local/_matrix/app/v1/thirdparty/user/irc")
        .query("access_token", "hs_token")
        .query("nick", "alice")
        .send(&service)
        .await;
    assert_eq!(handled.status_code, Some(StatusCode::OK));

    let list: Value = handled.take_json().await.expect("json body");
    assert!(list.as_array().expect("array").len() == 1);

    let mut not_handled = TestClient::get("http://local/_matrix/app/v1/thirdparty/user/xmpp")
        .query("access_token", "hs_token")
        .send(&service)
        .await;
    assert_eq!(not_handled.status_code, Some(StatusCode::NOT_FOUND));
    let err: Value = not_handled.take_json().await.expect("json body");
    assert_eq!(err["errcode"], "PROTOCOL_NOT_HANDLED");
}

#[tokio::test]
async fn keys_claim_and_ping_routes_behave_like_ts() {
    let appservice = appservice_with("http://localhost", Some(Arc::new(KeyClaimHandler)), vec![]);
    let service = Service::new(appservice.router());

    let key_claim =
        TestClient::post("http://local/_matrix/app/unstable/org.matrix.msc3983/keys/claim")
            .query("access_token", "hs_token")
            .json(&json!({"one_time_keys": {}}))
            .send(&service)
            .await;
    assert_eq!(key_claim.status_code, Some(StatusCode::OK));

    appservice.expect_ping_transaction("tx_ping").await;
    let ping = TestClient::post("http://local/_matrix/app/v1/ping")
        .query("access_token", "hs_token")
        .json(&json!({"transaction_id": "tx_ping"}))
        .send(&service)
        .await;
    assert_eq!(ping.status_code, Some(StatusCode::OK));
    assert!(appservice.expected_ping_transaction().await.is_none());

    let mut unknown = TestClient::get("http://local/_matrix/app/v1/not-implemented")
        .send(&service)
        .await;
    assert_eq!(unknown.status_code, Some(StatusCode::NOT_FOUND));
    let body: Value = unknown.take_json().await.expect("json body");
    assert_eq!(body["errcode"], "M_UNRECOGNIZED");
}
