// src/routes/trading.rs
use axum::{
    extract::{Query, State, Json},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;

use crate::trading::{
    StrategyConfig, StrategyManager, StrategyType,
};
use crate::models::orders::Signal;

#[derive(Clone)]
pub struct TradingState {
    pub strategy_manager: Arc<Mutex<StrategyManager>>,
    pub signals: Arc<Mutex<Vec<Signal>>>,
    pub app_state: crate::AppState,
}

impl TradingState {
    pub fn new(app_state: crate::AppState) -> Self {
        TradingState {
            strategy_manager: Arc::new(Mutex::new(StrategyManager::new())),
            signals: Arc::new(Mutex::new(Vec::new())),
            app_state,
        }
    }
}

// Request/Response types
#[derive(Debug, Deserialize)]
pub struct CreateStrategyRequest {
    pub name: String,
    pub strategy_type: String,
    pub symbol: String,
    pub risk_percent: Option<f64>,
    pub stop_loss_pct: Option<f64>,
    pub take_profit_pct: Option<f64>,
    pub max_positions: Option<usize>,
}

impl CreateStrategyRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("name must be between 1 and 100 characters".to_string());
        }
        
        if let Some(risk) = self.risk_percent {
            if risk < 0.1 || risk > 100.0 {
                return Err("risk_percent must be between 0.1 and 100.0".to_string());
            }
        }
        
        if self.symbol.is_empty() || self.symbol.len() > 20 {
            return Err("symbol must be between 1 and 20 characters".to_string());
        }
        
        if !self.symbol.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            return Err("symbol must contain only uppercase letters and digits".to_string());
        }
        
        if let Some(max_pos) = self.max_positions {
            if max_pos < 1 || max_pos > 10 {
                return Err("max_positions must be between 1 and 10".to_string());
            }
        }
        
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateStrategyRequest {
    pub risk_percent: Option<f64>,
    pub stop_loss_pct: Option<f64>,
    pub take_profit_pct: Option<f64>,
    pub max_positions: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct StrategyResponse {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub config: StrategyConfig,
}

#[derive(Debug, Deserialize)]
pub struct BacktestRequest {
    pub dsl: String,
    pub buy_condition: String,
    pub sell_condition: String,
    pub symbol: String,
}

#[derive(Debug, Serialize)]
pub struct BacktestResponse {
    pub total_trades: usize,
    pub win_rate: f64,
    pub total_profit: f64,
    pub max_drawdown: f64,
    pub trades: Vec<crate::backtest::types::Trade>,
}

#[derive(Debug, Deserialize)]
pub struct OptimizerRequest {
    pub dsl: String,
    pub symbol: String,
    pub population_size: Option<usize>,
    pub generations: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct OptimizerResponse {
    pub rsi_period: usize,
    pub buy_condition: String,
    pub sell_condition: String,
    pub score: f64,
    pub total_trades: usize,
    pub win_rate: f64,
    pub total_profit: f64,
    pub max_drawdown: f64,
}

#[derive(Debug, Serialize)]
pub struct SignalResponse {
    pub symbol: String,
    pub signal_type: String,
    pub confidence: f64,
    pub timestamp: i64,
}

// Handlers
pub async fn create_strategy(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    Json(req): Json<CreateStrategyRequest>,
) -> Result<Json<StrategyResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;

    if let Err(e) = req.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }

    let strategy_type = match req.strategy_type.as_str() {
        "moving_average_crossover" => StrategyType::MovingAverageCrossover,
        "rsi_momentum" => StrategyType::RSIMomentum,
        "macd_crossover" => StrategyType::MACDCrossover,
        "multi_indicator" => StrategyType::MultiIndicator,
        _ => StrategyType::Custom(req.strategy_type.clone()),
    };

    let mut config = StrategyConfig::new(req.name.clone(), strategy_type, req.symbol.clone());
    config.owner_id = Some(user_id.clone());

    if let Some(risk) = req.risk_percent {
        config.risk_percent = risk;
    }
    if let Some(stop_loss) = req.stop_loss_pct {
        config.stop_loss_pct = stop_loss;
    }
    if let Some(take_profit) = req.take_profit_pct {
        config.take_profit_pct = take_profit;
    }
    if let Some(max_pos) = req.max_positions {
        config.max_positions = max_pos;
    }

    let id = config.id.clone();
    trading_state
        .strategy_manager
        .lock()
        .add_strategy(config.clone());

    Ok(Json(StrategyResponse {
        id,
        name: req.name,
        enabled: true,
        config,
    }))
}

