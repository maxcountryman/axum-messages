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
//! use axum_messages::{Messages, MessagesManagerLayer};
//! use tower_sessions::{MemoryStore, SessionManagerLayer};
//!
//! async fn set_messages_handler(messages: Messages) -> impl IntoResponse {
//!     messages
//!         .info("Hello, world!")
//!         .debug("This is a debug message.");
//!
//!     Redirect::to("/read-messages")
//! }
//!
//! async fn read_messages_handler(messages: Messages) -> impl IntoResponse {
//!     let messages = messages
//!         .into_iter()
//!         .map(|message| format!("{}: {}", message.level, message))
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
//! #[tokio::main]
//! async fn main() {
//!     let session_store = MemoryStore::default();
//!     let session_layer = SessionManagerLayer::new(session_store).with_secure(false);
//!
//!     let app = Router::new()
//!         .route("/", get(set_messages_handler))
//!         .route("/read-messages", get(read_messages_handler))
//!         .layer(MessagesManagerLayer)
//!         .layer(session_layer);
//!
//!     let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
//!     let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
//!     axum::serve(listener, app.into_make_service())
//!         .await
//!         .unwrap();
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

use core::fmt;
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{self, AtomicBool},
        Arc,
    },
    task::{Context, Poll},
};

use async_trait::async_trait;
use axum_core::{
    extract::{FromRequestParts, Request},
    response::Response,
};
use http::{request::Parts, StatusCode};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tower::{Layer, Service};
use tower_sessions_core::{session, Session};

// N.B.: Code structure directly borrowed from `axum-flash`: https://github.com/davidpdrsn/axum-flash/blob/5e8b2bded97fd10bb275d5bc66f4d020dec465b9/src/lib.rs

/// Container for a message which provides a level and message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message level, i.e. `Level`.
    #[serde(rename = "l")]
    pub level: Level,

    /// The message itself.
    #[serde(rename = "m")]
    pub message: String,

    /// adding extra args
    #[serde(rename = "a")]
    pub extra_args: HashMap<String, String>,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Message {
    /// adding extra arg to the message
    pub fn add_arg(mut self, name: String, value: impl Into<String>) -> Self {
        self.extra_args.insert(name, value.into());
        self
    }
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

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Debug => "Debug",
            Self::Info => "Info",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Error => "Error",
        })
    }
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
    data: Arc<Mutex<Data>>,
    is_modified: Arc<AtomicBool>,
}

impl Messages {
    const DATA_KEY: &'static str = "axum-messages.data";

    fn new(session: Session, data: Data) -> Self {
        Self {
            session,
            data: Arc::new(Mutex::new(data)),
            is_modified: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Push a `Debug` message.
    pub fn debug(self, message: impl Into<String>) -> Self {
        self.push(Level::Debug, message)
    }

    /// Push an `Info` message.
    pub fn info(self, message: impl Into<String>) -> Self {
        self.push(Level::Info, message)
    }

    /// Push a `Success` message.
    pub fn success(self, message: impl Into<String>) -> Self {
        self.push(Level::Success, message)
    }

    /// Push a `Warning` message.
    pub fn warning(self, message: impl Into<String>) -> Self {
        self.push(Level::Warning, message)
    }

    /// Push an `Error` message.
    pub fn error(self, message: impl Into<String>) -> Self {
        self.push(Level::Error, message)
    }

    /// Push a message with the given level.
    pub fn push(self, level: Level, message: impl Into<String>) -> Self {
        {
            let mut data = self.data.lock();
            data.pending_messages.push_back(Message {
                message: message.into(),
                level,
                // default initial value for extra args
                extra_args: HashMap::new(),
            });
        }

        if !self.is_modified() {
            self.is_modified.store(true, atomic::Ordering::Release);
        }

        self
    }

    async fn save(self) -> Result<Self, session::Error> {
        self.session
            .insert(Self::DATA_KEY, self.data.clone())
            .await?;
        Ok(self)
    }

    fn load(self) -> Self {
        {
            // Load messages by taking them from the pending queue.
            let mut data = self.data.lock();
            data.messages = std::mem::take(&mut data.pending_messages);
        }
        self
    }

    fn is_modified(&self) -> bool {
        self.is_modified.load(atomic::Ordering::Acquire)
    }
}

impl Iterator for Messages {
    type Item = Message;

    fn next(&mut self) -> Option<Self::Item> {
        let mut data = self.data.lock();
        let message = data.messages.pop_front();
        if message.is_some() && !self.is_modified() {
            self.is_modified.store(true, atomic::Ordering::Release);
        }
        message
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for Messages
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Messages>()
            .cloned()
            .ok_or((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not extract messages. Is `MessagesManagerLayer` installed?",
            ))
            .map(Messages::load)
    }
}

/// MIddleware provider `Messages` as a request extension.
#[derive(Debug, Clone)]
pub struct MessagesManager<S> {
    inner: S,
}

impl<ReqBody, ResBody, S> Service<Request<ReqBody>> for MessagesManager<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send,
    ReqBody: Send + 'static,
    ResBody: Default + Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        // Because the inner service can panic until ready, we need to ensure we only
        // use the ready service.
        //
        // See: https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            let Some(session) = req.extensions().get::<Session>().cloned() else {
                let mut res = Response::default();
                *res.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                return Ok(res);
            };

            let data = match session.get::<Data>(Messages::DATA_KEY).await {
                Ok(Some(data)) => data,
                Ok(None) => Data::default(),
                Err(_) => {
                    let mut res = Response::default();
                    *res.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                    return Ok(res);
                }
            };

            let messages = Messages::new(session, data);

            req.extensions_mut().insert(messages.clone());

            let res = inner.call(req).await;

            if messages.is_modified() && messages.save().await.is_err() {
                let mut res = Response::default();
                *res.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                return Ok(res);
            };

            res
        })
    }
}

/// Layer for `MessagesManager`.
#[derive(Debug, Clone)]
pub struct MessagesManagerLayer;

impl<S> Layer<S> for MessagesManagerLayer {
    type Service = MessagesManager<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MessagesManager { inner }
    }
}

#[cfg(test)]
mod tests {
    use axum::{response::Redirect, routing::get, Router};
    use axum_core::{body::Body, extract::Request, response::IntoResponse};
    use http::header;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use tower_sessions::{MemoryStore, SessionManagerLayer};

    use super::*;

    #[tokio::test]
    async fn basic() {
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

        let app = Router::new()
            .route("/", get(root))
            .route("/set-message", get(set_message))
            .layer(MessagesManagerLayer)
            .layer(session_layer);

        async fn root(messages: Messages) -> impl IntoResponse {
            messages
                .into_iter()
                .map(|message| format!("{}: {}", message.level, message))
                .collect::<Vec<_>>()
                .join(", ")
        }

        #[axum::debug_handler]
        async fn set_message(messages: Messages) -> impl IntoResponse {
            messages
                .debug("Hello, world!")
                .info("This is an info message.");
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
