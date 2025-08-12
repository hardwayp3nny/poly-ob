use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookLevel {
    pub price: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub market: String,
    pub asset_id: String,
    pub hash: String,
    pub timestamp: String,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    #[serde(default)]
    pub min_order_size: Option<String>,
    #[serde(default)]
    pub neg_risk: Option<bool>,
    #[serde(default)]
    pub tick_size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BooksRequestParams {
    pub params: Vec<BookTokenParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookTokenParam {
    pub token_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisBookRecord {
    pub bids: String,
    pub asks: String,
    pub hash: String,
    pub timestamp: String,
    pub updated_at: i64,
    pub market: String,
}


