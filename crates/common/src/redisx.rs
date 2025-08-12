use anyhow::Result;
use redis::AsyncCommands;
use crate::lua::LUA_CAS_UPDATE;
use crate::types::{OrderBookSnapshot, RedisBookRecord};

#[derive(Clone)]
pub struct RedisClient {
    pub conn: redis::aio::MultiplexedConnection,
}

impl RedisClient {
    pub async fn connect(url: &str) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }

    pub async fn cas_upsert_book(&mut self, ob: &OrderBookSnapshot, now_ms: i64) -> Result<String> {
        let key = format!("ob:{}", ob.asset_id);
        let bids = serde_json::to_string(&ob.bids)?;
        let asks = serde_json::to_string(&ob.asks)?;
        let rv: String = redis::Script::new(LUA_CAS_UPDATE)
            .key(key)
            .arg(&ob.hash)
            .arg(&ob.timestamp)
            .arg(bids)
            .arg(asks)
            .arg(now_ms)
            .arg(&ob.market)
            .invoke_async(&mut self.conn)
            .await?;
        Ok(rv)
    }

    pub async fn get_book(&mut self, token_id: &str) -> Result<Option<RedisBookRecord>> {
        let key = format!("ob:{}", token_id);
        let (bids, asks, hash, timestamp, updated_at, market): (Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>, Option<String>) = self
            .conn
            .hget(&key, ("bids", "asks", "hash", "timestamp", "updated_at", "market"))
            .await?;
        if let (Some(bids), Some(asks), Some(hash), Some(timestamp), Some(updated_at), Some(market)) =
            (bids, asks, hash, timestamp, updated_at, market)
        {
            Ok(Some(RedisBookRecord { bids, asks, hash, timestamp, updated_at, market }))
        } else {
            Ok(None)
        }
    }

    pub async fn publish_update(&mut self, channel: &str, ob: &OrderBookSnapshot) -> Result<()> {
        let msg = serde_json::to_string(ob)?;
        let _: i64 = redis::cmd("PUBLISH")
            .arg(channel)
            .arg(msg)
            .query_async(&mut self.conn)
            .await?;
        Ok(())
    }
}


