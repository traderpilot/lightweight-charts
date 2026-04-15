// src/trading/mod.rs
pub mod backtest;
pub mod engine;
pub mod signals;
pub mod strategy;

pub use strategy::{StrategyConfig, StrategyManager, StrategyType};
