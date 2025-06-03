use axum::http::StatusCode;
use axum::{
    Json,
    response::{IntoResponse, Response},
};

use crate::shared::structs::discord::ping::{PingRequest, PingResponse};

pub async fn ack_ping(Json(ping_request): Json<PingRequest>) -> Response {
    if ping_request.r#type == 1 {
        (StatusCode::OK, Json(PingResponse { r#type: 1 })).into_response()
    } else {
        StatusCode::BAD_REQUEST.into_response()
    }
}
