use crate::imports::*;
use crate::monitor::monitor;
use axum::{
    async_trait,
    extract::{path::ErrorKind, rejection::PathRejection, FromRequestParts, Query},
    http::{header, request::Parts, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    // Json,
    Router,
};
use tokio::net::TcpListener;

use axum::{error_handling::HandleErrorLayer, BoxError};
use std::time::Duration;
use tower::{buffer::BufferLayer, limit::RateLimitLayer, ServiceBuilder};
use tower_http::cors::{Any, CorsLayer};

pub async fn server(args: &Args) -> Result<(TcpListener, Router)> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let app = Router::new().route("/v1/wrpc/:encoding/:network", get(get_elected_node));

    let app = if args.status {
        log_warn!("Routes", "Enabling `/status` route");
        app.route("/status", get(get_status_all_nodes))
    } else {
        log_success!("Routes", "Disabling `/status` route");
        app
    };

    let app = if let Some(rate_limit) = args.rate_limit.as_ref() {
        log_success!("Limits", "Setting rate limit to: {} requests per {} seconds", rate_limit.requests, rate_limit.period);
        app.layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Unhandled error: {}", err))
                }))
                .layer(BufferLayer::new(1024))
                .layer(RateLimitLayer::new(rate_limit.requests, Duration::from_secs(rate_limit.period))),
        )
    } else {
        log_warn!("Limits", "Rate limit is disabled");
        app
    };

    let app = app.layer(CorsLayer::new().allow_origin(Any));

    log_success!("Server", "Listening on http://{}", args.listen.as_str());
    let listener = tokio::net::TcpListener::bind(args.listen.as_str()).await.unwrap();
    Ok((listener, app))
}

// respond with a JSON object containing the status of all nodes
async fn get_status_all_nodes() -> impl IntoResponse {
    let json = monitor().get_all_json();
    (StatusCode::OK, [(header::CONTENT_TYPE, HeaderValue::from_static(mime::APPLICATION_JSON.as_ref()))], json).into_response()
}

// respond with a JSON object containing the elected node
async fn get_elected_node(Query(_query): Query<QueryParams>, Path(params): Path<PathParams>) -> impl IntoResponse {
    // println!("params: {:?}", params);
    // println!("query: {:?}", query);

    if let Some(json) = monitor().get_json(&params) {
        ([(header::CONTENT_TYPE, HeaderValue::from_static(mime::APPLICATION_JSON.as_ref()))], json).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()))],
            "NOT FOUND".to_string(),
        )
            .into_response()
    }
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
