use anyhow::Result;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{info, Level};

#[derive(Parser, Debug)]
struct Args {
    /// Token id to benchmark
    #[arg(long)]
    token: String,
    /// Redis url (subscribe ob_updates for REST path)
    #[arg(long, default_value = "redis://127.0.0.1:6379")]
    redis: String,
    /// Output CSV path
    #[arg(long, default_value = "bench.csv")]
    out: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    let args = Args::parse();

    // spawn WS task
    let (ws_tx, mut ws_rx) = tokio::sync::mpsc::unbounded_channel::<(String, i128)>();
    let token1 = args.token.clone();
    tokio::spawn(async move {
        if let Err(e) = run_ws(token1, ws_tx).await {
            tracing::error!("ws error: {}", e);
        }
    });

    // spawn REST consumer from Redis (printer-like)
    let (rest_tx, mut rest_rx) = tokio::sync::mpsc::unbounded_channel::<(String, i128)>();
    let token2 = args.token.clone();
    let redis_url = args.redis.clone();
    tokio::spawn(async move {
        if let Err(e) = run_rest_from_redis(redis_url, token2, rest_tx).await {
            tracing::error!("rest error: {}", e);
        }
    });

    // open CSV
    let mut wtr = csv::Writer::from_path(&args.out)?;
    wtr.write_record(["source", "hash", "local_ms"])?;

    // dedupe on (source, hash)
    use std::collections::HashSet;
    let mut seen_ws: HashSet<String> = HashSet::new();
    let mut seen_rest: HashSet<String> = HashSet::new();

    loop {
        tokio::select! {
            Some((hash, t)) = ws_rx.recv() => {
                if seen_ws.insert(hash.clone()) {
                    wtr.write_record(["ws", &hash, &t.to_string()])?;
                    wtr.flush()?;
                    info!("ws hash={} t_ms={}", hash, t);
                }
            }
            Some((hash, t)) = rest_rx.recv() => {
                if seen_rest.insert(hash.clone()) {
                    wtr.write_record(["rest", &hash, &t.to_string()])?;
                    wtr.flush()?;
                    info!("rest hash={} t_ms={}", hash, t);
                }
            }
        }
    }
}

async fn run_ws(token: String, tx: tokio::sync::mpsc::UnboundedSender<(String, i128)>) -> Result<()> {
    // channel per docs: wss://ws-subscriptions-clob.polymarket.com/ws/ + subscribe {"type":"market","assets_ids":[token]}
    // 按 stream.py 的方式连接 market 频道
    let (ws, _) = connect_async("wss://ws-subscriptions-clob.polymarket.com/ws/market").await?;
    // subscribe
    let sub = serde_json::json!({
        "type": "market",
        "assets_ids": [token],
    }).to_string();
    let (mut write, mut read) = ws.split();
    write.send(Message::Text(sub)).await?;

    // 可选心跳，防止长连被关闭
    tokio::spawn(async move {
        loop {
            if write.send(Message::Text("PING".into())).await.is_err() { break; }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let Message::Text(txt) = msg {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        if item.get("event_type").and_then(|x| x.as_str()) == Some("book") {
                            if let Some(hash) = item.get("hash").and_then(|x| x.as_str()) {
                                let now_ms = chrono::Utc::now().timestamp_millis() as i128;
                                let _ = tx.send((hash.to_string(), now_ms));
                            }
                        }
                    }
                } else if let Some(et) = val.get("event_type").and_then(|x| x.as_str()) {
                    if et == "book" {
                        if let Some(hash) = val.get("hash").and_then(|x| x.as_str()) {
                            let now_ms = chrono::Utc::now().timestamp_millis() as i128;
                            let _ = tx.send((hash.to_string(), now_ms));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn run_rest_from_redis(redis_url: String, token: String, tx: tokio::sync::mpsc::UnboundedSender<(String, i128)>) -> Result<()> {
    let client = redis::Client::open(redis_url.clone())?;
    let conn = client.get_async_connection().await?;
    let mut pubsub = conn.into_pubsub();
    pubsub.subscribe("ob_updates").await?;
    use futures_util::StreamExt;
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    loop {
        let msg: redis::Msg = pubsub.on_message().next().await.unwrap();
        let payload: String = msg.get_payload()?;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&payload) {
            // 只统计目标 token
            if v.get("asset_id").and_then(|x| x.as_str()) == Some(token.as_str()) {
                if let Some(hash) = v.get("hash").and_then(|x| x.as_str()) {
                    if seen.insert(hash.to_string()) {
                        let now_ms = chrono::Utc::now().timestamp_millis() as i128;
                        let _ = tx.send((hash.to_string(), now_ms));
                    }
                }
            }
        }
    }
}


