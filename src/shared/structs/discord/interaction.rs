use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InteractionRequest {
    pub r#type: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InteractionResponse {
    pub r#type: i32,
}
