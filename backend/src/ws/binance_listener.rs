// src/ws/binance_listener.rs
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as TungsteniteMessage};
use url::Url;
use std::sync::Arc;
use crate::AppState;
use crate::models::candle::Candle;
use crate::models::indicators::update_indicators_last;
use crate::models::binance::*;
use crate::channels::MarketData;
use crate::metrics::{CANDLES_PROCESSED, PARSE_ERRORS, BINANCE_RECONNECTS, BINANCE_CONNECTED, CIRCUIT_BREAKER_STATE};
use crate::ws::circuit_breaker::CircuitBreaker;
use std::collections::VecDeque;
use futures::StreamExt;
use std::time::Instant;
use std::sync::atomic::{AtomicU64, Ordering};
use lazy_static::lazy_static;
use parking_lot::Mutex;

static CIRCUIT_BREAKER: once_cell::sync::Lazy<CircuitBreaker> = once_cell::sync::Lazy::new(|| {
    CircuitBreaker::new(3, 30)
});

lazy_static! {
    static ref DEAD_LETTER_QUEUE: Mutex<Vec<DeadLetterEntry>> = Mutex::new(Vec::new());
    static ref ERROR_COUNT: AtomicU64 = AtomicU64::new(0);
    static ref TOTAL_MESSAGES: AtomicU64 = AtomicU64::new(0);
}

#[derive(Clone)]
pub struct DeadLetterEntry {
    pub timestamp: u64,
    pub error: String,
    pub payload: String,
}

pub fn get_dead_letter_queue() -> Vec<DeadLetterEntry> {
    DEAD_LETTER_QUEUE.lock().iter().cloned().collect()
}

pub fn get_error_rate() -> f64 {
    let total = TOTAL_MESSAGES.load(Ordering::Relaxed);
    let errors = ERROR_COUNT.load(Ordering::Relaxed);
    if total == 0 {
        return 0.0;
    }
    (errors as f64 / total as f64) * 100.0
}

fn calculate_backoff(attempt: u32) -> tokio::time::Duration {
    // Exponential backoff with jitter: 1s, 2s, 4s, 8s, 16s, 32s, 60s (max)
    const MAX_BACKOFF_SECS: u64 = 60;
    let base_secs = 1_u64;
    let backoff_secs = base_secs.saturating_mul(2_u64.pow(attempt.min(6)));
    let capped_secs = backoff_secs.min(MAX_BACKOFF_SECS);
    let jitter_ms = fastrand::u64(200..1200);

    tokio::time::Duration::from_secs(capped_secs) + tokio::time::Duration::from_millis(jitter_ms)
}

fn add_to_dead_letter_queue(error: String, payload: String) {
    let mut queue = DEAD_LETTER_QUEUE.lock();
    queue.push(DeadLetterEntry {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        error,
        payload,
    });
    // Keep max 100 errors
    while queue.len() > 100 {
        queue.remove(0);
    }
}

fn get_next_sequence(state: &AppState, symbol: &str) -> u64 {
    let tracker = state.sequence_tracker.lock();
    let entry = tracker.entry(symbol.to_string());
    let mut entry = entry.or_insert(0);
    *entry += 1;
    *entry
}

