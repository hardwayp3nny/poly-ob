use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use tracing::{info, Level};

#[derive(Parser, Debug)]
struct Args {
    /// Redis url
    #[arg(long, default_value = "redis://127.0.0.1:6379")]
    redis: String,
    /// Channel name
    #[arg(long, default_value = "ob_updates")]
    channel: String,
    /// Optional filter for token_id prefix
    #[arg(long)]
    filter: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    let args = Args::parse();

    let client = redis::Client::open(args.redis.clone())?;
    let conn = client.get_async_connection().await?;
    let mut pubsub = conn.into_pubsub();
    pubsub.subscribe(&args.channel).await?;

    info!("printer subscribed to '{}' on {}", args.channel, args.redis);
    loop {
        let msg: redis::Msg = pubsub.on_message().next().await.unwrap();
        let payload: String = msg.get_payload()?;
        // payload is OrderBookSnapshot JSON
        if let Some(prefix) = &args.filter {
            if !payload.contains(prefix) { continue; }
        }
        // pretty print minimal fields and top levels depth sizes
        match serde_json::from_str::<serde_json::Value>(&payload) {
            Ok(v) => {
                let asset = v.get("asset_id").and_then(|x| x.as_str()).unwrap_or("");
                let hash = v.get("hash").and_then(|x| x.as_str()).unwrap_or("");
                let ts = v.get("timestamp").and_then(|x| x.as_str()).unwrap_or("");
                let bids_len = v.get("bids").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
                let asks_len = v.get("asks").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
                println!("[{ts}] {asset} hash={hash} bids={bids_len} asks={asks_len}");
            }
            Err(_) => println!("{}", payload),
        }
    }
}


