use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderName, HeaderValue, StatusCode},
    response::Response,
};
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::debug;

use crate::config::GatewayConfig;

pub struct AppState {
    pub config: GatewayConfig,
    pub client: Client,
}

pub async fn proxy_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let path = req.uri().path();
    let query = req.uri().query().unwrap_or_default();
    
    debug!("Proxying request: {}?{}", path, query);
    
    let mut matched_route = None;
    let mut longest_match = 0;
    
    for (_, route) in &state.config.routes {
         let exact_match = path == route.path;
         let prefix_match = path.starts_with(&format!("{}/", route.path));
         let root_match = route.path == "/" && path.starts_with('/');
         
         if (exact_match || prefix_match || root_match) && route.path.len() > longest_match {
             longest_match = route.path.len();
             matched_route = Some(route);
         }
    }
    
    let route = match matched_route {
        Some(r) => r,
        None => return Err(StatusCode::NOT_FOUND),
    };
    
    let mut target_url_str = if let Some(ref targets) = route.targets {
        if targets.is_empty() {
            return Err(StatusCode::BAD_GATEWAY);
        }
        let fetch_idx = route.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        targets[fetch_idx % targets.len()].clone()
    } else if let Some(ref t) = route.target {
        t.clone()
    } else {
        return Err(StatusCode::BAD_GATEWAY);
    };

    if target_url_str.starts_with("ws://") {
        target_url_str = target_url_str.replacen("ws://", "http://", 1);
    } else if target_url_str.starts_with("wss://") {
        target_url_str = target_url_str.replacen("wss://", "https://", 1);
    }
    
    let remaining_path = if route.strip_prefix {
        let stripped = path.trim_start_matches(&route.path);
        if stripped.is_empty() { "/" } else { stripped }
    } else {
        path
    };
    
    let target_ends_slash = target_url_str.ends_with('/');
    let remain_starts_slash = remaining_path.starts_with('/');
    
    if target_ends_slash && remain_starts_slash {
        target_url_str.pop();
        target_url_str.push_str(remaining_path);
    } else if !target_ends_slash && !remain_starts_slash {
        target_url_str.push('/');
        target_url_str.push_str(remaining_path);
    } else {
        if !(remaining_path == "/" && target_ends_slash) {
            target_url_str.push_str(remaining_path);
        }
    }
    
    if !query.is_empty() {
        target_url_str.push('?');
        target_url_str.push_str(query);
    }
    
    let mut is_upgrade = false;
    let headers = req.headers();
    
    let has_upgrade_con = headers.get(axum::http::header::CONNECTION)
        .map(|v| v.to_str().unwrap_or("").to_lowercase().contains("upgrade"))
        .unwrap_or(false);
        
    let has_websocket_upg = headers.get(axum::http::header::UPGRADE)
        .map(|v| v.to_str().unwrap_or("").eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
        
    if has_upgrade_con && has_websocket_upg {
        is_upgrade = true;
    }

    let on_upgrade = if is_upgrade {
        Some(hyper::upgrade::on(&mut req))
    } else {
        None
    };

    let method = req.method().clone();
    
    let scheme = req.uri().scheme_str().unwrap_or("http");
    
    let mut headers = req.headers().clone();
    
    let mut hop_by_hop = vec![
        "keep-alive".to_string(),
        "proxy-authenticate".to_string(),
        "proxy-authorization".to_string(),
        "te".to_string(),
        "trailers".to_string(),
        "transfer-encoding".to_string(),
    ];
    
    if !is_upgrade {
        hop_by_hop.push("connection".to_string());
        hop_by_hop.push("upgrade".to_string());
    }
    
    if let Some(conn_header) = headers.get(axum::http::header::CONNECTION) {
        if let Ok(conn_str) = conn_header.to_str() {
            for token in conn_str.split(',') {
                let clean_token = token.trim().to_lowercase();
                if !hop_by_hop.contains(&clean_token) {
                    hop_by_hop.push(clean_token);
                }
            }
        }
    }

    for header in &hop_by_hop {
        headers.remove(header);
    }

    headers.remove("content-length");
    headers.remove("content-encoding");

    let host = headers.remove("host").unwrap_or(HeaderValue::from_static("unknown"));

    let client_ip = addr.ip().to_string();
    
    if let Ok(ip_val) = HeaderValue::from_str(&client_ip) {
        headers.insert("x-forwarded-for", ip_val);
    }
    headers.insert("x-forwarded-host", host);
    if let Ok(proto_val) = HeaderValue::from_str(scheme) {
        headers.insert("x-forwarded-proto", proto_val);
    }

    let is_get = method == axum::http::Method::GET;
    let mut proxy_req = state.client.request(method, target_url_str);
    
    for (name, value) in headers.iter() {
        proxy_req = proxy_req.header(name.as_str(), value.as_bytes());
    }
    
    if !is_get && !is_upgrade {
        let body_stream = req.into_body().into_data_stream();
        let proxy_body = reqwest::Body::wrap_stream(body_stream);
        proxy_req = proxy_req.body(proxy_body);
    }
    
    let res = match proxy_req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Proxy error: {:?}", e);
            return Err(StatusCode::BAD_GATEWAY);
        }
    };
    
    let status_code = res.status();
    let res_headers = res.headers().clone();

    if status_code == StatusCode::SWITCHING_PROTOCOLS && is_upgrade {
        if let Some(on_upgrade) = on_upgrade {
            tokio::spawn(async move {
                let upgraded_backend = res.upgrade().await;
                let upgraded_client = on_upgrade.await;
                match (upgraded_backend, upgraded_client) {
                    (Ok(mut backend), Ok(client)) => {
                        let mut tokio_client = hyper_util::rt::TokioIo::new(client);
                        let _ = tokio::io::copy_bidirectional(&mut tokio_client, &mut backend).await;
                    }
                    (Err(e), _) => tracing::error!("Backend upgrade failed: {:?}", e),
                    (_, Err(e)) => tracing::error!("Client upgrade failed: {:?}", e),
                }
            });
            
            let mut axum_res = Response::builder()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .body(Body::empty())
                .unwrap();
                
            let axum_headers = axum_res.headers_mut();
            for (name, value) in res_headers.iter() {
                if let Ok(value) = HeaderValue::from_bytes(value.as_bytes()) {
                    let name_str = name.as_str().to_string();
                    if !hop_by_hop.contains(&name_str) {
                         if let Ok(n) = HeaderName::from_bytes(name_str.as_bytes()) {
                             axum_headers.insert(n, value);
                         }
                    }
                }
            }
            return Ok(axum_res);
        }
    }
    
    let res_stream = res.bytes_stream();
    let body = Body::from_stream(res_stream);

    let mut axum_res = Response::builder()
        .status(status_code)
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
    let axum_headers = axum_res.headers_mut();
    for (name, value) in res_headers.iter() {
        if let Ok(value) = HeaderValue::from_bytes(value.as_bytes()) {
            let name_str = name.as_str().to_string();
            if !hop_by_hop.contains(&name_str) {
                 if let Ok(n) = HeaderName::from_bytes(name_str.as_bytes()) {
                     axum_headers.insert(n, value);
                 }
            }
        }
    }
    
    Ok(axum_res)
}
