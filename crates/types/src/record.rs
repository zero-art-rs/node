use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Record<Data> {
    pub user_id: String,
    pub data: Data,
}

impl<Data> fmt::Display for Record<Data>
where
    Data: Serialize + DeserializeOwned + fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[user_id: {}, cursor: {}]", self.user_id, self.data)
    }
}