pub async fn list_strategies(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
) -> Result<Json<Vec<StrategyConfig>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;
    let strategies = trading_state
        .strategy_manager
        .lock()
        .list_user_strategies(&user_id);
    Ok(Json(strategies))
}

pub async fn get_strategy(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Option<StrategyConfig>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;

    if let Some(id) = params.get("id") {
        if let Some(strategy) = trading_state
            .strategy_manager
            .lock()
            .get_user_strategy(id, &user_id)
        {
            return Ok(Json(Some(strategy.config.clone())));
        }
    }

    Ok(Json(None))
}

pub async fn enable_strategy(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;
    if let Some(id) = params.get("id") {
        if trading_state
            .strategy_manager
            .lock()
            .enable_strategy_for_user(id, &user_id)
        {
            return Ok(Json(json!({"status": "enabled"})));
        }
        return Err((StatusCode::FORBIDDEN, Json(json!({"error": "Unauthorized or unknown strategy"}))));
    }
    Err((StatusCode::BAD_REQUEST, Json(json!({"error": "No ID provided"}))))
}

pub async fn disable_strategy(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;
    if let Some(id) = params.get("id") {
        if trading_state
            .strategy_manager
            .lock()
            .disable_strategy_for_user(id, &user_id)
        {
            return Ok(Json(json!({"status": "disabled"})));
        }
        return Err((StatusCode::FORBIDDEN, Json(json!({"error": "Unauthorized or unknown strategy"}))));
    }
    Err((StatusCode::BAD_REQUEST, Json(json!({"error": "No ID provided"}))))
}

pub async fn delete_strategy(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;
    if let Some(id) = params.get("id") {
        if trading_state
            .strategy_manager
            .lock()
            .remove_strategy_for_user(id, &user_id)
        {
            return Ok(Json(json!({"status": "deleted"})));
        }
        return Err((StatusCode::FORBIDDEN, Json(json!({"error": "Unauthorized or unknown strategy"}))));
    }
    Err((StatusCode::BAD_REQUEST, Json(json!({"error": "No ID provided"}))))
}

pub async fn get_strategy_stats(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
) -> Result<Json<Vec<(String, usize, usize, f64, f64)>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;
    let stats = trading_state
        .strategy_manager
        .lock()
        .get_user_stats(&user_id);
    Ok(Json(stats))
}

pub async fn run_backtest(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    Json(req): Json<BacktestRequest>,
) -> Result<Json<BacktestResponse>, (StatusCode, Json<serde_json::Value>)> {
    let _user_id = crate::auth::require_user_id(&headers)?;

    // Parse DSL
    let def: crate::indicator::dsl::IndicatorDef = serde_json::from_str(&req.dsl)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid DSL: {}", e)}))))?;

    // Compile indicator
    let compiled = crate::indicator::compiler::compile(def)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Compile error: {}", e)}))))?;

    // Get candles
    let candles = trading_state.app_state.candles_cache.get(&req.symbol)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "No candles for symbol"}))))?;

    let candles_vec: Vec<_> = candles.iter().cloned().collect();

    // Create indicator engine
    let indicator = crate::indicator::engine::IndicatorEngine::new(compiled);

    // Run backtest
    let result = crate::backtest::engine::run_backtest(
        &candles_vec,
        indicator,
        &req.buy_condition,
        &req.sell_condition,
    );

    let response = BacktestResponse {
        total_trades: result.total_trades,
        win_rate: result.win_rate,
        total_profit: result.total_profit,
        max_drawdown: result.max_drawdown,
        trades: result.trades,
    };

    Ok(Json(response))
}

