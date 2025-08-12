use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    pub redis_url: String,
    pub base_url: String,
    pub tokens: Vec<String>,
    pub fetch_nodes: Vec<String>, // ip:port (tcp socket)
    #[serde(default = "default_plan_horizon")] 
    pub plan_horizon_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FetchConfig {
    pub redis_url: String,
    pub base_url: String,
    pub node_id: String,
    #[serde(default = "default_capacity")] 
    pub capacity_rps: u32, // default 20
    #[serde(default = "default_bind")] 
    pub bind_addr: String, // 0.0.0.0:3000
}

fn default_plan_horizon() -> u64 { 5 }
fn default_capacity() -> u32 { 20 }
fn default_bind() -> String { "0.0.0.0:3000".into() }

pub fn load_client(path: &str) -> Result<ClientConfig> {
    let s = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&s)?)
}

pub fn load_fetch(path: &str) -> Result<FetchConfig> {
    let s = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&s)?)
}


