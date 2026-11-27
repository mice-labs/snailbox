//! A Message, its id, and the forms it prints in: `show`'s text and JSON, `list`'s one-line
//! text and the JSON object `list --json` holds per Message.

use std::fmt;

use chrono::{DateTime, Local, SecondsFormat, Utc};
use uuid::Uuid;

/// A Message's id: a random UUID version 4 that snailbox generates (ADR 0001).
#[derive(Clone)]
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

/// The start of a Message's id as a person types it: a whole id or any non-empty prefix of
/// one. Ids are stored in lowercase, so it is lowercased and matches whatever the case typed.
#[derive(Clone, Debug)]
pub struct IdPrefix(String);

impl IdPrefix {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for IdPrefix {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() {
            Err("an id prefix cannot be empty".to_string())
        } else {
            Ok(Self(text.to_lowercase()))
        }
    }
}

/// A Sender's or Receiver's name, which must hold something other than whitespace.
#[derive(Clone, Debug)]
pub struct Name(String);

impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A name read back from the Inbox: it passed the blank check on the way in, so it is
    /// not checked again and a read never fails on it.
    pub fn stored(text: String) -> Self {
        Self(text)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Name {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.trim().is_empty() {
            return Err("the name is blank; give a name".to_owned());
        }
        Ok(Self(text.to_owned()))
    }
}

/// What a creator supplies for a new Message; the Inbox adds the id and creation time.
pub struct NewMessage {
    pub sender: Name,
    pub receivers: Vec<Name>,
    pub subject: String,
    pub body: Option<String>,
    /// The id, or a unique prefix of it, of the Message this one replies to.
    pub reply_to: Option<IdPrefix>,
}

pub struct Message {
    pub id: MessageId,
    pub sender: Name,
    pub receivers: Vec<Name>,
    pub subject: String,
    pub body: Option<String>,
    pub parent: Option<MessageId>,
    pub created_at: DateTime<Utc>,
}

/// `text` with each tab, carriage return and line feed replaced by a space; `to_list_line` uses it.
fn one_line(text: &str) -> String {
    text.replace(['\t', '\r', '\n'], " ")
}

impl Message {
    /// Header lines (with a `Parent:` line after `Subject:` for a Reply), then a blank line and
    /// the body when there is one. The creation time is in the local time zone, which chrono
    /// reads from `TZ` or else the system setting.
    pub fn to_text(&self) -> String {
        let created = self.local_time();
        let parent = self
            .parent
            .as_ref()
            .map(|parent| format!("Parent:   {parent}\n"))
            .unwrap_or_default();
        let mut text = format!(
            "Id:       {}\nFrom:     {}\nTo:       {}\nSubject:  {}\n{parent}Created:  {created}\n",
            self.id,
            self.sender,
            self.receivers
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join(", "),
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
        let receivers: Vec<String> = self
            .receivers
            .iter()
            .map(|r| one_line(r.as_str()))
            .collect();
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.id.short(),
            self.local_time(),
            one_line(self.sender.as_str()),
            receivers.join(","),
            one_line(&self.subject)
        )
    }

    fn local_time(&self) -> impl fmt::Display {
        self.created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S %:z")
    }

    /// One JSON object; `created_at` is RFC 3339 in UTC, and `body` and `parent` are null when
    /// absent.
    pub fn to_json(&self) -> String {
        self.to_json_value().to_string()
    }

    /// The object `to_json` prints.
    fn to_json_value(&self) -> serde_json::Value {
        let mut value = self.to_list_json_value();
        value["body"] = serde_json::json!(self.body);
        value["parent"] = serde_json::json!(self.parent.as_ref().map(ToString::to_string));
        value
    }

    /// The object `list --json` holds per Message: the fields of `to_list_line`, with the full
    /// id and an RFC 3339 UTC `created_at`, and no `body` or `parent`.
    pub fn to_list_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id.to_string(),
            "from": self.sender.as_str(),
            "to": self.receivers.iter().map(Name::as_str).collect::<Vec<_>>(),
            "subject": self.subject,
            "created_at": self.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_name_is_refused() {
        for blank in ["", "  ", " \t"] {
            let error = blank.parse::<Name>().err();
            assert_eq!(
                error.as_deref(),
                Some("the name is blank; give a name"),
                "for {blank:?}"
            );
        }
    }

    #[test]
    fn a_name_is_kept_as_given() {
        let name: Name = " alice ".parse().expect("the name is not blank");

        assert_eq!(name.as_str(), " alice ");
    }
}
