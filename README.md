# matrix-bot-sdk (Rust)

Rust implementation of a Matrix Bot SDK focused on bots and application services.

## Status

This repository now includes:

- Core Matrix models and event models.
- HTTP request layer and validation utilities.
- Storage abstractions with memory/file/postgres backends.
- Logging, metrics, join strategies, preprocessors, and mixins.
- `MatrixClient` async core API + synchronous wrapper.
- Admin APIs, appservice scaffolding (Salvo), E2EE baseline, and auxiliary clients.
- Unit/integration tests and CI workflow.

## Quick Start

```rust
use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
use url::Url;

# async fn run() -> anyhow::Result<()> {
let client = MatrixClient::new(
    Url::parse("https://matrix.example.org")?,
    MatrixAuth::new("YOUR_ACCESS_TOKEN"),
);

client.send_message("!room:example.org", "hello from rust sdk").await?;
# Ok(())
# }
```

## Features

- `storage-postgres`: enable `SimplePostgresStorageProvider`.
- `storage-sqlite`: reserved for sqlite backend extension.
- `appservice`: reserved feature gate for appservice-specific runtime behaviors.
- `e2ee`: reserved feature gate for stronger crypto integrations.
- `ruma-types`: enables optional `ruma` dependency.

## Development

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Examples

Run:

```bash
cargo run --example basic_bot
```