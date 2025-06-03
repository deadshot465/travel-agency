use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PingRequest {
    pub r#type: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PingResponse {
    pub r#type: i32,
}
