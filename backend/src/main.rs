mod models;
mod indicator;
mod routes;
mod ws;
mod services;
mod trading;
mod backtest;
mod optimizer;
mod channels;
mod metrics;
mod middleware;
mod auth;
mod utils;
mod db;

use std::sync::Arc;
use std::time::Duration;
use db::DbStore;
use std::collections::VecDeque;
use dashmap::DashMap;

use axum::{
    routing::get,
    Router,
    extract::ws::WebSocketUpgrade,
    response::IntoResponse,
};
use axum::http::HeaderValue;
use tokio::{net::TcpListener, sync::Notify};
use tower_http::cors::{CorsLayer, Any, AllowOrigin};

use routes::market::{get_candles_rate_limited, get_order_book, get_recent_trades};
use routes::trading::{TradingState, create_router};
use routes::health::{create_health_router, HealthState};
use routes::auth::create_auth_router;
use ws::handler::{handle_socket, SubscriptionPreferences};
use channels::MarketData;
use tokio::signal;
use middleware::RateLimiter;

const MAX_CANDLES: usize = 500;

#[derive(Clone)]
pub struct AppState {
    pub candles_cache: Arc<DashMap<String, VecDeque<crate::models::candle::Candle>>>,
    pub client_senders: Arc<tokio::sync::Mutex<Vec<tokio::sync::mpsc::UnboundedSender<Arc<MarketData>>>>>,
    pub sequence_tracker: Arc<parking_lot::Mutex<DashMap<String, u64>>>,
    pub health_state: crate::routes::health::HealthState,
    pub shutdown_signal: Arc<parking_lot::Mutex<bool>>,
    pub candles_rate_limiter: Arc<middleware::RateLimiter>,
    pub strategies_rate_limiter: Arc<middleware::RateLimiter>,
    pub db: Arc<DbStore>,
}

#[tokio::main]
async fn main() {
    // Initialize tracing for structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    tracing::info!("Starting lightweight-charts-backend v{}", env!("CARGO_PKG_VERSION"));

    // Initialize Prometheus metrics
    if let Err(e) = metrics::init_metrics() {
        tracing::error!("Failed to initialize metrics: {}", e);
    }
    tracing::info!("Metrics initialized");

    // Create health state
    let health_state = HealthState::new();
    let health_state_clone = health_state.clone();

    let db = Arc::new(DbStore::open("data/persistence").expect("Failed to open persistence storage"));

    let db_flush = db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(e) = db_flush.flush() {
                tracing::error!(error = %e, "Failed periodic persistence flush");
            }
        }
    });

    // Create app state with DashMap and sequence tracker
    let state = AppState {
        candles_cache: Arc::new(DashMap::new()),
        client_senders: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        sequence_tracker: Arc::new(parking_lot::Mutex::new(DashMap::new())),
        health_state: health_state_clone,
        shutdown_signal: Arc::new(parking_lot::Mutex::new(false)),
        candles_rate_limiter: Arc::new(RateLimiter::new(1000, 1)),
        strategies_rate_limiter: Arc::new(RateLimiter::new(100, 1)),
        db: db.clone(),
    };

    for symbol in db.list_symbols() {
        if let Ok(candles) = db.load_candle_history(&symbol) {
            if !candles.is_empty() {
                state.candles_cache.insert(symbol.clone(), VecDeque::from(candles.clone()));
                state.health_state.update_cache_size(&symbol, candles.len());
                tracing::info!(symbol = %symbol, count = candles.len(), "Loaded candle history from persistence");
            }
        }
    }

    // Create trading state
    let trading_state = TradingState::new(state.clone());

    // Start Binance WebSocket listener for multiple symbols
    let state_clone = state.clone();
    tokio::spawn(async move {
        ws::binance_listener::start_binance_listener(state_clone).await;
    });

    let cors = if std::env::var("CORS_PERMISSIVE").is_ok() {
        tracing::warn!("CORS is PERMISSIVE - only use in development!");
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(AllowOrigin::list([
                HeaderValue::from_static("https://trading.example.com"),
                HeaderValue::from_static("https://app.example.com"),
            ]))
            .allow_methods(Any)
            .allow_headers(Any)
    };

    let app = Router::new()
        .merge(create_health_router(health_state))
        .merge(create_auth_router())
        .nest("/api/trading", create_router(trading_state.clone()))
        .route("/api/candles", get({
            let state = state.clone();
            move |query, connect_info| get_candles_rate_limited(query, state, connect_info)
        }))
        .route("/api/orderbook", get(get_order_book))
        .route("/api/trades", get(get_recent_trades))
        .route("/ws", get({
            let state = state.clone();
            move |ws: WebSocketUpgrade, query| async move { ws_handler(ws, query, state).await }
        }))
        .layer(cors);

    tracing::info!("Server running at http://localhost:3000");

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    
    // Set up signal handlers for graceful shutdown
    let shutdown = state.shutdown_signal.clone();
    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_notify_clone = shutdown_notify.clone();

    tokio::spawn(async move {
        let mut term_signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to bind SIGTERM handler");

        tokio::select! {
            _ = signal::ctrl_c() => {
                tracing::info!("Received Ctrl-C, shutting down gracefully...");
            }
            _ = term_signal.recv() => {
                tracing::info!("Received SIGTERM, shutting down gracefully...");
            }
        }

        *shutdown.lock() = true;
        shutdown_notify_clone.notify_waiters();
    });

    let shutdown_check = shutdown_notify.clone();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_check.notified().await;
            tracing::info!("Shutting down server...");
        });

    match tokio::time::timeout(tokio::time::Duration::from_secs(30), server).await {
        Ok(res) => {
            res.unwrap();
            tracing::info!("Server shutdown completed within 30 seconds");
        }
        Err(_) => {
            tracing::warn!("Graceful shutdown timed out after 30 seconds, forcing exit");
        }
    }

    if let Err(e) = state.db.flush() {
        tracing::error!("Failed to flush persistence storage on shutdown: {}", e);
    } else {
        tracing::info!("Persistence storage flushed successfully on shutdown");
    }

    let metrics_dump = metrics::flush_metrics();
    tracing::info!("Metrics flushed on shutdown, {} bytes", metrics_dump.len());

    tracing::info!("Server shutdown complete");
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    state: AppState
) -> impl IntoResponse {
    let symbol = params.get("symbol").cloned().unwrap_or_else(|| "btcusdt".to_string());
    let subscription = SubscriptionPreferences::from_query(symbol.clone(), &params);
    ws.on_upgrade(move |socket| handle_socket(socket, subscription, state))
}