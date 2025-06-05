use axum::http::StatusCode;
use axum::{
    Json,
    response::{IntoResponse, Response},
};

use crate::shared::structs::discord::interaction::{InteractionRequest, InteractionResponse};

pub async fn handle_interaction(Json(interaction_request): Json<InteractionRequest>) -> Response {
    if interaction_request.r#type == 1 {
        (StatusCode::OK, Json(InteractionResponse { r#type: 1 })).into_response()
    } else {
        StatusCode::BAD_REQUEST.into_response()
    }
}
