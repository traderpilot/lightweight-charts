// src/routes/health.rs
use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;

pub struct HealthState {
    pub start_time: Instant,
    pub binance_connected: Arc<parking_lot::Mutex<bool>>,
    pub cache_sizes: Arc<parking_lot::Mutex<HashMap<String, usize>>>,
    pub connection_count: Arc<parking_lot::Mutex<usize>>,
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for HealthState {
    fn clone(&self) -> Self {
        Self {
            start_time: self.start_time,
            binance_connected: self.binance_connected.clone(),
            cache_sizes: self.cache_sizes.clone(),
            connection_count: self.connection_count.clone(),
        }
    }
}

impl HealthState {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            binance_connected: Arc::new(parking_lot::Mutex::new(false)),
            cache_sizes: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            connection_count: Arc::new(parking_lot::Mutex::new(0)),
        }
    }

    pub fn set_binance_connected(&self, connected: bool) {
        *self.binance_connected.lock() = connected;
    }

    pub fn update_cache_size(&self, symbol: &str, size: usize) {
        self.cache_sizes.lock().insert(symbol.to_string(), size);
    }

    pub fn increment_connections(&self) {
        *self.connection_count.lock() += 1;
    }

    pub fn decrement_connections(&self) {
        let mut count = self.connection_count.lock();
        if *count > 0 {
            *count -= 1;
        }
    }
}

pub async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().timestamp(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub async fn ready_check(state: axum::extract::State<HealthState>) -> axum::Json<serde_json::Value> {
    let is_connected = *state.binance_connected.lock();
    let cache_sizes = state.cache_sizes.lock();
    let has_cached_data = !cache_sizes.is_empty();

    if is_connected && has_cached_data {
        axum::Json(serde_json::json!({
            "status": "ready",
            "binance_connected": true,
            "symbols_tracked": cache_sizes.len(),
        }))
    } else {
        axum::Json(serde_json::json!({
            "status": "not_ready",
            "binance_connected": is_connected,
            "has_cache_data": has_cached_data,
        }))
    }
}

pub async fn metrics_handler() -> impl axum::response::IntoResponse {
    let metrics = crate::metrics::gather_metrics();
    axum::response::Response::builder()
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(axum::body::Body::from(metrics))
        .unwrap()
}

#[derive(serde::Serialize)]
pub struct ErrorEntry {
    pub timestamp: u64,
    pub error: String,
    pub payload: String,
}

pub async fn diagnostics_errors() -> axum::Json<serde_json::Value> {
    let errors = crate::ws::binance_listener::get_dead_letter_queue();
    let error_rate = crate::ws::binance_listener::get_error_rate();
    
    let error_list: Vec<ErrorEntry> = errors.into_iter().map(|e| ErrorEntry {
        timestamp: e.timestamp,
        error: e.error,
        payload: e.payload,
    }).collect();
    
    axum::Json(serde_json::json!({
        "error_count": error_list.len(),
        "error_rate_percent": error_rate,
        "errors": error_list,
    }))
}

pub fn create_health_router(health_state: HealthState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(ready_check))
        .route("/metrics", get(metrics_handler))
        .route("/api/diagnostics/errors", get(diagnostics_errors))
        .with_state(health_state)
}