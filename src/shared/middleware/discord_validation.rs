use axum::{
    body::{Body, Bytes},
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;

const SIGNATURE_HEADER: &str = "X-Signature-Ed25519";
const TIMESTAMP_HEADER: &str = "X-Signature-Timestamp";

pub async fn validate_interaction(
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let signature = headers
        .get(SIGNATURE_HEADER)
        .cloned()
        .and_then(|v| v.to_str().map(ToString::to_string).ok())
        .unwrap_or_default();

    let timestamp = headers
        .get(TIMESTAMP_HEADER)
        .cloned()
        .and_then(|v| v.to_str().map(ToString::to_string).ok())
        .unwrap_or_default();

    match buffer_request_body(request, signature, timestamp).await {
        Ok(request) => next.run(request).await,
        Err(e) => e,
    }
}

async fn buffer_request_body(
    request: axum::extract::Request,
    signature: String,
    timestamp: String,
) -> Result<axum::extract::Request, Response> {
    let (parts, body) = request.into_parts();

    let bytes = body
        .collect()
        .await
        .map_err(|e| {
            let error_msg = format!("Internal server error when collecting body bytes: {e:?}");
            tracing::error!("{}", &error_msg);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, error_msg).into_response()
        })?
        .to_bytes();

    match validate(bytes, signature, timestamp) {
        Ok(bytes) => Ok(axum::extract::Request::from_parts(parts, Body::from(bytes))),
        Err(e) => Err(e),
    }
}

#[allow(clippy::result_large_err)]
fn validate(bytes: Bytes, signature: String, timestamp: String) -> Result<Bytes, Response> {
    let public_key =
        std::env::var("APPLICATION_PUBLIC_KEY").expect("Failed to get application public key.");

    let body = bytes.to_vec();

    match String::from_utf8(body) {
        Ok(s) => {
            let message = format!("{timestamp}{s}");

            let signature_bytes =
                hex::decode(&signature).expect("Failed to decode public key from hex value.");
            let public_key_bytes =
                hex::decode(&public_key).expect("Failed to decode public key from hex value.");

            let result =
                nacl::sign::verify(&signature_bytes, message.as_bytes(), &public_key_bytes);

            match result {
                Ok(res) => {
                    if res {
                        Ok(bytes)
                    } else {
                        Err(axum::http::StatusCode::UNAUTHORIZED.into_response())
                    }
                }
                Err(e) => {
                    let error_msg = format!("Failed to verify: {e:?}");
                    tracing::error!("{}", &error_msg);
                    Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, error_msg).into_response())
                }
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to build string from UTF-8 encoded body: {e:?}");
            tracing::error!("{}", &error_msg);
            Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, error_msg).into_response())
        }
    }
}
