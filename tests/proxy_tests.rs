use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;

use api_gateway::{
    config::{GatewayConfig, RouteConfig},
    proxy::{proxy_handler, AppState},
};

async fn start_mock_backend() -> u16 {
    let app = Router::new()
        .route("/api/hello", get(|| async { "Hello from backend" }))
        .route(
            "/api/echo-header",
            get(|req: Request<Body>| async move {
                let val = req
                    .headers()
                    .get("x-custom-header")
                    .map(|v| v.to_str().unwrap().to_string())
                    .unwrap_or_else(|| "none".to_string());
                val
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });

    port
}

#[tokio::test]
async fn test_proxy_basic_rewrite() {
    let backend_port = start_mock_backend().await;

    let mut routes = HashMap::new();
    routes.insert(
        "test-service".to_string(),
        RouteConfig {
            path: "/proxy".to_string(),
            target: Some(format!("http://127.0.0.1:{}", backend_port)),
        targets: None,
        counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            strip_prefix: true,
        },
    );

    let config = GatewayConfig {
        port: 0,
        rate_limit_per_second: None,
        routes,
    };

    let state = Arc::new(AppState {
        config,
        client: Client::new(),
        settings: Default::default(),
    });

    let app = Router::new()
        .fallback(axum::routing::any(proxy_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });

    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "http://127.0.0.1:{}/proxy/api/hello",
            gateway_port
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text().await.unwrap(), "Hello from backend");

    let res = client
        .get(format!(
            "http://127.0.0.1:{}/proxy/api/echo-header",
            gateway_port
        ))
        .header("x-custom-header", "test-value")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text().await.unwrap(), "test-value");
}

