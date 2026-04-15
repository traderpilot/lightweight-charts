let currentSymbol = 'btcusdt';
let chart;
let candleSeries;
let rsiSeries;
let macdSeries;
let socket;

function initChart() {
    chart = LightweightCharts.createChart(document.getElementById('chart'), {
        width: 800,
        height: 400,
    });

    // Main candlestick series
    candleSeries = chart.addCandlestickSeries();

    // RSI indicator
    rsiSeries = chart.addLineSeries({
        color: 'purple',
        lineWidth: 2,
        title: 'RSI',
        priceScaleId: 'right',
    });

    // MACD histogram
    macdSeries = chart.addHistogramSeries({
        color: 'blue',
        title: 'MACD Histogram',
        priceScaleId: 'macd',
    });
}

function loadSymbol(symbol) {
    currentSymbol = symbol;
    loadData();
    
    // Close existing socket and open new one (though currently broadcasting all)
    if (socket) {
        socket.close();
    }
    connectWebSocket();
}

function loadData() {
    fetch(`/api/candles?symbol=${currentSymbol}`)
        .then(res => res.json())
        .then(data => {
            // Set candlestick data
            candleSeries.setData(data);

            // Set RSI data (filter out null values)
            const rsiData = data
                .filter(d => d.rsi !== null)
                .map(d => ({ time: d.time, value: d.rsi }));
            rsiSeries.setData(rsiData);

            // Set MACD data
            const macdData = data
                .filter(d => d.histogram !== null)
                .map(d => ({ time: d.time, value: d.histogram }));
            macdSeries.setData(macdData);
        });
}

function connectWebSocket() {
    const wsProtocol = window.location.protocol === 'https:' ? 'wss' : 'ws'
    socket = new WebSocket(`${wsProtocol}://${window.location.host}/ws`)

    socket.onmessage = (event) => {
        const data = JSON.parse(event.data);
        if (data.symbol && data.symbol.toLowerCase() === currentSymbol) {
            candleSeries.update({
                time: data.time,
                open: data.open,
                high: data.high,
                low: data.low,
                close: data.close
            });
        }
    };
}

// Initialize chart
initChart();
loadData();
connectWebSocket();

// Symbol buttons
document.querySelectorAll('button').forEach(button => {
    button.addEventListener('click', () => {
        loadSymbol(button.textContent.toLowerCase().replace('/', ''));
    });
});