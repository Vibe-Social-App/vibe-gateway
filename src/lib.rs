pub mod config;
pub mod proxy;

pub use config::load_config;
pub use proxy::{proxy_handler, AppState};

use axum::{
    routing::any,
    Router,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

pub async fn run() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_gateway=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = load_config("config.yml").expect("Failed to load config.yml");
    let port = config.port;
    
    let rate_limit = config.rate_limit_per_second.unwrap_or(100);
    
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(rate_limit)
            .burst_size(rate_limit as u32 * 2)
            .finish()
            .unwrap()
    );
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("Failed to build reqwest client");

    let state = Arc::new(AppState {
        config,
        client,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    
    let governor_layer = GovernorLayer::new(governor_conf);

    let app = Router::new()
        .fallback(any(proxy_handler))
        .layer(governor_layer)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let bind_addr = format!("0.0.0.0:{}", port);
    tracing::info!("API Gateway starting on {}", bind_addr);
    
    let listener = TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}

