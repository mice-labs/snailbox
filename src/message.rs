//! A Message, its id, and the two forms `show` prints it in.

use std::fmt;

use chrono::{DateTime, Local, SecondsFormat, Utc};
use uuid::Uuid;

/// A Message's id: a random UUID version 4 that snailbox generates (ADR 0001).
pub struct MessageId(Uuid);

impl MessageId {
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

impl std::str::FromStr for MessageId {
    type Err = uuid::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(text).map(Self)
    }
}

/// What a creator supplies for a new Message; the Inbox adds the id and creation time.
pub struct NewMessage {
    pub sender: String,
    pub receivers: Vec<String>,
    pub subject: String,
    pub body: Option<String>,
    /// The id, or a unique prefix of it, of the Message this one replies to.
    pub reply_to: Option<String>,
}

pub struct Message {
    pub id: MessageId,
    pub sender: String,
    pub receivers: Vec<String>,
    pub subject: String,
    pub body: Option<String>,
    pub parent: Option<MessageId>,
    pub created_at: DateTime<Utc>,
}

impl Message {
    /// Header lines (with a `Parent:` line after `Subject:` for a Reply), then a blank line and
    /// the body when there is one. The creation time is in the local time zone, which chrono
    /// reads from `TZ` or else the system setting.
    pub fn to_text(&self) -> String {
        let created = self
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S %:z");
        let parent = self
            .parent
            .as_ref()
            .map(|parent| format!("Parent:   {parent}\n"))
            .unwrap_or_default();
        let mut text = format!(
            "Id:       {}\nFrom:     {}\nTo:       {}\nSubject:  {}\n{parent}Created:  {created}\n",
            self.id,
            self.sender,
            self.receivers.join(", "),
            self.subject,
        );
        if let Some(body) = &self.body {
            text.push('\n');
            text.push_str(body);
            text.push('\n');
        }
        text
    }

    /// One JSON object; `created_at` is RFC 3339 in UTC, and `body` and `parent` are null when
    /// absent.
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "id": self.id.to_string(),
            "from": self.sender,
            "to": self.receivers,
            "subject": self.subject,
            "created_at": self.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            "body": self.body,
            "parent": self.parent.as_ref().map(ToString::to_string),
        })
        .to_string()
    }
}
