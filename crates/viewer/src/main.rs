use std::net::SocketAddr;

use anyhow::Result;
use axum::{routing::get, Router};
use futures::StreamExt;
// use redis::AsyncCommands; // not needed here
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // broadcaster for SSE
    let (tx, _rx) = broadcast::channel::<String>(1024);

    // spawn redis subscriber
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = subscribe_redis("redis://127.0.0.1:6379", "ob_updates", tx_clone).await {
            tracing::error!("redis subscriber error: {}", e);
        }
    });

    // HTTP server with SSE endpoint
    let app = Router::new()
        .route("/events", get(sse_handler))
        .with_state(tx)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any));

    let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    info!("viewer running at http://{}/ (SSE: /events)", addr);
    // Axum 0.7 使用 hyper::Server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn subscribe_redis(url: &str, channel: &str, tx: broadcast::Sender<String>) -> Result<()> {
    let client = redis::Client::open(url)?;
    // use simple async connection for PubSub (into_pubsub is on Connection)
    let conn = client.get_async_connection().await?;
    let mut pubsub = conn.into_pubsub();
    pubsub.subscribe(channel).await?;
    loop {
        let msg: redis::Msg = pubsub.on_message().next().await.unwrap();
        let payload: String = msg.get_payload()?;
        let _ = tx.send(payload);
    }
}

async fn sse_handler(axum::extract::State(tx): axum::extract::State<broadcast::Sender<String>>) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, axum::Error>>> {
    use axum::response::sse::{Event, Sse};
    let mut rx = tx.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    yield Ok(Event::default().data(msg));
                }
                Err(_) => break,
            }
        }
    };
    Sse::new(stream)
}


