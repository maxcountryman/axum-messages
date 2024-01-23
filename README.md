<h1 align="center">
    axum-messages
</h1>

<p align="center">
    🛎️ One-time notification messages for Axum.
</p>

<div align="center">
    <a href="https://crates.io/crates/axum-messages">
        <img src="https://img.shields.io/crates/v/axum-messages.svg" />
    </a>
    <a href="https://docs.rs/axum-messages">
        <img src="https://docs.rs/axum-messages/badge.svg" />
    </a>
    <a href="https://github.com/maxcountryman/axum-messages/actions/workflows/rust.yml">
        <img src="https://github.com/maxcountryman/axum-messages/actions/workflows/rust.yml/badge.svg" />
    </a>
</div>

## 🎨 Overview

This crate provides one-time notification messages, or flash messages, for `axum` applications.

It's built on top of [`tower-sessions`](https://github.com/maxcountryman/tower-sessions), so applications that already use `tower-sessions` can use this crate with minimal setup.

For an implementation that uses `axum-extra` cookies, please see [`axum-flash`](https://crates.io/crates/axum-flash); `axum-messages` borrows from that crate, but simplifies the API by leveraging `tower-sessions`.

This crate's implementation is inspired by the [Django messages framework](https://docs.djangoproject.com/en/5.0/ref/contrib/messages/).

## 📦 Install

To use the crate in your project, add the following to your `Cargo.toml` file:

```toml
[dependencies]
axum-messages = "0.3.0"
```

## 🤸 Usage

### Example

```rust
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
```

You can find this [example][basic-example] in the [example directory][examples].

## 🦺 Safety

This crate uses `#![forbid(unsafe_code)]` to ensure everything is implemented in 100% safe Rust.

## 🛟 Getting Help

We've put together a number of [examples][examples] to help get you started. You're also welcome to [open a discussion](https://github.com/maxcountryman/axum-messages/discussions/new?category=q-a) and ask additional questions you might have.

## 👯 Contributing

We appreciate all kinds of contributions, thank you!

[basic-example]: https://github.com/maxcountryman/axum-messages/tree/main/examples/basic.rs
[examples]: https://github.com/maxcountryman/axum-messages/tree/main/examples
