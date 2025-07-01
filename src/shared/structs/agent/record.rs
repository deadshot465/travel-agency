use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs, Role,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serenity::all::GuildChannel;
use uuid::Uuid;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Content {
    Plain(String),
    Dynamic(Value),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PlanRecord {
    pub id: Uuid,
    pub messages: Vec<Message>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Content,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PlanMapping {
    pub plan_id: Uuid,
    pub thread_id: GuildChannel,
}

impl Message {
    pub fn to_openai_message(&self) -> anyhow::Result<ChatCompletionRequestMessage> {
        let message = match self.role {
            Role::System => ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(self.content.extract_content()?)
                    .build()?,
            ),
            Role::User => ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(self.content.extract_content()?)
                    .build()?,
            ),
            Role::Assistant => ChatCompletionRequestMessage::Assistant(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(self.content.extract_content()?)
                    .build()?,
            ),
            _ => panic!("Unexpected message type."),
        };

        Ok(message)
    }
}

impl Content {
    pub fn extract_content(&self) -> anyhow::Result<String> {
        let content = match self {
            Content::Plain(s) => s.clone(),
            Content::Dynamic(v) => {
                if let Some(value_map) = v.as_object() {
                    serde_json::to_string_pretty(value_map)?
                } else {
                    "".into()
                }
            }
        };

        Ok(content)
    }
}
