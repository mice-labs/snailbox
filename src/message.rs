//! A Message, its id, and the forms it prints in: `show`'s text and JSON, `list`'s one-line
//! text and the JSON object `list --json` embeds.

use std::fmt;

use chrono::{DateTime, Local, SecondsFormat, Utc};
use uuid::Uuid;

/// A Message's id: a random UUID version 4 that snailbox generates (ADR 0001).
pub struct MessageId(Uuid);

impl MessageId {
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }

    /// The first eight characters of the id, which `list` prints.
    pub fn short(&self) -> String {
        self.to_string()[..8].to_owned()
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
}

pub struct Message {
    pub id: MessageId,
    pub sender: String,
    pub receivers: Vec<String>,
    pub subject: String,
    pub body: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// `text` with each tab, carriage return and line feed replaced by a space; `to_list_line` uses it.
fn one_line(text: &str) -> String {
    text.replace(['\t', '\r', '\n'], " ")
}

impl Message {
    /// Header lines, then a blank line and the body when there is one. The creation time is
    /// in the local time zone, which chrono reads from `TZ` or else the system setting.
    pub fn to_text(&self) -> String {
        let created = self.local_time();
        let mut text = format!(
            "Id:       {}\nFrom:     {}\nTo:       {}\nSubject:  {}\nCreated:  {created}\n",
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

    /// One line for `list`: the first eight characters of the id, the creation time, the
    /// Sender, the Receivers joined by commas and the subject, separated by tabs. A tab or line
    /// break inside the Sender, a Receiver or the subject prints as a space, so the line keeps
    /// its five fields.
    pub fn to_list_line(&self) -> String {
        let receivers: Vec<String> = self.receivers.iter().map(|r| one_line(r)).collect();
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.id.short(),
            self.local_time(),
            one_line(&self.sender),
            receivers.join(","),
            one_line(&self.subject)
        )
    }

    fn local_time(&self) -> impl fmt::Display {
        self.created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S %:z")
    }

    /// One JSON object; `created_at` is RFC 3339 in UTC and `body` is null when absent.
    pub fn to_json(&self) -> String {
        self.to_json_value().to_string()
    }

    /// The object `to_json` prints, for embedding in a larger document.
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id.to_string(),
            "from": self.sender,
            "to": self.receivers,
            "subject": self.subject,
            "created_at": self.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            "body": self.body,
        })
    }
}
