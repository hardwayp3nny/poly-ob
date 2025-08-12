use anyhow::Result;
use reqwest::{Client, ClientBuilder};
use std::time::Duration;

use crate::types::{BookTokenParam, OrderBookSnapshot};

#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
    base: String,
}

impl HttpClient {
    pub fn new(base: impl Into<String>) -> Result<Self> {
        let inner = ClientBuilder::new()
            .use_rustls_tls()
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(50)
            .tcp_keepalive(Duration::from_secs(30))
            .connect_timeout(Duration::from_millis(1200))
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .build()?;
        Ok(Self { inner, base: base.into() })
    }

    pub async fn get_book(&self, token_id: &str) -> Result<OrderBookSnapshot> {
        let url = format!("{}/book?token_id={}", self.base, token_id);
        let resp = self.inner.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("GET /book {}", status);
        }
        Ok(resp.json::<OrderBookSnapshot>().await?)
    }

    pub async fn get_books(&self, token_ids: &[String]) -> Result<Vec<OrderBookSnapshot>> {
        // POST /books with raw array body: [{ "token_id": "..." }, ...]
        let url = format!("{}/books", self.base);
        let body: Vec<BookTokenParam> = token_ids
            .iter()
            .map(|t| BookTokenParam { token_id: t.clone() })
            .collect();
        let resp = self
            .inner
            .post(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
            )
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Origin", &self.base)
            .header("Referer", format!("{}/", &self.base))
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("send POST /books failed: {:?}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("POST /books {}: {}", status, text);
        }
        Ok(resp.json::<Vec<OrderBookSnapshot>>().await?)
    }
}


