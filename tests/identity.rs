use std::sync::Arc;

use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
use matrix_bot_sdk::identity::IdentityClient;
use matrix_bot_sdk::models::{OpenIDConnect, Threepid};
use mockito::Server;
use serde_json::json;

async fn setup_identity_client(
    server_url: reqwest::Url,
    identity_url: reqwest::Url,
) -> IdentityClient {
    let client = Arc::new(MatrixClient::new(
        server_url,
        MatrixAuth::new("AccessToken"),
    ));
    IdentityClient::new("id_access_token", identity_url, client)
}

#[tokio::test]
async fn get_account() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let expected_response = json!({
        "user_id": "@alice:example.org",
    });

    let mock = server
        .mock("GET", "/_matrix/identity/v2/account")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let result = client.get_account().await.expect("Failed to get account");
    assert_eq!(result.get("user_id").unwrap(), "@alice:example.org");

    mock.assert_async().await;
}

#[tokio::test]
async fn get_terms_of_service() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let expected_response = json!({
        "policies": {
            "test1": {
                "en": {
                    "name": "Test Policy",
                    "url": "https://terms.example.org/v1/en/test1"
                },
                "version": "1"
            }
        }
    });

    let mock = server
        .mock("GET", "/_matrix/identity/v2/terms")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let result = client
        .get_terms_of_service()
        .await
        .expect("Failed to get TOS");
    assert!(result.get("policies").unwrap().get("test1").is_some());

    mock.assert_async().await;
}

#[tokio::test]
async fn accept_terms() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let urls = vec![
        "https://terms.example.org/v1/en/test1".to_string(),
        "https://terms.example.org/v1/en/test2".to_string(),
    ];

    let mock = server
        .mock("POST", "/_matrix/identity/v2/terms")
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(mockito::Matcher::Json(json!({ "user_accepts": urls })))
        .with_body("{}")
        .create_async()
        .await;

    client
        .accept_terms(urls)
        .await
        .expect("Failed to accept TOS");

    mock.assert_async().await;
}

#[tokio::test]
async fn lookup() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let addresses = vec![
        "4kenr7N9drpCJ4AfalmlGQVsOn3o2RHjkADUpXJWZUc".to_string(),
        "nlo35_T5fzSGZzJApqu8lgIudJvmOQtDaHtr-I4rU7I".to_string(),
    ];
    let algorithm = "sha256";
    let pepper = "matrixrocks";

    let expected_response = json!({
        "mappings": {
            "4kenr7N9drpCJ4AfalmlGQVsOn3o2RHjkADUpXJWZUc": "@alice:example.org"
        }
    });

    let mock = server
        .mock("POST", "/_matrix/identity/v2/lookup")
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(mockito::Matcher::Json(json!({
            "algorithm": algorithm,
            "pepper": pepper,
            "addresses": addresses,
        })))
        .with_body(expected_response.to_string())
        .create_async()
        .await;

    let result = client
        .lookup(addresses, algorithm, pepper)
        .await
        .expect("Failed to lookup");
    assert_eq!(
        result
            .get("mappings")
            .unwrap()
            .get("4kenr7N9drpCJ4AfalmlGQVsOn3o2RHjkADUpXJWZUc")
            .unwrap(),
        "@alice:example.org"
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn make_email_invite() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let email = "alice@example.org";
    let room_id = "!room:example.org";

    // Default matrix client doesn't know its user ID out of the box unless authenticated/queried
    // We will assume "sender": "" because `get_user_id` resolves to default empty string or None
    // without storage initialized

    let expected_body = json!({
        "address": email,
        "room_id": room_id,
        "medium": "email",
        "sender": "",
    });

    let stored_invite = json!({
        "display_name": "a...@e...",
        "public_keys": [
            "serverkey",
            "ephemeralkey"
        ],
        "token": "s3cret"
    });

    let mock = server
        .mock("POST", "/_matrix/identity/v2/store-invite")
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(mockito::Matcher::Json(expected_body))
        .with_body(stored_invite.to_string())
        .create_async()
        .await;

    let result = client
        .make_email_invite(email, room_id)
        .await
        .expect("Failed to make invite");
    assert_eq!(result.get("token").unwrap(), "s3cret");

    mock.assert_async().await;
}

