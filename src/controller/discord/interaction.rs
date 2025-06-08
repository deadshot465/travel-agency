use axum::body::Bytes;
use axum::http::StatusCode;
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serenity::all::CommandInteraction;

use crate::shared::structs::discord::interaction::{InteractionRequest, InteractionResponse};

pub async fn handle_interaction(request: Bytes) -> Response {
    let bytes = request.to_vec();

    match serde_json::from_slice::<CommandInteraction>(&bytes) {
        Ok(command_request) => {
            tracing::debug!(
                "Received incoming command interaction: {:?}",
                &command_request
            );
            StatusCode::OK.into_response()
        }
        Err(_) => match serde_json::from_slice::<InteractionRequest>(&bytes) {
            Ok(ping_request) => {
                if ping_request.r#type == 1 {
                    (StatusCode::OK, Json(InteractionResponse { r#type: 1 })).into_response()
                } else {
                    StatusCode::BAD_REQUEST.into_response()
                }
            }
            Err(e) => {
                let error_message = format!("Failed to deserialize incoming payload: {}", e);
                tracing::error!("{}", &error_message);
                StatusCode::BAD_REQUEST.into_response()
            }
        },
    }
}
