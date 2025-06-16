use types::MqTopic;
use uuid::Uuid;

/// `TopicBuilder` correctly builds topics from singular names and UUIDs<br>
pub struct TopicBuilder {
    elements: Vec<String>
}

impl TopicBuilder {
    pub fn new() -> TopicBuilder {
        TopicBuilder {
            elements: Vec::new()
        }
    }

    /// Add new prefix to the existing route:<br>
    /// `chats` + `personal` -> `chats.personal`
    pub fn prefix(mut self, element: String) -> Self {
        self.elements.push(element);
        self
    }

    /// Specify UUID of topic<br>
    /// Generally is the same as `.prefix()`, but works for `uuid::Uuid` instead of `String`
    pub fn id(mut self, id: Uuid) -> Self {
        self.elements.push(id.to_string());
        self
    }

    /// Get built-up `MqTopic` value from the builder
    pub fn build(&self) -> MqTopic {
        MqTopic::from(self.elements.join("."))
    }
}