#[tokio::test]
async fn accept_all_terms() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let terms = json!({
        "policies": {
            "p1": {
                "en": { "url": "https://terms.example.org/en/p1" },
                "version": "1"
            },
            "p2": {
                "de": { "url": "https://terms.example.org/de/p2" },
                "version": "2"
            },
            "p3": {
                "en": { "url": "https://terms.example.org/en/p1" },
                "version": "3"
            }
        }
    });

    let get_terms = server
        .mock("GET", "/_matrix/identity/v2/terms")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(terms.to_string())
        .create_async()
        .await;

    let post_terms = server
        .mock("POST", "/_matrix/identity/v2/terms")
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(mockito::Matcher::Json(json!({
            "user_accepts": [
                "https://terms.example.org/de/p2",
                "https://terms.example.org/en/p1"
            ]
        })))
        .with_body("{}")
        .create_async()
        .await;

    client
        .accept_all_terms()
        .await
        .expect("accept_all_terms should succeed");

    get_terms.assert_async().await;
    post_terms.assert_async().await;
}

#[tokio::test]
async fn lookup_threepids_prefers_sha256() {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use sha2::{Digest, Sha256};

    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let hash_details = server
        .mock("GET", "/_matrix/identity/v2/hash_details")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "algorithms": ["none", "sha256"],
                "lookup_pepper": "matrixrocks"
            })
            .to_string(),
        )
        .create_async()
        .await;

    let pids = vec![
        Threepid {
            medium: "email".into(),
            address: "Alice@Example.org".into(),
            validated_at: None,
            added_at: None,
        },
        Threepid {
            medium: "email".into(),
            address: "bob@example.org".into(),
            validated_at: None,
            added_at: None,
        },
    ];

    let to_hash = |address: &str| {
        let input = format!("{} {} {}", address.to_lowercase(), "email", "matrixrocks");
        let mut hasher = Sha256::new();
        hasher.update(input);
        URL_SAFE_NO_PAD.encode(hasher.finalize())
    };

    let hashed_alice = to_hash("Alice@Example.org");
    let hashed_bob = to_hash("bob@example.org");

    let lookup = server
        .mock("POST", "/_matrix/identity/v2/lookup")
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(mockito::Matcher::Json(json!({
            "algorithm": "sha256",
            "pepper": "matrixrocks",
            "addresses": [hashed_alice, hashed_bob]
        })))
        .with_body(
            json!({
                "mappings": {
                    to_hash("Alice@Example.org"): "@alice:example.org"
                }
            })
            .to_string(),
        )
        .create_async()
        .await;

    let result = client
        .lookup_threepids(&pids, false)
        .await
        .expect("lookup_threepids should succeed");

    assert_eq!(
        result,
        vec![Some("@alice:example.org".to_string()), None],
        "lookup order should match input order"
    );

    hash_details.assert_async().await;
    lookup.assert_async().await;
}

#[tokio::test]
async fn lookup_threepids_plaintext_fallback() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();
    let client = setup_identity_client(url.clone(), url.clone()).await;

    let hash_details = server
        .mock("GET", "/_matrix/identity/v2/hash_details")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "algorithms": ["none"],
                "lookup_pepper": "ignored"
            })
            .to_string(),
        )
        .create_async()
        .await;

    let pids = vec![Threepid {
        medium: "email".into(),
        address: "Alice@Example.org".into(),
        validated_at: None,
        added_at: None,
    }];

    let lookup = server
        .mock("POST", "/_matrix/identity/v2/lookup")
        .with_status(200)
        .with_header("content-type", "application/json")
        .match_body(mockito::Matcher::Json(json!({
            "algorithm": "none",
            "pepper": "ignored",
            "addresses": ["alice@example.org email"]
        })))
        .with_body(
            json!({
                "mappings": {
                    "alice@example.org email": "@alice:example.org"
                }
            })
            .to_string(),
        )
        .create_async()
        .await;

    let result = client
        .lookup_threepids(&pids, true)
        .await
        .expect("lookup_threepids plaintext fallback should succeed");

    assert_eq!(result, vec![Some("@alice:example.org".to_string())]);
    hash_details.assert_async().await;
    lookup.assert_async().await;
}

#[tokio::test]
async fn acquire_identity_client() {
    let mut server = Server::new_async().await;
    let url = reqwest::Url::parse(&server.url()).unwrap();

    let register = server
        .mock("POST", "/_matrix/identity/v2/account/register")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({ "token": "id_access_token_from_register" }).to_string())
        .create_async()
        .await;

    let mx_client = Arc::new(MatrixClient::new(
        url.clone(),
        MatrixAuth::new("mx_access_token"),
    ));
    let oidc = OpenIDConnect {
        access_token: "oidc_token".into(),
        token_type: "Bearer".into(),
        matrix_server_name: "example.org".into(),
        expires_in: 3600,
    };

    let acquired = IdentityClient::acquire(&oidc, url.clone(), mx_client)
        .await
        .expect("acquire should succeed");

    assert_eq!(acquired.access_token, "id_access_token_from_register");
    assert_eq!(acquired.server_url, url);
    register.assert_async().await;
}
