// src/routes/market.rs
use axum::{Json, extract::Query};
use crate::models::candle::Candle;
use crate::services::data_service::get_historical_candles;
use crate::AppState;
use crate::MAX_CANDLES;
use crate::models::indicators::IndicatorParams;
use serde::Deserialize;
use std::collections::VecDeque;

#[derive(Deserialize)]
pub struct MarketQuery {
    pub symbol: Option<String>,
    pub interval: Option<String>,
}

#[derive(Deserialize)]
pub struct IndicatorQuery {
    pub symbol: Option<String>,
    pub interval: Option<String>,
    pub rsi_period: Option<usize>,
    pub macd_fast: Option<usize>,
    pub macd_slow: Option<usize>,
    pub bb_period: Option<usize>,
    pub bb_std: Option<f64>,
}

pub async fn get_candles(Query(params): Query<MarketQuery>, state: AppState) -> Json<Vec<Candle>> {
    let symbol = params.symbol.unwrap_or_else(|| "btcusdt".to_string()).to_lowercase();
    let interval = params.interval.unwrap_or_else(|| "1m".to_string());
    
    // Check rate limit per endpoint
    let _client_ip = "default"; // Would be extracted from request in production
    
    // Check cache first (include interval in cache key)
    let cache_key = format!("{}:{}", symbol, interval);
    if let Some(candles) = state.candles_cache.get(&cache_key) {
        return Json(candles.iter().cloned().collect());
    }
    
    // Fetch from API and cache
    match get_historical_candles(&symbol, &interval).await {
        Ok(candles_vec) => {
            let mut deque: VecDeque<Candle> = candles_vec.into_iter().collect();
            // Limit to MAX_CANDLES
            while deque.len() > MAX_CANDLES {
                deque.pop_front();
            }
            let result: Vec<Candle> = deque.iter().cloned().collect();
            state.candles_cache.insert(cache_key, deque);
            Json(result)
        }
        Err(_) => Json(vec![]),
    }
}

pub async fn get_candles_rate_limited(
    Query(params): Query<MarketQuery>,
    state: AppState,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Result<Json<Vec<Candle>>, axum::http::StatusCode> {
    let ip = addr.ip().to_string();
    
    // Check rate limit: 1000 req/sec for candles endpoint
    if !state.candles_rate_limiter.check_ip(&ip) {
        tracing::warn!("Rate limit exceeded for {} on /api/candles", ip);
        return Err(axum::http::StatusCode::TOO_MANY_REQUESTS);
    }
    
    let symbol = params.symbol.unwrap_or_else(|| "btcusdt".to_string()).to_lowercase();
    let interval = params.interval.unwrap_or_else(|| "1m".to_string());
    
    // Check cache first (include interval in cache key)
    let cache_key = format!("{}:{}", symbol, interval);
    if let Some(candles) = state.candles_cache.get(&cache_key) {
        return Ok(Json(candles.iter().cloned().collect()));
    }
    
    match get_historical_candles(&symbol, &interval).await {
        Ok(candles_vec) => {
            let mut deque: VecDeque<Candle> = candles_vec.into_iter().collect();
            while deque.len() > MAX_CANDLES {
                deque.pop_front();
            }
            let result: Vec<Candle> = deque.iter().cloned().collect();
            state.candles_cache.insert(cache_key, deque);
            Ok(Json(result))
        }
        Err(_) => Ok(Json(vec![])),
    }
}

pub async fn get_candles_custom_indicators(
    Query(query): Query<IndicatorQuery>,
    state: AppState,
) -> Result<Json<Vec<Candle>>, axum::http::StatusCode> {
    let symbol = query.symbol.unwrap_or_else(|| "btcusdt".to_string()).to_lowercase();
    let interval = query.interval.unwrap_or_else(|| "1m".to_string());
    
    let mut params = IndicatorParams::default();
    if let Some(rsi) = query.rsi_period { params.rsi_period = rsi; }
    if let Some(fast) = query.macd_fast { params.macd_fast = fast; }
    if let Some(slow) = query.macd_slow { params.macd_slow = slow; }
    if let Some(period) = query.bb_period { params.bb_period = period; }
    if let Some(std) = query.bb_std { params.bb_std = std; }
    
    let cache_key = format!("{}:{}:custom", symbol, interval);
    
    if let Some(candles) = state.candles_cache.get(&cache_key) {
        return Ok(Json(candles.iter().cloned().collect()));
    }
    
    match get_historical_candles(&symbol, &interval).await {
        Ok(mut candles_vec) => {
            crate::models::indicators::calculate_indicators_with_params(&mut candles_vec, &params);
            let mut deque: VecDeque<Candle> = candles_vec.into_iter().collect();
            while deque.len() > MAX_CANDLES {
                deque.pop_front();
            }
            let result: Vec<Candle> = deque.iter().cloned().collect();
            state.candles_cache.insert(cache_key, deque);
            Ok(Json(result))
        }
        Err(_) => Ok(Json(vec![])),
    }
}

#[derive(serde::Serialize)]
pub struct OrderBookEntry {
    pub price: f64,
    pub quantity: f64,
}

#[derive(serde::Serialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
    pub timestamp: u64,
}

