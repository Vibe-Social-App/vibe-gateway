use serde::Deserialize;
use std::fs;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

fn default_counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteConfig {
    pub path: String,
    pub target: Option<String>,
    pub targets: Option<Vec<String>>,
    pub strip_prefix: bool,
    #[serde(skip, default = "default_counter")]
    pub counter: Arc<AtomicUsize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GatewayConfig {
    pub port: u16,
    pub rate_limit_per_second: Option<u64>,
    pub routes: HashMap<String, RouteConfig>,
}

pub fn load_config(path: &str) -> std::io::Result<GatewayConfig> {
    let content = fs::read_to_string(path)?;
    let config: GatewayConfig = serde_yaml::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(config)
}
