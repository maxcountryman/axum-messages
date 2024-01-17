use std::net::SocketAddr;

use axum::{
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use axum_messages::Messages;
use time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

#[tokio::main]
async fn main() {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::days(1)));

    let app = Router::new()
        .route("/", get(set_messages_handler))
        .route("/read-messages", get(read_messages_handler))
        .layer(session_layer);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

async fn read_messages_handler(messages: Messages) -> impl IntoResponse {
    let messages = messages
        .into_iter()
        .map(|(level, message)| format!("{:?}: {}", level, message))
        .collect::<Vec<_>>()
        .join(", ");

    if messages.is_empty() {
        "No messages yet!".to_string()
    } else {
        messages
    }
}

async fn set_messages_handler(messages: Messages) -> impl IntoResponse {
    messages
        .info("Hello, world!")
        .debug("This is a debug message.")
        .save()
        .await
        .unwrap();

    Redirect::to("/read-messages")
}