pub async fn start_binance_listener(state: AppState) {
    let symbols = vec!["btcusdt", "ethusdt", "solusdt"];
    let streams: Vec<String> = symbols.iter().map(|s| format!("{}@kline_1m", s)).collect();
    let stream_url = format!("wss://stream.binance.com:9443/stream?streams={}", streams.join("/"));

    let mut reconnect_attempt = 0u32;

    loop {
        // Check circuit breaker before attempting connection
        if !CIRCUIT_BREAKER.can_execute() {
            tracing::warn!("Circuit breaker open, waiting before retry...");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            continue;
        }

        let url = Url::parse(&stream_url).unwrap();
        let (ws_stream, _) = match connect_async(url).await {
            Ok(conn) => {
                // Reset reconnect counter on successful connection
                reconnect_attempt = 0;
                BINANCE_CONNECTED.set(1);
                CIRCUIT_BREAKER.record_success();
                CIRCUIT_BREAKER_STATE.set(0);
                tracing::info!("Connected to Binance WebSocket");
                conn
            },
            Err(e) => {
                CIRCUIT_BREAKER.record_failure();
                let state = CIRCUIT_BREAKER.state();
                let state_val = match state {
                    "closed" => 0,
                    "open" => 1,
                    "half_open" => 2,
                    _ => 0,
                };
                CIRCUIT_BREAKER_STATE.set(state_val);
                
                let backoff = calculate_backoff(reconnect_attempt);
                BINANCE_RECONNECTS.inc();
                BINANCE_CONNECTED.set(0);
                tracing::warn!(attempt = reconnect_attempt, error = ?e, delay = ?backoff, "Failed to connect to Binance, retrying");
                tokio::time::sleep(backoff).await;
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                continue;
            }
        };

        let (mut _write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            let _start = Instant::now();
            match msg {
                Ok(TungsteniteMessage::Text(text)) => {
                    // Parse using serde structs for better error handling
                    let parsed: Result<BinanceStreamMessage, _> = serde_json::from_str(&text);
                    match parsed {
                        Ok(data) => {
                            let symbol = data.data.kline.symbol.clone();
                            let symbol_lower = symbol.to_lowercase();
                            
                            // Only process closed klines to avoid duplicate updates
                            if !data.data.kline.is_closed {
                                continue;
                            }
                            
                            let time = data.data.kline.start_time / 1000;
                            let open = match data.data.kline.open.parse::<f64>() {
                                Ok(val) => val,
                                Err(e) => {
                                    PARSE_ERRORS.inc();
                                    tracing::error!(error = %e, "Failed to parse open price");
                                    continue;
                                }
                            };
                            let high = match data.data.kline.high.parse::<f64>() {
                                Ok(val) => val,
                                Err(e) => {
                                    PARSE_ERRORS.inc();
                                    tracing::error!(error = %e, "Failed to parse high price");
                                    continue;
                                }
                            };
                            let low = match data.data.kline.low.parse::<f64>() {
                                Ok(val) => val,
                                Err(e) => {
                                    PARSE_ERRORS.inc();
                                    tracing::error!(error = %e, "Failed to parse low price");
                                    continue;
                                }
                            };
                            let close = match data.data.kline.close.parse::<f64>() {
                                Ok(val) => val,
                                Err(e) => {
                                    PARSE_ERRORS.inc();
                                    tracing::error!(error = %e, "Failed to parse close price");
                                    continue;
                                }
                            };

                            // Create market data (zero-copy Arc) with sequence number
                            let sequence = get_next_sequence(&state, &symbol);
                            let market_data = Arc::new(MarketData {
                                symbol,
                                sequence,
                                time,
                                open,
                                high,
                                low,
                                close,
                            });

                            // Track metrics
                            CANDLES_PROCESSED.inc();
                            tracing::debug!(symbol = %symbol_lower, time = time, "Processed new candle");

                            // Update cache with lock-free message
                            // Initialize if not exists
                            state.candles_cache.entry(symbol_lower.clone()).or_insert_with(VecDeque::new);

                            if let Some(mut candles) = state.candles_cache.get_mut(&symbol_lower) {
                                let len = candles.len();
                                if len > 0 {
                                    let last_time = candles.back().unwrap().time;
                                    if time == last_time {
                                        // Update existing candle
                                        if let Some(last) = candles.back_mut() {
                                            last.open = open;
                                            last.high = high;
                                            last.low = low;
                                            last.close = close;
                                        }
                                    } else {
                                        // Add new candle
                                        let new_candle = Candle {
                                            time,
                                            open,
                                            high,
                                            low,
                                            close,
                                            volume: 0.0,
                                            rsi: None,
                                            ema12: None,
                                            ema26: None,
                                            macd: None,
                                            signal: None,
                                            histogram: None,
                                            bollinger_upper: None,
                                            bollinger_middle: None,
                                            bollinger_lower: None,
                                            stoch_k: None,
                                            stoch_d: None,
                                        };
                                        candles.push_back(new_candle);
                                        // Limit size
                                        while candles.len() > crate::MAX_CANDLES {
                                            candles.pop_front();
                                        }
                                    }
                                    // Update indicators incrementally
                                    update_indicators_last(&mut candles);
                                } else {
                                    // First candle for this symbol
                                    let new_candle = Candle {
                                        time,
                                        open,
                                        high,
                                        low,
                                        close,
                                        volume: 0.0,
                                        rsi: None,
                                        ema12: None,
                                        ema26: None,
                                        macd: None,
                                        signal: None,
                                        histogram: None,
                                        bollinger_upper: None,
                                        bollinger_middle: None,
                                        bollinger_lower: None,
                                        stoch_k: None,
                                        stoch_d: None,
                                    };
                                    candles.push_back(new_candle);
                                    // Update indicators for the first candle
                                    update_indicators_last(&mut candles);
                                }

                                let serialized: Vec<Candle> = candles.iter().cloned().collect();
                                if let Err(e) = state.db.save_candle_history(&symbol_lower, &serialized) {
                                    tracing::error!(symbol = %symbol_lower, error = %e, "Failed to persist candle history");
                                }

                                state.health_state.update_cache_size(&symbol_lower, candles.len());
                            }

                            // Send to all connected clients via their individual channels
                            let client_senders = state.client_senders.lock().await;
                            for sender in client_senders.iter() {
                                let _ = sender.send(market_data.clone());
                            }
                        }
Err(e) => {
                            TOTAL_MESSAGES.fetch_add(1, Ordering::Relaxed);
                            ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                            add_to_dead_letter_queue(format!("Parse error: {}", e), text.clone());
                            PARSE_ERRORS.inc();
                            tracing::error!(error = %e, "Failed to parse Binance message");
                            continue;
                        }
                    }
                }
Err(e) => {
                    tracing::error!(error = %e, "WebSocket error from Binance");
                    BINANCE_CONNECTED.set(0);
                    break;
                }
                _ => {}
            }
        }
        
        // Connection lost, prepare for reconnection
        tracing::warn!("Binance connection lost, reconnecting...");
        let backoff = calculate_backoff(reconnect_attempt);
        tokio::time::sleep(backoff).await;
        reconnect_attempt = reconnect_attempt.saturating_add(1);
    }
}