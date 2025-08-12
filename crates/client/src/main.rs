use anyhow::Result;
use poly_ob_common::http::HttpClient;
use poly_ob_common::redisx::RedisClient;
use poly_ob_common::settings::{load_client, ClientConfig};
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep_until, Duration, Instant};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cfg = load_client("client_config.toml")?;
    run_client(cfg).await
}

async fn run_client(cfg: ClientConfig) -> Result<()> {
    let mut redis = RedisClient::connect(&cfg.redis_url).await?;
    let http = HttpClient::new(&cfg.base_url)?;
    info!("client started, tokens={}, nodes={}", cfg.tokens.len(), cfg.fetch_nodes.len());

    // health check loop for fetch nodes
    tokio::spawn(health_loop(cfg.fetch_nodes.clone()));

    // scheduler loop
    scheduler_loop(cfg, http, &mut redis).await
}

async fn health_loop(nodes: Vec<String>) {
    loop {
        for n in &nodes {
            // TCP zero-RTT is not possible, but we can keepalive or pre-connect from fetcher side.
            // Here we just attempt connect to check liveness quickly (short timeout)
            let addr = n.clone();
            let fut = TcpStream::connect(&addr);
            let res = tokio::time::timeout(Duration::from_millis(200), fut).await;
            match res {
                Ok(Ok(mut s)) => {
                    // immediate close after connect is enough for health
                    let _ = s.shutdown().await;
                    info!("health ok: {}", addr);
                }
                _ => {
                    error!("health fail: {}", addr);
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn scheduler_loop(cfg: ClientConfig, http: HttpClient, redis: &mut RedisClient) -> Result<()> {
    let n = cfg.fetch_nodes.len().max(1);
    let delta = Duration::from_secs_f64(1.0 / (20.0 * n as f64)); // 全局节拍
    let mut next = Instant::now() + Duration::from_millis(200);

    // 每个节点维护一个最近 payload，初始为全量 tokens
    let mut last_payload: Vec<Vec<String>> = vec![cfg.tokens.clone(); n];

    let batch = cfg.tokens.len().max(1) / (20 * n) + 1; // 初始批大小，后续可自适应

    loop {
        for (i, node) in cfg.fetch_nodes.iter().enumerate() {
            // 计算该槽位分配的批（简单轮询切片）
            let offset = (chrono::Utc::now().timestamp() as usize + i) % cfg.tokens.len().max(1);
            let mut lump = Vec::with_capacity(batch);
            for k in 0..batch {
                lump.push(cfg.tokens[(offset + k) % cfg.tokens.len()].clone());
            }
            last_payload[i] = lump.clone();

            // 发送指令（TCP socket），消息格式: JSON { tokens: [...], trigger: true }
            let payload = serde_json::json!({ "tokens": lump, "trigger": true });
            if let Err(e) = send_command(node, &payload).await {
                error!("send to {} failed: {}", node, e);
            }

            // 全局节拍间隔
            sleep_until(next).await;
            next += delta;
        }
    }
}

async fn send_command(addr: &str, json: &serde_json::Value) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut stream = TcpStream::connect(addr).await?;
    let data = serde_json::to_vec(json)?;
    // 简单定界：长度前缀 + 数据
    let len = (data.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&data).await?;
    stream.shutdown().await?;
    Ok(())
}


