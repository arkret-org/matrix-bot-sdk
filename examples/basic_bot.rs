use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let homeserver = std::env::var("MATRIX_HOMESERVER")
        .unwrap_or_else(|_| "https://matrix.example.org".to_owned());
    let token = std::env::var("MATRIX_ACCESS_TOKEN").unwrap_or_else(|_| "REPLACE_ME".to_owned());
    let room_id =
        std::env::var("MATRIX_ROOM_ID").unwrap_or_else(|_| "!room:example.org".to_owned());

    let client = MatrixClient::new(Url::parse(&homeserver)?, MatrixAuth::new(token));
    client
        .send_message(&room_id, "Hello from matrix-bot-sdk example")
        .await?;
    Ok(())
}
