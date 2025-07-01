use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
};
use serenity::all::ImageHash;

pub fn build_one_shot_messages(
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<Vec<ChatCompletionRequestMessage>> {
    Ok(vec![
        ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_prompt)
                .build()?,
        ),
        ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_prompt)
                .build()?,
        ),
    ])
}

pub fn create_avatar_url(id: u64, image_hash: ImageHash) -> String {
    let hash = image_hash.to_string();
    format!(
        "https://cdn.discordapp.com/avatars/{}/{}.webp?size=1024",
        id, hash
    )
}
