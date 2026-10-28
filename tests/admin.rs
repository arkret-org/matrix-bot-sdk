use matrix_bot_sdk::{
    admin::{AdminApis, SynapseAdminApis, SynapseRoomProperty, SynapseUserProperties},
    client::{MatrixAuth, MatrixClient}, /* We need MatrixAuth since the client is configured
                                        * with it */
};
use mockito::{Matcher, Server};
use serde_json::json;

async fn setup_admin_client(server_url: reqwest::Url) -> AdminApis {
    let client = MatrixClient::new(server_url, MatrixAuth::new("AccessToken"));
    AdminApis::new(client)
}

async fn setup_synapse_client(server_url: reqwest::Url) -> SynapseAdminApis {
    let client = MatrixClient::new(server_url, MatrixAuth::new("AccessToken"));
    SynapseAdminApis::new(client)
}

#[tokio::test]
async fn whois_user() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_admin_client(url).await;

    let user_id = "@someone:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(user_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    let expected_response = json!({
        "user_id": user_id,
        "devices": {
            "foobar": {
                "sessions": [{
                    "connections": [{
                        "ip": "127.0.0.1",
                        "last_seen": 1000,
                        "user_agent": "FakeDevice/1.0.0",
                    }]
                }]
            }
        }
    });

    let mock = server
        .mock(
            "GET",
            format!("/_matrix/client/v3/admin/whois/{}", encoded).as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let result = client
        .whois_user(user_id)
        .await
        .expect("Failed to get whois");
    assert_eq!(result.user_id, user_id);
    assert!(result.devices.contains_key("foobar"));

    mock.assert_async().await;
}

#[tokio::test]
async fn is_admin() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let user_id = "@someone:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(user_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    let expected_response = json!({ "admin": true });

    let mock = server
        .mock(
            "GET",
            format!("/_synapse/admin/v1/users/{}/admin", encoded).as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let is_admin = client
        .is_admin(user_id)
        .await
        .expect("Failed to check admin");
    assert!(is_admin);

    mock.assert_async().await;
}

#[tokio::test]
async fn get_user() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let user_id = "@someone:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(user_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    let expected_response = json!({
        "displayname": "foobar",
        "avatar_url": "mxc://example.org/animage",
        "admin": true,
        "deactivated": false,
    });

    let mock = server
        .mock(
            "GET",
            format!("/_synapse/admin/v2/users/{}", encoded).as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let user = client
        .get_user(user_id)
        .await
        .expect("Failed to fetch user");
    assert_eq!(user.displayname.unwrap(), "foobar");
    assert!(user.admin.unwrap());

    mock.assert_async().await;
}

#[tokio::test]
async fn upsert_user() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let user_id = "@someone:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(user_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    let request_props = SynapseUserProperties {
        displayname: Some("foobar".to_string()),
        avatar_url: Some("mxc://example.org/animage".to_string()),
        admin: Some(true),
        deactivated: Some(false),
        locked: None,
        password: Some("foobar".to_string()),
        logout_devices: None,
    };

    let expected_response = json!({
        "displayname": "foobar",
        "avatar_url": "mxc://example.org/animage",
        "admin": true,
        "deactivated": false,
    });

    let mock = server
        .mock(
            "PUT",
            format!("/_synapse/admin/v2/users/{}", encoded).as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(Matcher::Json(serde_json::to_value(&request_props).unwrap()))
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let user = client
        .upsert_user(user_id, &request_props)
        .await
        .expect("Failed to upsert user");
    assert_eq!(user.displayname.unwrap(), "foobar");
    assert!(user.admin.unwrap());

    mock.assert_async().await;
}

#[tokio::test]
async fn make_room_admin() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let room_id = "!room:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(room_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
    let user_id = "@alice:example.org";

    let mock = server
        .mock(
            "POST",
            format!("/_synapse/admin/v1/rooms/{}/make_room_admin", encoded).as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(mockito::Matcher::Json(json!({ "user_id": user_id })))
        .with_body("{}")
        .create_async()
        .await;

    client
        .make_room_admin(room_id, Some(user_id))
        .await
        .expect("Failed to make room admin");
    mock.assert_async().await;
}

