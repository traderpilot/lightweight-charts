import React, { useEffect, useRef, useState, useCallback } from 'react'
import { createChart } from 'lightweight-charts'
import '../styles/ChartPanel.css'

const TIMEFRAMES = ['1m', '5m', '15m', '1h', '4h', '1d']
const STORAGE_KEY = 'trading-dashboard-state'

const DEFAULT_INDICATORS = {
  rsi: true,
  macd: true,
  ema: false,
  bollinger: false,
}

const DEFAULT_PARAMS = {
  rsiPeriod: 14,
  macdFast: 12,
  macdSlow: 26,
  bbPeriod: 20,
  bbStd: 2,
}

const PLAN_ACCESS = {
  free: ['rsi', 'ema'],
  pro: ['rsi', 'ema'],
  elite: ['rsi', 'ema', 'macd'],
}

function isIndicatorAllowedByPlan(plan, indicator) {
  return PLAN_ACCESS[plan]?.includes(indicator)
}

function loadState() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    return saved ? JSON.parse(saved) : null
  } catch {
    return null
  }
}

function saveState(state) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  } catch (e) {
    console.warn('Failed to save state:', e)
  }
}

export default function ChartPanel({ symbol, signals = [] }) {
  const containerRef = useRef(null)
  const chartRef = useRef(null)
  const candleSeriesRef = useRef(null)
  const volumeSeriesRef = useRef(null)
  const emaSeriesRef = useRef(null)
  const bbSeriesRef = useRef(null)
  const rsiSeriesRef = useRef(null)
  const macdSeriesRef = useRef(null)
  const signalLineRef = useRef(null)
  const markerSeriesRef = useRef(null)
  
  const [timeframe, setTimeframe] = useState(() => {
    const saved = loadState()
    return saved?.timeframe || '1h'
  })
  const [indicators, setIndicators] = useState(() => {
    const saved = loadState()
    return saved?.indicators || DEFAULT_INDICATORS
  })
  const [params, setParams] = useState(() => {
    const saved = loadState()
    return saved?.params || DEFAULT_PARAMS
  })
  const [plan, setPlan] = useState(() => {
    const saved = loadState()
    return saved?.plan || 'elite'
  })
  const [showSettings, setShowSettings] = useState(false)

  useEffect(() => {
    saveState({ symbol, timeframe, indicators, params, plan })
  }, [symbol, timeframe, indicators, params, plan])

  const loadChartData = useCallback(async (sym, tf, candleSeries, volumeSeries, emaSeries, bbSeries, rsiSeries, macdSeries, signalLine) => {
    try {
      const queryParams = new URLSearchParams({ symbol: sym, interval: tf })
      if (params.rsiPeriod !== 14) queryParams.set('rsi_period', params.rsiPeriod.toString())
      if (params.macdFast !== 12) queryParams.set('macd_fast', params.macdFast.toString())
      if (params.macdSlow !== 26) queryParams.set('macd_slow', params.macdSlow.toString())
      if (params.bbPeriod !== 20) queryParams.set('bb_period', params.bbPeriod.toString())
      if (params.bbStd !== 2) queryParams.set('bb_std', params.bbStd.toString())
      
      const response = await fetch(`/api/candles?${queryParams}`)
      const data = await response.json()

      const candleData = data.map(d => ({
        time: d.time,
        open: d.open, high: d.high, low: d.low, close: d.close,
      }))
      candleSeries.setData(candleData)

      const volumeData = data.map(d => ({
        time: d.time, value: d.volume || 0,
        color: d.close >= d.open ? '#26a69a40' : '#ef535040',
      }))
      volumeSeries.setData(volumeData)

      if (indicators.ema) {
        const emaData = data.filter(d => d.ema12 !== null).map(d => ({ time: d.time, value: d.ema12 }))
        const ema26Data = data.filter(d => d.ema26 !== null).map(d => ({ time: d.time, value: d.ema26, lineWidth: 1, color: '#FF6B6B' }))
        emaSeries.setData(emaData)
        if (emaSeriesRef.current) emaSeriesRef.current.setData(ema26Data)
      }

      if (indicators.bollinger) {
        const bbData = data.filter(d => d.bollinger_middle !== null).map(d => ({
          time: d.time,
          value: d.bollinger_middle,
          lineWidth: 1,
          lineColor: '#4CAF50',
        }))
        bbSeries.setData(bbData)
      }

      if (indicators.rsi) {
        const rsiData = data.filter(d => d.rsi !== null).map(d => ({ time: d.time, value: d.rsi }))
        rsiSeries.setData(rsiData)
      }

      if (indicators.macd) {
        const macdData = data.filter(d => d.histogram !== null).map(d => ({ time: d.time, value: d.histogram }))
        macdSeries.setData(macdData)
        const signalData = data.filter(d => d.signal !== null).map(d => ({ time: d.time, value: d.signal }))
        signalLine.setData(signalData)
      }

      chartRef.current?.timeScale().fitContent()
    } catch (error) {
      console.error('Error loading chart data:', error)
    }
  }, [indicators, params])

  useEffect(() => {
    if (!containerRef.current) return

    const chart = createChart(containerRef.current, {
      layout: { textColor: '#DDD', background: { color: '#1e1e1e' } },
      width: containerRef.current.clientWidth,
      height: containerRef.current.clientHeight,
      timeScale: { timeVisible: true, secondsVisible: false },
    })

    const candleSeries = chart.addCandlestickSeries({
      upColor: '#26a69a', downColor: '#ef5350',
      borderDownColor: '#ef5350', borderUpColor: '#26a69a',
      wickDownColor: '#ef5350', wickUpColor: '#26a69a',
    })

    const volumeSeries = chart.addHistogramSeries({
      priceFormat: { type: 'volume' },
      priceScaleId: 'volume',
    })
    chart.priceScale('volume').applyOptions({ scaleMargins: { top: 0.85, bottom: 0 } })

    const emaSeries = chart.addLineSeries({ color: '#00BCD4', lineWidth: 1, priceScaleId: 'left', title: 'EMA 12' })
    const bbSeries = chart.addLineSeries({ color: '#4CAF50', lineWidth: 1, lineStyle: 2, priceScaleId: 'left', title: 'BB Middle' })

    const rsiSeries = chart.addLineSeries({
      color: '#9c27b0', lineWidth: 2,
      priceScaleId: 'rsi', title: 'RSI (14)',
    })

    const macdSeries = chart.addHistogramSeries({
      color: '#2196F3',
      priceScaleId: 'macd', title: 'MACD Histogram',
    })

    const signalLine = chart.addLineSeries({
      color: '#FF9800', lineWidth: 1,
      priceScaleId: 'macd', title: 'MACD Signal',
    })

    // Strategy signal markers
    const markerSeries = chart.addMarkersSeries()

    // Configure price scales
    chart.priceScale('left').applyOptions({ scaleMargins: { top: 0.1, bottom: 0.1 } })
    chart.priceScale('rsi').applyOptions({ scaleMargins: { top: 0.7, bottom: 0 }, autoScale: false, fixedMin: 0, fixedMax: 100 })
    chart.priceScale('macd').applyOptions({ scaleMargins: { top: 0.85, bottom: 0 } })

    chartRef.current = chart
    candleSeriesRef.current = candleSeries
    volumeSeriesRef.current = volumeSeries
    emaSeriesRef.current = emaSeries
    bbSeriesRef.current = bbSeries
    rsiSeriesRef.current = rsiSeries
    macdSeriesRef.current = macdSeries
    signalLineRef.current = signalLine
    markerSeriesRef.current = markerSeries

    loadChartData(symbol, timeframe, candleSeries, volumeSeries, emaSeries, bbSeries, rsiSeries, macdSeries, signalLine)

    const handleResize = () => {
      if (containerRef.current) {
        chart.applyOptions({
          width: containerRef.current.clientWidth,
          height: containerRef.current.clientHeight,
        })
      }
    }
    window.addEventListener('resize', handleResize)

    // Crosshair/zoom sync setup (placeholder for future)
    chart.timeScale().subscribeVisibleTimeRangeChange(() => {})
    chart.subscribeCrosshairMove(() => {})

    return () => {
      window.removeEventListener('resize', handleResize)
      chart.remove()
    }
  }, [symbol, timeframe, loadChartData])

  // WebSocket for real-time updates
  useEffect(() => {
    const requestedIndicators = Object.entries(indicators)
      .filter(([indicator, enabled]) => enabled && isIndicatorAllowedByPlan(plan, indicator))
      .map(([indicator]) => indicator)
      .join(',')

    const queryParams = new URLSearchParams({ symbol, plan })
    if (requestedIndicators) {
      queryParams.set('indicators', requestedIndicators)
    }

    const wsProtocol = window.location.protocol === 'https:' ? 'wss' : 'ws'
    const wsUrl = `${wsProtocol}://${window.location.host}/ws?${queryParams.toString()}`
    const ws = new WebSocket(wsUrl)
    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        candleSeriesRef.current?.update({ time: data.time, open: data.open, high: data.high, low: data.low, close: data.close })
        volumeSeriesRef.current?.update({ time: data.time, value: data.volume || 0, color: data.close >= data.open ? '#26a69a40' : '#ef535040' })

        if (data.rsi !== undefined && data.rsi !== null) {
          rsiSeriesRef.current?.update({ time: data.time, value: data.rsi })
        }

        if (data.ema12 !== undefined && data.ema12 !== null) {
          emaSeriesRef.current?.update({ time: data.time, value: data.ema12 })
        }

        if (data.ema26 !== undefined && data.ema26 !== null) {
          emaSeriesRef.current?.update({ time: data.time, value: data.ema26 })
        }

        if (data.histogram !== undefined && data.histogram !== null) {
          macdSeriesRef.current?.update({ time: data.time, value: data.histogram })
          signalLineRef.current?.update({ time: data.time, value: data.signal })
        }
      } catch (error) {
        console.error('WebSocket error:', error)
      }
    }
    return () => ws.close()
  }, [symbol, plan, indicators])

  // Update signal markers when signals change
  useEffect(() => {
    if (!markerSeriesRef.current || !signals.length) return
    
    const markers = signals.slice(-20).map(signal => ({
      time: signal.time || Math.floor(Date.now() / 1000),
      position: signal.action === 'buy' ? 'aboveBar' : 'belowBar',
      color: signal.action === 'buy' ? '#26a69a' : '#ef5350',
      shape: signal.action === 'buy' ? 'arrowUp' : 'arrowDown',
      text: signal.action?.toUpperCase() || '',
    }))
    markerSeriesRef.current.setMarkers(markers)
  }, [signals])

  const toggleIndicator = (indicator) => {
    if (!isIndicatorAllowedByPlan(plan, indicator)) {
      return
    }

    setIndicators(prev => ({ ...prev, [indicator]: !prev[indicator] }))
  }

  const updateParam = (key, value) => {
    setParams(prev => ({ ...prev, [key]: value }))
  }

  useEffect(() => {
    setIndicators(prev => {
      const next = { ...prev }
      Object.keys(next).forEach(key => {
        if (!isIndicatorAllowedByPlan(plan, key)) {
          next[key] = false
        }
      })
      return next
    })
  }, [plan])

  return (
    <div className="chart-panel">
      <div className="chart-header">
        <div className="chart-title">Price Chart - {symbol.toUpperCase()}</div>
        <div className="chart-controls">
          <div className="timeframe-selector">
            {TIMEFRAMES.map(tf => (
              <button key={tf} className={`tf-btn ${timeframe === tf ? 'active' : ''}`} onClick={() => setTimeframe(tf)}>
                {tf}
              </button>
            ))}
          </div>
          <div className="plan-selector">
            {[ 'free', 'pro', 'elite' ].map(option => (
              <button
                key={option}
                className={`plan-btn ${plan === option ? 'active' : ''}`}
                onClick={() => setPlan(option)}
              >
                {option.toUpperCase()}
              </button>
            ))}
          </div>
          <div className="indicator-toggles">
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={indicators.rsi}
                disabled={!isIndicatorAllowedByPlan(plan, 'rsi')}
                onChange={() => toggleIndicator('rsi')}
              />
              RSI {plan === 'free' ? '(PRO)' : ''}
            </label>
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={indicators.macd}
                disabled={!isIndicatorAllowedByPlan(plan, 'macd')}
                onChange={() => toggleIndicator('macd')}
              />
              MACD {plan !== 'elite' ? '(ELITE)' : ''}
            </label>
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={indicators.ema}
                disabled={!isIndicatorAllowedByPlan(plan, 'ema')}
                onChange={() => toggleIndicator('ema')}
              />
              EMA {plan === 'free' ? '(PRO)' : ''}
            </label>
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={indicators.bollinger}
                disabled={true}
                onChange={() => toggleIndicator('bollinger')}
              />
              BB (future)
            </label>
            <button className="settings-btn" onClick={() => setShowSettings(!showSettings)}>⚙️</button>
          </div>
        </div>
      </div>
      <div className="chart-container-inner" ref={containerRef} />
    </div>
  )
}