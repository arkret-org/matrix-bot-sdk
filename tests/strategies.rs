use std::time::Duration;

use matrix_bot_sdk::strategies::{AppserviceJoinRoomStrategy, SimpleRetryJoinStrategy};

#[test]
fn appservice_join_room_strategy_can_be_created() {
    let _strategy = AppserviceJoinRoomStrategy::new();
}

#[test]
fn appservice_join_room_strategy_default() {
    let _strategy = AppserviceJoinRoomStrategy;
}

#[test]
fn simple_retry_join_strategy_can_be_created() {
    let _strategy = SimpleRetryJoinStrategy::new();
}

#[test]
fn simple_retry_join_strategy_default() {
    let strategy = SimpleRetryJoinStrategy::default();
    assert!(!strategy.clone().schedule.is_empty());
}

#[test]
fn simple_retry_join_strategy_with_custom_schedule() {
    let strategy = SimpleRetryJoinStrategy::with_schedule(vec![
        Duration::from_millis(0),
        Duration::from_millis(10),
        Duration::from_millis(20),
    ]);
    let cloned = strategy.clone();
    assert_eq!(cloned.schedule.len(), 3);
}

#[tokio::test]
async fn simple_retry_join_strategy_succeeds_on_first_try() {
    let strategy = SimpleRetryJoinStrategy::with_schedule(vec![
        Duration::from_millis(0),
        Duration::from_millis(10),
    ]);

    let result = strategy
        .retry(|| async { Ok("!void:example.org".to_owned()) })
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn simple_retry_join_strategy_retries_on_failure() {
    let strategy = SimpleRetryJoinStrategy::with_schedule(vec![
        Duration::from_millis(0),
        Duration::from_millis(10),
        Duration::from_millis(20),
    ]);

    let mut attempts = 0;
    let result = strategy
        .retry(|| {
            attempts += 1;
            let current_attempts = attempts;
            async move {
                if current_attempts < 3 {
                    Err(anyhow::anyhow!("Simulated failure"))
                } else {
                    Ok("!void:example.org".to_owned())
                }
            }
        })
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn simple_retry_join_strategy_fails_after_all_attempts() {
    let strategy = SimpleRetryJoinStrategy::with_schedule(vec![
        Duration::from_millis(0),
        Duration::from_millis(10),
    ]);

    let result = strategy
        .retry(|| async { Err(anyhow::anyhow!("Simulated failure")) })
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Simulated failure")
    );
}
