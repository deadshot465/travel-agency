use std::time::Instant;

use command_macros::command_handler;
use serenity::all::{CommandInteraction, EditInteractionResponse, EditMessage};

use crate::shared::structs::AppState;

#[command_handler]
pub async fn ping(interaction: CommandInteraction, app_state: AppState) -> anyhow::Result<()> {
    let edited_content = EditInteractionResponse::new().content("Pinging...");

    let start = Instant::now();

    let mut message = app_state
        .http
        .edit_original_interaction_response(&interaction.token, &edited_content, Vec::new())
        .await?;

    let end = Instant::now();
    let elapsed = end.duration_since(start);

    let edit = EditMessage::new().content(format!("Latency: {} ms", elapsed.as_millis()));

    message.edit(&app_state.http, edit).await?;

    Ok(())
}