pub async fn get_order_book(
    Query(params): Query<MarketQuery>,
) -> Result<Json<OrderBook>, axum::http::StatusCode> {
    let symbol = params.symbol.unwrap_or_else(|| "btcusdt".to_string()).to_uppercase();
    
    let url = format!("https://api.binance.com/api/v3/depth?symbol={}&limit=20", symbol);
    
    match reqwest::get(&url).await {
        Ok(response) => {
            #[derive(Deserialize)]
            struct BinanceBook {
                bids: Vec<Vec<String>>,
                asks: Vec<Vec<String>>,
            }
            
            match response.json::<BinanceBook>().await {
                Ok(book) => {
                    let bids: Vec<OrderBookEntry> = book.bids.iter().take(10).map(|b| OrderBookEntry {
                        price: b[0].parse().unwrap_or(0.0),
                        quantity: b[1].parse().unwrap_or(0.0),
                    }).collect();
                    
                    let asks: Vec<OrderBookEntry> = book.asks.iter().take(10).map(|a| OrderBookEntry {
                        price: a[0].parse().unwrap_or(0.0),
                        quantity: a[1].parse().unwrap_or(0.0),
                    }).collect();
                    
                    Ok(Json(OrderBook {
                        symbol: symbol.to_lowercase(),
                        bids,
                        asks,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    }))
                }
                Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
        Err(_) => Err(axum::http::StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[derive(serde::Serialize)]
pub struct Trade {
    pub id: u64,
    pub price: f64,
    pub quantity: f64,
    pub time: u64,
    pub is_buyer_maker: bool,
}

pub async fn get_recent_trades(
    Query(params): Query<MarketQuery>,
) -> Result<Json<Vec<Trade>>, axum::http::StatusCode> {
    let symbol = params.symbol.unwrap_or_else(|| "btcusdt".to_string()).to_uppercase();
    
    let url = format!("https://api.binance.com/api/v3/trades?symbol={}&limit=50", symbol);
    
    match reqwest::get(&url).await {
        Ok(response) => {
            #[derive(Deserialize)]
            struct BinanceTrade {
                id: u64,
                price: String,
                qty: String,
                time: u64,
                is_buyer_maker: bool,
            }
            
            match response.json::<Vec<BinanceTrade>>().await {
                Ok(trades) => {
                    let result: Vec<Trade> = trades.into_iter().map(|t| Trade {
                        id: t.id,
                        price: t.price.parse().unwrap_or(0.0),
                        quantity: t.qty.parse().unwrap_or(0.0),
                        time: t.time / 1000,
                        is_buyer_maker: t.is_buyer_maker,
                    }).collect();
                    Ok(Json(result))
                }
                Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
        Err(_) => Err(axum::http::StatusCode::SERVICE_UNAVAILABLE),
    }
}