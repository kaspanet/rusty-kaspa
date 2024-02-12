use crate::args::Args;
use crate::result::Result;
use axum::{
    async_trait,
    extract::{path::ErrorKind, rejection::PathRejection, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use kaspa_consensus_core::network::NetworkId;
use kaspa_wrpc_client::WrpcEncoding;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::net::TcpListener;

use axum::{error_handling::HandleErrorLayer, BoxError};
use std::time::Duration;
use tower::{buffer::BufferLayer, limit::RateLimitLayer, ServiceBuilder};

pub async fn server(args: &Args) -> Result<(TcpListener, Router)> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new().route("/status", get(status)).route("/v1/wrpc/:protocol/:network", get(handler));

    let app = if let Some(rate_limit) = args.rate_limit.as_ref() {
        println!("Setting rate limit to: {} requests per {} seconds", rate_limit.requests, rate_limit.period);
        app.layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Unhandled error: {}", err))
                }))
                .layer(BufferLayer::new(1024))
                .layer(RateLimitLayer::new(rate_limit.requests, Duration::from_secs(rate_limit.period))),
        )
    } else {
        println!("Rate limit is disabled");
        app
    };

    println!("Listening on http://{}", args.listen.as_str());
    let listener = tokio::net::TcpListener::bind(args.listen.as_str()).await.unwrap();
    Ok((listener, app))
}

// basic handler that responds with a static string
async fn status() -> &'static str {
    "Hello, World!"
}

async fn handler(Path(params): Path<Params>) -> impl IntoResponse {
    Json(params)
}

#[derive(Debug, Deserialize, Serialize)]
struct Params {
    protocol: WrpcEncoding,
    network: NetworkId,
}

// We define our own `Path` extractor that customizes the error from `axum::extract::Path`
struct Path<T>(T);

#[async_trait]
impl<S, T> FromRequestParts<S> for Path<T>
where
    // these trait bounds are copied from `impl FromRequest for axum::extract::path::Path`
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<PathError>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> std::result::Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => {
                let (status, body) = match rejection {
                    PathRejection::FailedToDeserializePathParams(inner) => {
                        let mut status = StatusCode::BAD_REQUEST;

                        let kind = inner.into_kind();
                        let body = match &kind {
                            ErrorKind::WrongNumberOfParameters { .. } => PathError { message: kind.to_string(), location: None },

                            ErrorKind::ParseErrorAtKey { key, .. } => {
                                PathError { message: kind.to_string(), location: Some(key.clone()) }
                            }

                            ErrorKind::ParseErrorAtIndex { index, .. } => {
                                PathError { message: kind.to_string(), location: Some(index.to_string()) }
                            }

                            ErrorKind::ParseError { .. } => PathError { message: kind.to_string(), location: None },

                            ErrorKind::InvalidUtf8InPathParam { key } => {
                                PathError { message: kind.to_string(), location: Some(key.clone()) }
                            }

                            ErrorKind::UnsupportedType { .. } => {
                                // this error is caused by the programmer using an unsupported type
                                // (such as nested maps) so respond with `500` instead
                                status = StatusCode::INTERNAL_SERVER_ERROR;
                                PathError { message: kind.to_string(), location: None }
                            }

                            ErrorKind::Message(msg) => PathError { message: msg.clone(), location: None },

                            _ => PathError { message: format!("Unhandled deserialization error: {kind}"), location: None },
                        };

                        (status, body)
                    }
                    PathRejection::MissingPathParams(error) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, PathError { message: error.to_string(), location: None })
                    }
                    _ => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        PathError { message: format!("Unhandled path rejection: {rejection}"), location: None },
                    ),
                };

                Err((status, axum::Json(body)))
            }
        }
    }
}

#[derive(Serialize)]
struct PathError {
    message: String,
    location: Option<String>,
}
