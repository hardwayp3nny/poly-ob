use anyhow::Result;
use poly_ob_common::http::HttpClient;
use poly_ob_common::redisx::RedisClient;
use poly_ob_common::settings::{load_fetch, FetchConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, Instant};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cfg = load_fetch("fetch_config.toml")?;
    run_fetcher(cfg).await
}

async fn run_fetcher(cfg: FetchConfig) -> Result<()> {
    let http = HttpClient::new(&cfg.base_url)?;
    let redis = RedisClient::connect(&cfg.redis_url).await?;

    let listener = TcpListener::bind(&cfg.bind_addr).await?;
    info!("fetch node {} listening {}", cfg.node_id, cfg.bind_addr);

    // 上次 payload
    let mut last_tokens: Vec<String> = Vec::new();

    loop {
        let (mut socket, peer) = listener.accept().await?;
        let http = http.clone();
        let mut redis = redis.clone();
        let mut last_tokens_ref = last_tokens.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(&mut socket, &http, &mut redis, &mut last_tokens_ref).await {
                error!("handle_conn from {} error: {}", peer, e);
            }
        });
    }
}

async fn handle_conn(sock: &mut TcpStream, http: &HttpClient, redis: &mut RedisClient, last: &mut Vec<String>) -> Result<()> {
    // 读取长度前缀（容错：若 EOF，静默返回）
    let mut len_buf = [0u8; 4];
    if let Err(e) = sock.read_exact(&mut len_buf).await {
        if e.kind() == std::io::ErrorKind::UnexpectedEof || e.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(());
        } else {
            return Err(e.into());
        }
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    sock.read_exact(&mut buf).await?;
    let msg: serde_json::Value = serde_json::from_slice(&buf)?;

    let tokens = if let Some(arr) = msg.get("tokens").and_then(|v| v.as_array()) {
        let v: Vec<String> = arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
        if !v.is_empty() { *last = v.clone(); }
        if last.is_empty() { anyhow::bail!("empty tokens payload and no last state") }
        last.clone()
    } else {
        if last.is_empty() { anyhow::bail!("no tokens and empty last state") } else { last.clone() }
    };

    let start = Instant::now();
    // 批量请求 /books，打印关键定位信息
    let sample = tokens.get(0).cloned().unwrap_or_default();
    let sample2 = tokens.get(1).cloned().unwrap_or_default();
    tracing::info!(
        size = tokens.len(),
        %sample,
        %sample2,
        "dispatch batch"
    );
    let books = match http.get_books(&tokens).await {
        Ok(b) => b,
        Err(e) => {
            // 打印服务端返回文本（已在 HttpClient 中拼入状态与文本），不再做 fallback
            tracing::error!(err = %e, size = tokens.len(), "books endpoint failed");
            return Ok(());
        }
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    for ob in books.iter() {
        let res = redis.cas_upsert_book(ob, now_ms).await?;
        if res == "updated" {
            // 可选：发布实时更新到频道，供可视化订阅
            let _ = redis.publish_update("ob_updates", ob).await;
        }
    }
    let elapsed = start.elapsed();
    tracing::info!(fetched = books.len(), took_ms = %elapsed.as_millis(), "batch done");

    // 回复简单 OK
    let ok = b"OK";
    sock.write_all(ok).await?;
    sock.shutdown().await?;
    Ok(())
}


