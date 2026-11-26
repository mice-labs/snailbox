//! What `list` found, and the forms it prints in: text lines with a notice, and one JSON
//! object.

use crate::message::{Message, MessageId};

pub const AFTER_FLAG: &str = "--after";
pub const BEFORE_FLAG: &str = "--before";

/// What `list` found: the Messages returned, oldest first, and what the limit cut.
pub struct Listing {
    pub messages: Vec<Message>,
    /// `None` when the limit cut nothing.
    pub left_out: Option<LeftOut>,
}

/// The Messages a limit cut from a listing, and the cursor that continues from the ones kept.
pub struct LeftOut {
    pub count: usize,
    pub cursor: Cursor,
}

/// Where the Messages a limit cut lie, named by the kept Message next to them.
pub enum Cursor {
    /// They were stored before the first Message returned, whose id this is.
    Before(MessageId),
    /// They were stored after the last Message returned, whose id this is.
    After(MessageId),
}

impl Cursor {
    fn direction(&self) -> &'static str {
        match self {
            Cursor::Before(_) => "older",
            Cursor::After(_) => "newer",
        }
    }

    fn flag(&self) -> &'static str {
        match self {
            Cursor::Before(_) => BEFORE_FLAG,
            Cursor::After(_) => AFTER_FLAG,
        }
    }

    fn id(&self) -> &MessageId {
        match self {
            Cursor::Before(id) | Cursor::After(id) => id,
        }
    }
}

impl LeftOut {
    /// The flag and id that read the Messages a limit left out.
    fn next_args(&self) -> String {
        format!("{} {}", self.cursor.flag(), self.cursor.id())
    }

    /// The line text output ends with when a limit left Messages out.
    fn notice(&self) -> String {
        let direction = self.cursor.direction();
        let flag = self.cursor.flag();
        let count = self.count;
        let noun = if count == 1 { "Message" } else { "Messages" };
        format!(
            "{count} {direction} {noun} left out by the limit; read them with the same flags but {} in place of any {flag} (or --limit 0 for no limit)",
            self.next_args(),
        )
    }
}

impl Listing {
    /// One `Message::to_list_line` per Message, each ending in a line feed, then the left-out
    /// notice as a last line when the limit cut Messages.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for message in &self.messages {
            text.push_str(&message.to_list_line());
            text.push('\n');
        }
        if let Some(left_out) = &self.left_out {
            text.push_str(&left_out.notice());
            text.push('\n');
        }
        text
    }

    /// One JSON object, whether or not the limit cut anything: `messages` (as
    /// `Message::to_list_json_value`), `left_out` (the count, 0 for none) and `next` (the
    /// flag and id that read them, null for none).
    pub fn to_json(&self) -> String {
        let messages: Vec<_> = self
            .messages
            .iter()
            .map(Message::to_list_json_value)
            .collect();
        serde_json::json!({
            "messages": messages,
            "left_out": self.left_out.as_ref().map_or(0, |left_out| left_out.count),
            "next": self.left_out.as_ref().map(LeftOut::next_args),
        })
        .to_string()
    }
}
