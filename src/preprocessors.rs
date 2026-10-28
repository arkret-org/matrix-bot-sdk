use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait IPreprocessor: Send + Sync {
    async fn process(&self, event: &mut Value) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichReplyInfo {
    pub was_lenient: bool,
    pub parent_event_id: String,
    pub fallback_plain_body: String,
    pub fallback_html_body: String,
    pub fallback_sender: String,
    pub real_event: Option<Value>,
}

#[derive(Default)]
pub struct RichRepliesPreprocessor {
    _fetch_real_event: bool,
}

impl RichRepliesPreprocessor {
    pub fn new() -> Self {
        Self {
            _fetch_real_event: false,
        }
    }

    pub fn with_fetch_real_event(fetch: bool) -> Self {
        Self {
            _fetch_real_event: fetch,
        }
    }

    pub fn get_supported_event_types(&self) -> Vec<&'static str> {
        vec!["m.room.message"]
    }
}

#[async_trait]
impl IPreprocessor for RichRepliesPreprocessor {
    async fn process(&self, event: &mut Value) -> anyhow::Result<()> {
        let Some(content) = event.get_mut("content") else {
            return Ok(());
        };

        let relates_to = content
            .get("m.relates_to")
            .and_then(|r| r.get("m.in_reply_to"))
            .and_then(|r| r.get("event_id"))
            .and_then(Value::as_str);

        let Some(parent_event_id) = relates_to else {
            return Ok(());
        };
        let parent_event_id = parent_event_id.to_owned();

        let body = content
            .get("body")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_default();

        let formatted_body = content
            .get("formatted_body")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        let (fallback_plain_body, fallback_sender, reply_plain_body) =
            Self::parse_plain_body(&body);

        let (fallback_html_body, reply_html_body) =
            Self::parse_html_body(&formatted_body.unwrap_or_default());

        let was_lenient = fallback_plain_body.is_empty() && fallback_sender.is_empty();

        if !was_lenient && let Some(obj) = content.as_object_mut() {
            obj.insert("body".to_owned(), Value::String(reply_plain_body));
            if let Some(html) = reply_html_body {
                obj.insert(
                    "format".to_owned(),
                    Value::String("org.matrix.custom.html".to_owned()),
                );
                obj.insert("formatted_body".to_owned(), Value::String(html));
            }
        }

        let info = RichReplyInfo {
            was_lenient,
            parent_event_id,
            fallback_plain_body,
            fallback_html_body,
            fallback_sender,
            real_event: None,
        };

        if let Some(obj) = event.as_object_mut() {
            obj.insert("mx_richreply".to_owned(), serde_json::to_value(info)?);
        }

        Ok(())
    }
}

impl RichRepliesPreprocessor {
    fn parse_plain_body(body: &str) -> (String, String, String) {
        if !body.starts_with("> <") {
            return (String::new(), String::new(), body.to_owned());
        }

        let mut lines = body.lines();
        let first_line = lines.next().unwrap_or("");

        let sender_start = 3;
        let sender_end = first_line[sender_start..]
            .find('>')
            .map(|pos| sender_start + pos)
            .unwrap_or(first_line.len());

        let sender = first_line[sender_start..sender_end].to_owned();

        let mut fallback_lines = vec![first_line[sender_end + 2..].to_owned()];
        let mut reply_lines = Vec::new();
        let mut in_fallback = true;

        for line in lines {
            if in_fallback {
                if let Some(stripped) = line.strip_prefix("> ") {
                    fallback_lines.push(stripped.to_owned());
                } else if line.is_empty() {
                    in_fallback = false;
                } else {
                    reply_lines.push(line.to_owned());
                }
            } else {
                reply_lines.push(line.to_owned());
            }
        }

        let fallback_plain_body = format!("<{}> {}", sender, fallback_lines.join("\n"));
        let reply_plain_body = reply_lines.join("\n");

        (fallback_plain_body, sender, reply_plain_body)
    }

    fn parse_html_body(html: &str) -> (String, Option<String>) {
        let mx_reply_end = html.find("</mx-reply>");
        if let Some(end_pos) = mx_reply_end {
            let fallback = html[..end_pos]
                .replace("<mx-reply>", "")
                .replace("</mx-reply>", "");

            let reply_start = end_pos + "</mx-reply>".len();
            let reply = html[reply_start..].to_owned();

            let fallback_html = Self::extract_html_content(&fallback);
            (fallback_html, Some(reply))
        } else {
            (String::new(), None)
        }
    }

    fn extract_html_content(fallback: &str) -> String {
        if let Some(start) = fallback.find("<br />") {
            fallback[start + 6..].to_owned()
        } else if let Some(start) = fallback.find("<br/>") {
            fallback[start + 5..].to_owned()
        } else {
            fallback.to_owned()
        }
    }
}
