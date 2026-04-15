use crate::models::candle::Candle;
use dashmap::DashMap;
use serde_json;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct DbStore {
    cache: Arc<DashMap<String, Vec<Candle>>>,
    storage_file: PathBuf,
    dirty: Arc<AtomicBool>,
}

impl DbStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let path = path.as_ref();
        fs::create_dir_all(path)?;

        let storage_file = path.join("candles.json");
        let cache = Arc::new(DashMap::new());

        if storage_file.exists() {
            let bytes = fs::read(&storage_file)?;
            let data: HashMap<String, Vec<Candle>> = serde_json::from_slice(&bytes)?;
            for (symbol, candles) in data {
                cache.insert(symbol, candles);
            }
        }

        Ok(Self {
            cache,
            storage_file,
            dirty: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn save_candle_history(&self, symbol: &str, candles: &[Candle]) -> Result<(), Box<dyn Error>> {
        self.cache.insert(symbol.to_string(), candles.to_vec());
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn load_candle_history(&self, symbol: &str) -> Result<Vec<Candle>, Box<dyn Error>> {
        Ok(self
            .cache
            .get(symbol)
            .map(|entry| entry.value().clone())
            .unwrap_or_default())
    }

    pub fn list_symbols(&self) -> Vec<String> {
        self.cache.iter().map(|entry| entry.key().clone()).collect()
    }

    pub fn flush(&self) -> Result<(), Box<dyn Error>> {
        if self.dirty.swap(false, Ordering::Relaxed) {
            self.persist_all()?;
        }
        Ok(())
    }

    fn persist_all(&self) -> Result<(), Box<dyn Error>> {
        let data: HashMap<String, Vec<Candle>> = self
            .cache
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        let bytes = serde_json::to_vec(&data)?;
        fs::write(&self.storage_file, bytes)?;
        Ok(())
    }
}
