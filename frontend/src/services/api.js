const API_BASE = import.meta.env.VITE_API_BASE || '/api'

export const api = {
  // Market endpoints
  getCandles: async (symbol) => {
    const response = await fetch(`${API_BASE}/candles?symbol=${symbol}`)
    return response.json()
  },

  // Trading endpoints
  getStrategies: async () => {
    const response = await fetch(`${API_BASE}/trading/strategies/list`)
    return response.json()
  },

  createStrategy: async (strategyData) => {
    const response = await fetch(`${API_BASE}/trading/strategies`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(strategyData),
    })
    return response.json()
  },

  getSignals: async (symbol) => {
    const response = await fetch(`${API_BASE}/trading/signals?symbol=${symbol}`)
    return response.json()
  },

  deleteStrategy: async (strategyId) => {
    const response = await fetch(`${API_BASE}/trading/strategies/${strategyId}`, {
      method: 'DELETE',
    })
    return response.json()
  },

  toggleStrategy: async (strategyId, enabled) => {
    const response = await fetch(`${API_BASE}/trading/strategies/${strategyId}/toggle`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    })
    return response.json()
  },

  runBacktest: async (backtestData) => {
    const response = await fetch(`${API_BASE}/trading/backtest`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(backtestData),
    })
    return response.json()
  },
}
