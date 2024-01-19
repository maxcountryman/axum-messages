use std::net::SocketAddr;

use axum::{
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use axum_messages::{Messages, MessagesManagerLayer};
use tower_sessions::{MemoryStore, SessionManagerLayer};

async fn set_messages_handler(messages: Messages) -> impl IntoResponse {
    messages
        .info("Hello, world!")
        .debug("This is a debug message.");

    Redirect::to("/read-messages")
}

async fn read_messages_handler(messages: Messages) -> impl IntoResponse {
    let messages = messages
        .into_iter()
        .map(|message| format!("{}: {}", message.level, message))
        .collect::<Vec<_>>()
        .join(", ");

    if messages.is_empty() {
        "No messages yet!".to_string()
    } else {
        messages
    }
}

#[tokio::main]
async fn main() {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

    let app = Router::new()
        .route("/", get(set_messages_handler))
        .route("/read-messages", get(read_messages_handler))
        .layer(MessagesManagerLayer)
        .layer(session_layer);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
