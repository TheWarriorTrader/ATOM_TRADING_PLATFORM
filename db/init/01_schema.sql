-- Inizializzazione Database Trading System
-- TimescaleDB Schema v1.0

-- Abilita TimescaleDB
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- ============================================
-- TABELLE HYPERTABLE (time-series)
-- ============================================

-- Barre OHLCV (hypertable)
-- Compression: after 7 days
CREATE TABLE IF NOT EXISTS bars (
    time TIMESTAMPTZ NOT NULL,
    symbol TEXT NOT NULL,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume INTEGER NOT NULL DEFAULT 0,
    provider TEXT NOT NULL DEFAULT 'unknown',
    
    PRIMARY KEY (time, symbol)
);

-- Converti bars in hypertable
SELECT create_hypertable('bars', 'time', 
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- Tick data (per order flow)
-- Compression: after 1 day
CREATE TABLE IF NOT EXISTS ticks (
    time TIMESTAMPTZ NOT NULL,
    symbol TEXT NOT NULL,
    price DOUBLE PRECISION NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    side TEXT CHECK (side IN ('bid', 'ask', 'trade')),
    provider TEXT NOT NULL DEFAULT 'unknown'
);

SELECT create_hypertable('ticks', 'time',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- Fill (esecuzioni ordini)
-- Compression: after 30 days
CREATE TABLE IF NOT EXISTS fills (
    time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    id UUID DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL,
    symbol TEXT NOT NULL,
    price DOUBLE PRECISION NOT NULL,
    quantity DOUBLE PRECISION NOT NULL,
    side TEXT NOT NULL,
    commission DOUBLE PRECISION DEFAULT 0,
    provider TEXT NOT NULL DEFAULT 'unknown',
    
    PRIMARY KEY (time, id)
);

SELECT create_hypertable('fills', 'time',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- Eventi di trading (per audit e replay)
-- Compression: after 7 days
CREATE TABLE IF NOT EXISTS trading_events (
    time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    id UUID DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    event_data JSONB NOT NULL,
    PRIMARY KEY (time, id)
);

SELECT create_hypertable('trading_events', 'time',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- ============================================
-- TABELLE STANDARD
-- ============================================

-- Ordini
CREATE TABLE IF NOT EXISTS orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    external_id TEXT,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    quantity DOUBLE PRECISION NOT NULL,
    order_type TEXT NOT NULL CHECK (order_type IN ('market', 'limit', 'stop')),
    limit_price DOUBLE PRECISION,
    stop_price DOUBLE PRECISION,
    tif TEXT NOT NULL DEFAULT 'day' CHECK (tif IN ('day', 'gtc', 'ioc', 'fok')),
    status TEXT NOT NULL DEFAULT 'created' CHECK (status IN ('created', 'submitted', 'pending', 'partially_filled', 'filled', 'cancelled', 'rejected')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    provider TEXT NOT NULL DEFAULT 'unknown'
);

-- Posizioni
CREATE TABLE IF NOT EXISTS positions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol TEXT NOT NULL UNIQUE,
    quantity DOUBLE PRECISION NOT NULL DEFAULT 0,
    avg_price DOUBLE PRECISION NOT NULL DEFAULT 0,
    realized_pnl DOUBLE PRECISION NOT NULL DEFAULT 0,
    unrealized_pnl DOUBLE PRECISION NOT NULL DEFAULT 0,
    opened_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================
-- INDICI OTTIMIZZATI
-- ============================================

-- Indici per range scan su time-series
CREATE INDEX IF NOT EXISTS idx_bars_symbol_time ON bars(symbol, time DESC);
CREATE INDEX IF NOT EXISTS idx_ticks_symbol_time ON ticks(symbol, time DESC);
CREATE INDEX IF NOT EXISTS idx_fills_symbol_time ON fills(symbol, time DESC);
CREATE INDEX IF NOT EXISTS idx_fills_order_id ON fills(order_id, time DESC);
CREATE INDEX IF NOT EXISTS idx_trading_events_type_time ON trading_events(event_type, time DESC);

-- Indici per orders
CREATE INDEX IF NOT EXISTS idx_orders_symbol ON orders(symbol, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);
CREATE INDEX IF NOT EXISTS idx_orders_created_at ON orders(created_at DESC);

-- ============================================
-- COMPRESSIONE
-- ============================================

-- Compressione bars: dopo 7 giorni, segmentato per symbol
ALTER TABLE bars SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'symbol',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('bars', INTERVAL '7 days', if_not_exists => TRUE);

-- Compressione ticks: dopo 1 giorno (alto volume)
ALTER TABLE ticks SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'symbol',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('ticks', INTERVAL '1 day', if_not_exists => TRUE);

-- Compressione fills: dopo 30 giorni
ALTER TABLE fills SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'symbol',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('fills', INTERVAL '30 days', if_not_exists => TRUE);

-- Compressione trading_events: dopo 7 giorni
ALTER TABLE trading_events SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'event_type',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('trading_events', INTERVAL '7 days', if_not_exists => TRUE);

-- ============================================
-- CONTINUOUS AGGREGATES (opzionale)
-- ============================================

-- Aggregate 1-min bars from ticks (se necessario)
CREATE MATERIALIZED VIEW IF NOT EXISTS bars_1min
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('1 minute', time) AS bucket,
    symbol,
    first(price, time) AS open,
    max(price) AS high,
    min(price) AS low,
    last(price, time) AS close,
    sum(size) AS volume
FROM ticks
GROUP BY bucket, symbol
WITH NO DATA;

-- Policy refresh continuo
SELECT add_continuous_aggregate_policy('bars_1min',
    start_offset => INTERVAL '1 month',
    end_offset => INTERVAL '1 minute',
    schedule_interval => INTERVAL '1 minute',
    if_not_exists => TRUE
);

-- ============================================
-- FUNZIONI UTILITY
-- ============================================

-- Funzione per aggiornare timestamp automaticamente
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Trigger per orders
CREATE TRIGGER update_orders_updated_at
    BEFORE UPDATE ON orders
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Trigger per positions
CREATE TRIGGER update_positions_updated_at
    BEFORE UPDATE ON positions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================
-- VIEWS
-- ============================================

-- View posizioni correnti con P&L
CREATE OR REPLACE VIEW v_positions_with_pnl AS
SELECT
    p.*,
    CASE 
        WHEN p.quantity > 0 THEN (p.quantity * p.avg_price)
        ELSE 0
    END as position_value
FROM positions p
WHERE p.quantity != 0;

-- View ordini aperti
CREATE OR REPLACE VIEW v_open_orders AS
SELECT * FROM orders
WHERE status IN ('submitted', 'pending', 'partially_filled')
ORDER BY created_at DESC;

-- View fills recenti con dettagli ordine
CREATE OR REPLACE VIEW v_fills_recent AS
SELECT 
    f.*,
    o.external_id as order_external_id,
    o.order_type
FROM fills f
LEFT JOIN orders o ON f.order_id = o.id
ORDER BY f.time DESC
LIMIT 1000;
