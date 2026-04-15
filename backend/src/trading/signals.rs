// src/trading/signals.rs
use crate::models::candle::Candle;
use crate::models::orders::{Signal, SignalIndicators};
use std::collections::VecDeque;

pub struct SignalGenerator;

impl SignalGenerator {
    /// Generate trading signal based on multiple indicators
    pub fn generate_signal(symbol: &str, candles: &VecDeque<Candle>, price: f64) -> Option<Signal> {
        if candles.len() < 26 {
            return None; // Need 26 candles for MACD
        }

        let last_candle = candles.back()?;

        // Get previous candle for crossovers
        let prev_candle = if candles.len() >= 2 {
            candles.iter().rev().nth(1)
        } else {
            None
        };

        let rsi = last_candle.rsi;
        let _macd_signal = last_candle.macd.is_some() && last_candle.signal.is_some();
        let ema_signal = last_candle.ema12.is_some() && last_candle.ema26.is_some();

        let mut confidence: f64 = 0.0;
        let mut buy_score: f64 = 0.0;
        let mut sell_score: f64 = 0.0;

        // RSI analysis
        if let Some(rsi_val) = rsi {
            if rsi_val < 30.0 {
                buy_score += 2.0; // Oversold
            } else if rsi_val < 40.0 {
                buy_score += 1.0;
            } else if rsi_val > 70.0 {
                sell_score += 2.0; // Overbought
            } else if rsi_val > 60.0 {
                sell_score += 1.0;
            }
        }

        // MACD analysis (Golden Cross / Death Cross)
        if let (Some(macd), Some(signal), Some(prev)) =
            (last_candle.macd, last_candle.signal, prev_candle)
        {
            let prev_macd = prev.macd.unwrap_or(macd);
            let prev_signal = prev.signal.unwrap_or(signal);

            if prev_macd <= prev_signal && macd > signal {
                buy_score += 3.0; // Golden cross
            } else if prev_macd >= prev_signal && macd < signal {
                sell_score += 3.0; // Death cross
            }
        }

        // EMA analysis (EMA 12-26 crossover)
        if ema_signal {
            if let (Some(ema12), Some(ema26), Some(prev)) =
                (last_candle.ema12, last_candle.ema26, prev_candle)
            {
                let prev_ema12 = prev.ema12.unwrap_or(ema12);
                let prev_ema26 = prev.ema26.unwrap_or(ema26);

                if prev_ema12 <= prev_ema26 && ema12 > ema26 {
                    buy_score += 2.0; // Bullish cross
                } else if prev_ema12 >= prev_ema26 && ema12 < ema26 {
                    sell_score += 2.0; // Bearish cross
                }

                // Trend strength
                if ema12 > ema26 {
                    buy_score += 0.5;
                } else if ema26 > ema12 {
                    sell_score += 0.5;
                }
            }
        }

        // Price position analysis
        if let Some(ema12) = last_candle.ema12 {
            if price > ema12 * 1.01 {
                buy_score += 0.5; // Price above EMA
            } else if price < ema12 * 0.99 {
                sell_score += 0.5; // Price below EMA
            }
        }

        // Normalize scores
        let total_score = buy_score + sell_score;
        if total_score > 0.0 {
            confidence = (buy_score.max(sell_score) / total_score).min(1.0);
        }

        let indicators = SignalIndicators {
            rsi,
            macd_signal: last_candle
                .macd
                .map(|m| m > last_candle.signal.unwrap_or(m)),
            ema_signal: last_candle
                .ema12
                .map(|e12| e12 > last_candle.ema26.unwrap_or(e12)),
            price,
        };

        // Generate signal
        if buy_score > sell_score && buy_score > 2.0 && confidence > 0.5 {
            Some(Signal::new_buy(symbol.to_string(), confidence, indicators))
        } else if sell_score > buy_score && sell_score > 2.0 && confidence > 0.5 {
            Some(Signal::new_sell(symbol.to_string(), confidence, indicators))
        } else {
            None
        }
    }

    /// Analyze multiple timeframes for signal strength
    pub fn multi_timeframe_analysis(
        symbol: &str,
        short_candles: &VecDeque<Candle>,
        long_candles: &VecDeque<Candle>,
        price: f64,
    ) -> Option<Signal> {
        let short_signal = Self::generate_signal(symbol, short_candles, price);
        let long_signal = Self::generate_signal(symbol, long_candles, price);

        // Both timeframes agree = stronger signal
        match (short_signal, long_signal) {
            (Some(short), Some(long)) if short.signal_type == long.signal_type => {
                let mut signal = short.clone();
                signal.confidence = (short.confidence + long.confidence) / 2.0;
                Some(signal)
            }
            (Some(s), _) => Some(s),
            (None, Some(l)) => Some(l),
            _ => None,
        }
    }