#[tokio::test]
async fn test_proxy_not_found() {
    let mut routes = HashMap::new();
    routes.insert(
        "test-service".to_string(),
        RouteConfig {
            path: "/api/known".to_string(),
            target: Some("http://127.0.0.1:9999".to_string()),
        targets: None,
        counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            strip_prefix: false,
        },
    );

    let config = GatewayConfig {
        port: 0,
        rate_limit_per_second: None,
        routes,
    };

    let state = Arc::new(AppState {
        config,
        client: Client::new(),
        settings: Default::default(),
    });

    let app = Router::new()
        .fallback(axum::routing::any(proxy_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });

    let client = reqwest::Client::new();

    let res = client
        .get(format!("http://127.0.0.1:{}/api/unknown", gateway_port))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_starts_with_boundary_bug() {
    let backend_port = start_mock_backend().await;

    let mut routes = HashMap::new();
    routes.insert(
        "api-service".to_string(),
        RouteConfig {
            path: "/api".to_string(),
            target: Some(format!("http://127.0.0.1:{}", backend_port)),
        targets: None,
        counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            strip_prefix: false,
        },
    );

    let config = GatewayConfig {
        port: 0,
        rate_limit_per_second: None,
        routes,
    };

    let state = Arc::new(AppState {
        config,
        client: Client::new(),
        settings: Default::default(),
    });

    let app = Router::new()
        .fallback(axum::routing::any(proxy_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });

    let client = reqwest::Client::new();

    let res = client
        .get(format!("http://127.0.0.1:{}/api/hello", gateway_port))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let res = client
        .get(format!("http://127.0.0.1:{}/api2/hello", gateway_port))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_rate_limiting() {
    let backend_port = start_mock_backend().await;

    let mut routes = HashMap::new();
    routes.insert(
        "test-service".to_string(),
        RouteConfig {
            path: "/api".to_string(),
            target: Some(format!("http://127.0.0.1:{}", backend_port)),
        targets: None,
        counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            strip_prefix: false,
        },
    );

    let config = GatewayConfig {
        port: 0,
        rate_limit_per_second: Some(2),
        routes,
    };

    let state = Arc::new(AppState {
        config,
        client: Client::new(),
        settings: Default::default(),
    });

    let governor_conf = Arc::new(
        tower_governor::governor::GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(2)
            .finish()
            .unwrap(),
    );

    let app = Router::new()
        .fallback(axum::routing::any(proxy_handler))
        .layer(tower_governor::GovernorLayer::new(governor_conf))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/hello", gateway_port);


    for _ in 0..2 {
        let res = client.get(&url).send().await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    let res = client.get(&url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
}

async fn start_mock_backend_with_id(id: &str) -> u16 {
    let id_str = id.to_string();
    let app = Router::new().route("/api/id", get(move || async move { id_str.clone() }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });
    
    port
}

#[tokio::test]
async fn test_load_balancing_round_robin() {
    let backend1_port = start_mock_backend_with_id("backend-1").await;
    let backend2_port = start_mock_backend_with_id("backend-2").await;
    let backend3_port = start_mock_backend_with_id("backend-3").await;

    let target1 = format!("http://127.0.0.1:{}", backend1_port);
    let target2 = format!("http://127.0.0.1:{}", backend2_port);
    let target3 = format!("http://127.0.0.1:{}", backend3_port);

    let mut routes = HashMap::new();
    routes.insert(
        "lb-service".to_string(),
        RouteConfig {
            path: "/api".to_string(),
            target: None,
            targets: Some(vec![target1, target2, target3]),
            counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            strip_prefix: false,
        },
    );

    let config = GatewayConfig {
        port: 0,
        rate_limit_per_second: None,
        routes,
    };

    let state = std::sync::Arc::new(AppState {
        config,
        client: Client::new(),
        settings: Default::default(),
    });

    let app = Router::new()
        .fallback(axum::routing::any(proxy_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/id", gateway_port);

    // Make 6 requests, expect 2 hits each
    let mut hits = HashMap::new();
    
    for _ in 0..6 {
        let res = client.get(&url).send().await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let id_res = res.text().await.unwrap();
        *hits.entry(id_res).or_insert(0) += 1;
    }

    assert_eq!(hits.get("backend-1"), Some(&2));
    assert_eq!(hits.get("backend-2"), Some(&2));
    assert_eq!(hits.get("backend-3"), Some(&2));
}

// ---- upstream timeout tests ----

async fn start_slow_backend() -> u16 {
    let app = Router::new()
        .route("/slow", get(|| async {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            "slow ok"
        }))
        .route("/api/uploads/slow", get(|| async {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            "slow ok"
        }))
        .route("/fast", get(|| async { "fast ok" }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    port
}

async fn start_gateway_with(
    target: String,
    settings: api_gateway::settings::UpstreamSettings,
) -> u16 {
    let mut routes = HashMap::new();
    routes.insert(
        "r".to_string(),
        RouteConfig {
            path: "/".to_string(),
            target: Some(target),
            targets: None,
            counter: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            strip_prefix: false,
        },
    );
    let config = GatewayConfig { port: 0, rate_limit_per_second: None, routes };
    let client = api_gateway::settings::build_client(&settings);
    let state = Arc::new(AppState { config, client, settings });
    let app = Router::new()
        .fallback(axum::routing::any(proxy_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });
    port
}

fn short_settings() -> api_gateway::settings::UpstreamSettings {
    let mut s = api_gateway::settings::UpstreamSettings::default();
    s.timeout = std::time::Duration::from_secs(1);
    s.long_timeout = std::time::Duration::from_secs(10);
    s
}

#[tokio::test]
async fn test_upstream_timeout_returns_504_quickly() {
    let backend = start_slow_backend().await;
    let gw = start_gateway_with(format!("http://127.0.0.1:{}", backend), short_settings()).await;
    let start = std::time::Instant::now();
    let res = reqwest::get(format!("http://127.0.0.1:{}/slow", gw)).await.unwrap();
    assert_eq!(res.status(), 504);
    assert!(start.elapsed() < std::time::Duration::from_millis(2500));
    let body = res.text().await.unwrap();
    assert!(body.contains("\"error\""));
}

#[tokio::test]
async fn test_long_route_not_cut_by_timeout() {
    let backend = start_slow_backend().await;
    let gw = start_gateway_with(format!("http://127.0.0.1:{}", backend), short_settings()).await;
    let res = reqwest::get(format!("http://127.0.0.1:{}/api/uploads/slow", gw)).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "slow ok");
}

#[tokio::test]
async fn test_fast_request_unaffected() {
    let backend = start_slow_backend().await;
    let gw = start_gateway_with(format!("http://127.0.0.1:{}", backend), short_settings()).await;
    let res = reqwest::get(format!("http://127.0.0.1:{}/fast", gw)).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "fast ok");
}

#[tokio::test]
async fn test_connection_refused_returns_502() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead = l.local_addr().unwrap().port();
    drop(l);
    let gw = start_gateway_with(format!("http://127.0.0.1:{}", dead), short_settings()).await;
    let res = reqwest::get(format!("http://127.0.0.1:{}/fast", gw)).await.unwrap();
    assert_eq!(res.status(), 502);
}
