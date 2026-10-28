//! Matrix Bot SDK for Rust.
//!
//! ```no_run
//! use matrix_bot_sdk::client::{MatrixAuth, MatrixClient};
//! use url::Url;
//!
//! # async fn run() -> anyhow::Result<()> {
//! let client = MatrixClient::new(
//!     Url::parse("https://matrix.example.com")?,
//!     MatrixAuth::new("ACCESS_TOKEN"),
//! );
//! client
//!     .send_message("!room:example.com", "hello from rust")
//!     .await?;
//! # Ok(())
//! # }
//! ```

pub mod admin;
pub mod appservice;
pub mod client;
pub mod clients;
pub mod content_scanner;
pub mod e2ee;
pub mod helpers;
pub mod http;
pub mod identity;
pub mod logging;
pub mod metrics;
pub mod mixins;
pub mod models;
pub mod pantalaimon;
pub mod preprocessors;
pub mod storage;
pub mod strategies;

pub use client::{DMs, IFilter, MatrixAuth, MatrixClient, SynchronousMatrixClient};
