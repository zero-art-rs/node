use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CursorRecord {
    pub user_id: String,
    pub cursor: i64,
}

impl fmt::Display for CursorRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[user_id: {}, cursor: {}]", self.user_id, self.cursor,)
    }
}
