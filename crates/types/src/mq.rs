use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct MqTopic(String);

impl From<String> for MqTopic {
    fn from(s: String) -> Self {
        MqTopic(s)
    }
}

impl Deref for MqTopic {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}