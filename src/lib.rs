//! This crate provides one-time notification messages, or flash messages, for
//! `axum` applications.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::net::SocketAddr;
//!
//! use axum::{
//!     response::{IntoResponse, Redirect},
//!     routing::get,
//!     Router,
//! };
//! use axum_messages::Messages;
//! use time::Duration;
//! use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
//!
//! #[tokio::main]
//! async fn main() {
//!     let session_store = MemoryStore::default();
//!     let session_layer = SessionManagerLayer::new(session_store)
//!         .with_secure(false)
//!         .with_expiry(Expiry::OnInactivity(Duration::days(1)));
//!
//!     let app = Router::new()
//!         .route("/", get(set_messages_handler))
//!         .route("/read-messages", get(read_messages_handler))
//!         .layer(session_layer);
//!
//!     let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
//!     let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
//!     axum::serve(listener, app.into_make_service())
//!         .await
//!         .unwrap();
//! }
//!
//! async fn read_messages_handler(messages: Messages) -> impl IntoResponse {
//!     let messages = messages
//!         .into_iter()
//!         .map(|(level, message)| format!("{:?}: {}", level, message))
//!         .collect::<Vec<_>>()
//!         .join(", ");
//!
//!     if messages.is_empty() {
//!         "No messages yet!".to_string()
//!     } else {
//!         messages
//!     }
//! }
//!
//! async fn set_messages_handler(messages: Messages) -> impl IntoResponse {
//!     messages
//!         .info("Hello, world!")
//!         .debug("This is a debug message.")
//!         .save()
//!         .await
//!         .unwrap();
//!
//!     Redirect::to("/read-messages")
//! }
//! ```
#![warn(
    clippy::all,
    nonstandard_style,
    future_incompatible,
    missing_debug_implementations
)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::VecDeque;

use async_trait::async_trait;
use axum_core::extract::FromRequestParts;
use http::{request::Parts, StatusCode};
use serde::{Deserialize, Serialize};
use tower_sessions_core::{session, Session};

// N.B.: Code structure directly borrowed from `axum-flash`: https://github.com/davidpdrsn/axum-flash/blob/5e8b2bded97fd10bb275d5bc66f4d020dec465b9/src/lib.rs

/// Container for a message which provides a level and message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "l")]
    level: Level,

    #[serde(rename = "m")]
    message: String,
}

type MessageQueue = VecDeque<Message>;

/// Enumeration of message levels.
///
/// This folllows directly from the [Django
/// implementation][django-message-levels].
///
/// [django-message-levels]: https://docs.djangoproject.com/en/5.0/ref/contrib/messages/#message-levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    /// Development-related messages that will be ignored (or removed) in a
    /// production deployment.
    Debug = 0,

    /// Informational messages for the user.
    Info = 1,

    /// An action was successful, e.g. “Your profile was updated successfully”.
    Success = 2,

    /// A failure did not occur but may be imminent.
    Warning = 3,

    /// An action was not successful or some other failure occurred.
    Error = 4,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct Data {
    pending_messages: MessageQueue,
    messages: MessageQueue,
}

/// An extractor which holds the state of messages, using the session to ensure
/// messages are persisted between requests.
#[derive(Debug, Clone)]
pub struct Messages {
    session: Session,
    data: Data,
}

impl Messages {
    const DATA_KEY: &'static str = "messages.data";

    /// Push a `Debug` message.
    #[must_use = "`save` must be called to persist messages in the session"]
    pub fn debug(self, message: impl Into<String>) -> Self {
        self.push(Level::Debug, message)
    }

    /// Push an `Info` message.
    #[must_use = "`save` must be called to persist messages in the session"]
    pub fn info(self, message: impl Into<String>) -> Self {
        self.push(Level::Info, message)
    }

    /// Push a `Success` message.
    #[must_use = "`save` must be called to persist messages in the session"]
    pub fn success(self, message: impl Into<String>) -> Self {
        self.push(Level::Success, message)
    }

    /// Push a `Warning` message.
    #[must_use = "`save` must be called to persist messages in the session"]
    pub fn warning(self, message: impl Into<String>) -> Self {
        self.push(Level::Warning, message)
    }

    /// Push an `Error` message.
    #[must_use = "`save` must be called to persist messages in the session"]
    pub fn error(self, message: impl Into<String>) -> Self {
        self.push(Level::Error, message)
    }

    /// Push a message with the given level.
    #[must_use = "`save` must be called to persist messages in the session"]
    pub fn push(mut self, level: Level, message: impl Into<String>) -> Self {
        self.data.pending_messages.push_back(Message {
            message: message.into(),
            level,
        });
        self
    }

    /// Save messages back to the session.
    ///
    /// Note that this must called or messages will not be persisted between
    /// requests.
    pub async fn save(self) -> Result<Self, session::Error> {
        self.session
            .insert(Self::DATA_KEY, self.data.clone())
            .await?;
        Ok(self)
    }
}

impl Iterator for Messages {
    type Item = (Level, String);

    fn next(&mut self) -> Option<Self::Item> {
        let message = self.data.messages.pop_front()?;
        Some((message.level, message.message))
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for Messages
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(req, state).await?;
        let mut data = match session.get::<Data>(Self::DATA_KEY).await {
            Ok(Some(data)) => data,
            Ok(None) => Data::default(),
            Err(_) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not get from session",
                ));
            }
        };

        // Load messages by taking them from the pending queue.
        data.messages = std::mem::take(&mut data.pending_messages);

        // Save back to the session to ensure future loads do not repeat loaded
        // messages.
        if session.insert(Self::DATA_KEY, data.clone()).await.is_err() {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not insert to session",
            ));
        };

        Ok(Self { session, data })
    }
}

#[cfg(test)]
mod tests {
    use axum::{response::Redirect, routing::get, Router};
    use axum_core::{body::Body, extract::Request, response::IntoResponse};
    use http::header;
    use http_body_util::BodyExt;
    use time::Duration;
    use tower::ServiceExt;
    use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

    use super::*;

    #[tokio::test]
    async fn basic() {
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_expiry(Expiry::OnInactivity(Duration::days(1)));

        let app = Router::new()
            .route("/", get(root))
            .route("/set-message", get(set_message))
            .layer(session_layer);

        async fn root(messages: Messages) -> impl IntoResponse {
            messages
                .into_iter()
                .map(|(level, message)| format!("{:?}: {}", level, message))
                .collect::<Vec<_>>()
                .join(", ")
        }

        #[axum::debug_handler]
        async fn set_message(messages: Messages) -> impl IntoResponse {
            messages
                .debug("Hello, world!")
                .info("This is an info message.")
                .save()
                .await
                .unwrap();
            Redirect::to("/")
        }

        let request = Request::builder()
            .uri("/set-message")
            .body(Body::empty())
            .unwrap();
        let mut response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_redirection());
        let cookie = response.headers_mut().remove(header::SET_COOKIE).unwrap();

        let request = Request::builder()
            .uri("/")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(body, "Debug: Hello, world!, Info: This is an info message.");
    }
}