#[tokio::test]
async fn delete_room() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let room_id = "!room:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(room_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    let expected_body = json!({
        "purge": true,
        "force_purge": false,
        "block": false,
    });

    let mock = server
        .mock(
            "POST",
            format!("/_synapse/admin/v2/rooms/{}/delete", encoded).as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(Matcher::Json(expected_body))
        .with_body(json!({ "delete_id": "delete_id_123" }).to_string())
        .create_async()
        .await;

    let delete_id = client
        .delete_room(room_id, true, false, false)
        .await
        .expect("Failed to delete room");
    assert_eq!(delete_id, "delete_id_123");

    mock.assert_async().await;
}

#[tokio::test]
async fn get_event_nearest_to_timestamp() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let room_id = "!abc123:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(room_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
    let ts = 1234;
    let dir = "f";

    let expected_response = json!({
        "event_id": "$def456:example.org",
        "origin_server_ts": 4567,
    });

    let mock = server
        .mock(
            "GET",
            format!(
                "/_synapse/admin/v1/rooms/{}/timestamp_to_event?ts={}&dir={}",
                encoded, ts, dir
            )
            .as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let result = client
        .get_event_nearest_to_timestamp(room_id, ts, dir)
        .await
        .expect("Failed to get nearest event");
    assert_eq!(
        result.get("event_id").unwrap().as_str().unwrap(),
        "$def456:example.org"
    );
    assert_eq!(
        result.get("origin_server_ts").unwrap().as_u64().unwrap(),
        4567
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn list_users() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let mock = server
        .mock(
            "GET",
            "/_synapse/admin/v2/users?guests=true&deactivated=false&from=abc&limit=20&name=alice",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "users": [{
                    "name": "@alice:example.org"
                }],
                "next_token": "next",
                "total": 1
            })
            .to_string(),
        )
        .create_async()
        .await;

    let response = client
        .list_users(Some("abc"), Some(20), Some("alice"), true, false)
        .await
        .expect("Failed to list users");
    assert_eq!(response.users.len(), 1);
    assert_eq!(response.users[0].name, "@alice:example.org");

    mock.assert_async().await;
}

#[tokio::test]
async fn list_rooms() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let mock = server
        .mock("GET", "/_synapse/admin/v1/rooms")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("from".to_owned(), "1".to_owned()),
            Matcher::UrlEncoded("limit".to_owned(), "10".to_owned()),
            Matcher::UrlEncoded("search_term".to_owned(), "bridge".to_owned()),
            Matcher::UrlEncoded("order_by".to_owned(), "joined_members".to_owned()),
            Matcher::UrlEncoded("dir".to_owned(), "b".to_owned()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "rooms": [{
                    "room_id": "!room:example.org"
                }],
                "total_rooms": 1,
                "next_batch": "2"
            })
            .to_string(),
        )
        .create_async()
        .await;

    let response = client
        .list_rooms(
            Some("bridge"),
            Some("1"),
            Some(10),
            Some(SynapseRoomProperty::JoinedMembers),
            true,
        )
        .await
        .expect("Failed to list rooms");
    assert_eq!(response.rooms.len(), 1);
    assert_eq!(response.rooms[0].room_id, "!room:example.org");

    mock.assert_async().await;
}

#[tokio::test]
async fn get_delete_room_state() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_synapse_client(url).await;

    let room_id = "!room:example.org";
    let encoded =
        percent_encoding::utf8_percent_encode(room_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    let mock = server
        .mock(
            "GET",
            format!("/_synapse/admin/v2/rooms/{}/delete_status", encoded).as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "results": [{
                    "delete_id": "del_1"
                }]
            })
            .to_string(),
        )
        .create_async()
        .await;

    let response = client
        .get_delete_room_state(room_id)
        .await
        .expect("Failed to get room deletion state");
    assert_eq!(response.len(), 1);
    assert_eq!(response[0]["delete_id"], "del_1");

    mock.assert_async().await;
}