pub async fn run_optimizer(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    Json(req): Json<OptimizerRequest>,
) -> Result<Json<OptimizerResponse>, (StatusCode, Json<serde_json::Value>)> {
    let _user_id = crate::auth::require_user_id(&headers)?;

    let def: crate::indicator::dsl::IndicatorDef = serde_json::from_str(&req.dsl)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid DSL: {}", e)}))))?;

    let compiled = crate::indicator::compiler::compile(def)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Compile error: {}", e)}))))?;

    let candles = trading_state.app_state.candles_cache.get(&req.symbol)
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "No candles for symbol"}))))?;

    let candles_vec: Vec<_> = candles.iter().cloned().collect();
    let population_size = req.population_size.unwrap_or(50).clamp(10, 200);
    let generations = req.generations.unwrap_or(20).clamp(5, 100);

    let result = crate::optimizer::runner::run_optimizer(
        &candles_vec,
        &compiled,
        population_size,
        generations,
    );

    Ok(Json(OptimizerResponse {
        rsi_period: result.best_genome.rsi_period,
        buy_condition: result.buy_condition,
        sell_condition: result.sell_condition,
        score: result.score,
        total_trades: result.backtest.total_trades,
        win_rate: result.backtest.win_rate,
        total_profit: result.backtest.total_profit,
        max_drawdown: result.backtest.max_drawdown,
    }))
}

pub async fn get_signals(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Signal>>, (StatusCode, Json<serde_json::Value>)> {
    let _user_id = crate::auth::require_user_id(&headers)?;
    let signals = trading_state.signals.lock().clone();
    Ok(Json(signals))
}

pub fn create_router(trading_state: TradingState) -> Router {
    Router::new()
        .route("/strategies", post(create_strategy).get(list_strategies))
        .route("/strategies/get", get(get_strategy))
        .route("/strategies/enable", post(enable_strategy))
        .route("/strategies/disable", post(disable_strategy))
        .route("/strategies/delete", post(delete_strategy))
        .route("/strategies/stats", get(get_strategy_stats))
        .route("/backtest", post(run_backtest))
        .route("/optimizer", post(run_optimizer))
        .route("/signals", get(get_signals))
        .with_state(trading_state)
}

// Simpler version that works with current AppState
// Simpler version that works with current AppState
pub async fn create_strategy_simple(
    State(trading_state): State<TradingState>,
    headers: HeaderMap,
    req: axum::extract::Json<CreateStrategyRequest>,
) -> Result<Json<StrategyResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = crate::auth::require_user_id(&headers)?;

    if let Err(e) = req.0.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    
    let strategy_type = match req.0.strategy_type.as_str() {
        "moving_average_crossover" => StrategyType::MovingAverageCrossover,
        "rsi_momentum" => StrategyType::RSIMomentum,
        "macd_crossover" => StrategyType::MACDCrossover,
        "multi_indicator" => StrategyType::MultiIndicator,
        _ => StrategyType::Custom(req.0.strategy_type.clone()),
    };

    let mut config = StrategyConfig::new(req.0.name.clone(), strategy_type, req.0.symbol.clone());
    config.owner_id = Some(user_id.clone());

    if let Some(risk) = req.0.risk_percent {
        config.risk_percent = risk;
    }
    if let Some(stop_loss) = req.0.stop_loss_pct {
        config.stop_loss_pct = stop_loss;
    }
    if let Some(take_profit) = req.0.take_profit_pct {
        config.take_profit_pct = take_profit;
    }
    if let Some(max_pos) = req.0.max_positions {
        config.max_positions = max_pos;
    }

    let id = config.id.clone();
    trading_state
        .strategy_manager
        .lock()
        .add_strategy(config.clone());

    Ok(Json(StrategyResponse {
        id,
        name: req.0.name,
        enabled: true,
        config,
    }))
}
