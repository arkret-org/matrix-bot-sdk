use matrix_bot_sdk::preprocessors::{IPreprocessor, RichRepliesPreprocessor};
use serde_json::json;

#[tokio::test]
async fn rich_replies_preprocessor_can_be_created() {
    let _preprocessor = RichRepliesPreprocessor::new();
}

#[tokio::test]
async fn rich_replies_preprocessor_parses_single_line_event() {
    let preprocessor = RichRepliesPreprocessor::new();

    let original_event_id = "$original:example.org";
    let original_user_id = "@alice:example.org";
    let original_plain_text = "Hello world, this is text";
    let original_html = "<b>Hello world, this is text</b>";
    let original_room_id = "!somewhere:example.org";
    let reply_plain_text = "This is the reply";
    let reply_html = "<i>This is the reply text</i>";

    let mut event = json!({
        "content": {
            "m.relates_to": {
                "m.in_reply_to": {
                    "event_id": original_event_id
                }
            },
            "format": "org.matrix.custom.html",
            "body": format!("> <{}> {}\n\n{}", original_user_id, original_plain_text, reply_plain_text),
            "formatted_body": format!(
                "<mx-reply><blockquote><a href=\"https://matrix.to/#/{}/{}\">In reply to</a> <a href=\"https://matrix.to/#/{}\">{}</a><br />{}</blockquote></mx-reply>{}",
                original_room_id, original_event_id, original_user_id, original_user_id, original_html, reply_html
            )
        }
    });

    preprocessor
        .process(&mut event)
        .await
        .expect("process should work");

    let content = event.get("content").unwrap();
    assert_eq!(
        content.get("body").and_then(|v| v.as_str()),
        Some(reply_plain_text)
    );
    assert_eq!(
        content.get("formatted_body").and_then(|v| v.as_str()),
        Some(reply_html)
    );

    let reply_info = event.get("mx_richreply").unwrap();
    assert_eq!(
        reply_info.get("parent_event_id").and_then(|v| v.as_str()),
        Some(original_event_id)
    );
    assert_eq!(
        reply_info.get("fallback_sender").and_then(|v| v.as_str()),
        Some(original_user_id)
    );
    assert!(
        !reply_info
            .get("was_lenient")
            .and_then(|v| v.as_bool())
            .unwrap()
    );
}

#[tokio::test]
async fn rich_replies_preprocessor_parses_multiline_event() {
    let preprocessor = RichRepliesPreprocessor::new();

    let original_event_id = "$original:example.org";
    let original_user_id = "@alice:example.org";
    let _original_plain_text = "Hello world\nTesting line 2";
    let original_html = "<b>Hello world</b><br /><p>Testing line 2</p>";
    let original_room_id = "!somewhere:example.org";
    let reply_plain_text = "This is the reply\nWith two lines";
    let reply_html = "<i>This is the reply text</i><br /><p>With two lines</p>";

    let mut event = json!({
        "content": {
            "m.relates_to": {
                "m.in_reply_to": {
                    "event_id": original_event_id
                }
            },
            "format": "org.matrix.custom.html",
            "body": format!("> <{}> {}\n> {}\n\n{}", original_user_id, "Hello world", "Testing line 2", reply_plain_text),
            "formatted_body": format!(
                "<mx-reply><blockquote><a href=\"https://matrix.to/#/{}/{}\">In reply to</a> <a href=\"https://matrix.to/#/{}\">{}</a><br />{}</blockquote></mx-reply>{}",
                original_room_id, original_event_id, original_user_id, original_user_id, original_html, reply_html
            )
        }
    });

    preprocessor
        .process(&mut event)
        .await
        .expect("process should work");

    let reply_info = event.get("mx_richreply").unwrap();
    assert_eq!(
        reply_info.get("parent_event_id").and_then(|v| v.as_str()),
        Some(original_event_id)
    );
    assert_eq!(
        reply_info.get("fallback_sender").and_then(|v| v.as_str()),
        Some(original_user_id)
    );
    assert!(
        reply_info
            .get("fallback_plain_body")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("Hello world")
    );
}

#[tokio::test]
async fn rich_replies_preprocessor_ignores_non_reply() {
    let preprocessor = RichRepliesPreprocessor::new();

    let mut event = json!({
        "content": {
            "body": "This is a regular message"
        }
    });

    preprocessor
        .process(&mut event)
        .await
        .expect("process should work");

    assert!(event.get("mx_richreply").is_none());
}

#[tokio::test]
async fn rich_replies_preprocessor_handles_missing_content() {
    let preprocessor = RichRepliesPreprocessor::new();

    let mut event = json!({
        "event_id": "$event"
    });

    preprocessor
        .process(&mut event)
        .await
        .expect("process should work");
}

#[tokio::test]
async fn rich_replies_preprocessor_handles_missing_body() {
    let preprocessor = RichRepliesPreprocessor::new();

    let mut event = json!({
        "content": {
            "m.relates_to": {
                "m.in_reply_to": {
                    "event_id": "$original"
                }
            },
            "msgtype": "m.text"
        }
    });

    preprocessor
        .process(&mut event)
        .await
        .expect("process should work");
}

#[tokio::test]
async fn rich_replies_preprocessor_msc3676_reply() {
    let preprocessor = RichRepliesPreprocessor::new();

    let mut event = json!({
        "content": {
            "m.relates_to": {
                "m.in_reply_to": {
                    "event_id": "$original"
                }
            },
            "body": "image.png",
            "msgtype": "m.image",
            "url": "mxc://example.org/irrelevant"
        }
    });

    preprocessor
        .process(&mut event)
        .await
        .expect("process should work");

    let reply_info = event.get("mx_richreply").unwrap();
    assert!(
        reply_info
            .get("was_lenient")
            .and_then(|v| v.as_bool())
            .unwrap()
    );
    assert_eq!(
        reply_info.get("parent_event_id").and_then(|v| v.as_str()),
        Some("$original")
    );
    assert_eq!(
        reply_info
            .get("fallback_plain_body")
            .and_then(|v| v.as_str()),
        Some("")
    );
}

#[tokio::test]
async fn rich_replies_preprocessor_supported_event_types() {
    let preprocessor = RichRepliesPreprocessor::new();
    let types = preprocessor.get_supported_event_types();
    assert_eq!(types.len(), 1);
    assert!(types.contains(&"m.room.message"));
}