    /// Momentum-based signal (rate of change)
    pub fn momentum_signal(
        symbol: &str,
        candles: &VecDeque<Candle>,
        price: f64,
        period: usize,
    ) -> Option<Signal> {
        if candles.len() < period + 1 {
            return None;
        }

        let current_close = candles.back()?.close;
        let past_close = candles.iter().rev().nth(period)?.close;

        let momentum = ((current_close - past_close) / past_close) * 100.0;
        let confidence: f64 = (momentum.abs() / 5.0).min(1.0);

        let last_candle = candles.back()?;
        let signal_val = last_candle
            .signal
            .unwrap_or(last_candle.macd.unwrap_or(0.0));
        let macd_gt_signal = last_candle.macd.map(|m| m > signal_val);

        let indicators = SignalIndicators {
            rsi: last_candle.rsi,
            macd_signal: macd_gt_signal,
            ema_signal: None,
            price,
        };

        if momentum > 1.5 {
            Some(Signal::new_buy(symbol.to_string(), confidence, indicators))
        } else if momentum < -1.5 {
            Some(Signal::new_sell(symbol.to_string(), confidence, indicators))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_candle(
        close: f64,
        rsi: Option<f64>,
        ema12: Option<f64>,
        ema26: Option<f64>,
        macd: Option<f64>,
        signal: Option<f64>,
    ) -> Candle {
        Candle {
            time: 1234567890,
            open: close * 0.99,
            high: close * 1.01,
            low: close * 0.98,
            close,
            volume: 1000.0,
            rsi,
            ema12,
            ema26,
            macd,
            signal,
            histogram: None,
            bollinger_upper: None,
            bollinger_middle: None,
            bollinger_lower: None,
            stoch_k: None,
            stoch_d: None,
        }
    }

    #[test]
    fn test_rsi_extreme_signal() {
        let mut candles = VecDeque::new();

        // Create candles with RSI in oversold territory (< 30)
        for i in 0..26 {
            let rsi_val = if i < 25 { Some(50.0) } else { Some(25.0) };
            candles.push_back(create_test_candle(
                50.0 + i as f64,
                rsi_val,
                Some(50.0),
                Some(48.0),
                None,
                None,
            ));
        }

        let signal = SignalGenerator::generate_signal("BTCUSDT", &candles, 75.0);
        assert!(signal.is_some());
        let sig = signal.unwrap();
        assert_eq!(sig.signal_type, SignalType::StrongBuy); // High confidence due to oversold RSI + bullish EMA cross
        assert!(sig.confidence > 0.5);
    }

    #[test]
    fn test_crossover_signal_generation() {
        let mut candles = VecDeque::new();

        // Create candles that build up to an EMA12 > EMA26 crossover
        for i in 0..26 {
            let ema12 = if i < 25 { Some(48.0) } else { Some(51.0) }; // Crosses above
            let ema26 = Some(50.0);
            let macd = if i < 25 { Some(-2.0) } else { Some(1.0) };
            let sig = if i < 25 { Some(-1.0) } else { Some(2.0) };
            candles.push_back(create_test_candle(
                50.0 + i as f64 * 0.1,
                Some(50.0),
                ema12,
                ema26,
                macd,
                sig,
            ));
        }

        let signal = SignalGenerator::generate_signal("BTCUSDT", &candles, 76.0);
        assert!(signal.is_some(), "Expected signal on crossover");
        let sig = signal.unwrap();
        assert_eq!(sig.signal_type, SignalType::StrongBuy); // High confidence due to bullish EMA cross
        assert!(sig.confidence > 0.5);
    }

    #[test]
    fn test_no_signal_insufficient_candles() {
        let candles = VecDeque::new();
        let signal = SignalGenerator::generate_signal("BTCUSDT", &candles, 50.0);
        assert!(signal.is_none());
    }

    #[test]
    fn test_signal_requires_minimum_score() {
        let mut candles = VecDeque::new();

        // Create neutral candles (no strong signals)
        for i in 0..26 {
            candles.push_back(create_test_candle(
                50.0,
                Some(50.0),
                Some(50.0),
                Some(50.0),
                Some(0.0),
                Some(0.0),
            ));
        }

        let signal = SignalGenerator::generate_signal("BTCUSDT", &candles, 50.0);
        // Neutral conditions should not generate signal
        assert!(signal.is_none());
    }
}
