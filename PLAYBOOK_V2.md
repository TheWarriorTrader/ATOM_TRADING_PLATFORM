# PLAYBOOK V2: SISTEMA DI TRADING IN RUST
## "Production-Ready Development Guide"

**Versione:** 2.1-PRO
**Data:** Marzo 2026
**Stack:** Rust (Tokio), TimescaleDB, Redis, Tauri v2
**Goal:** Sistema end-to-end operante in 4 settimane con supporto dev professionale

---

> 📋 **Documenti Collegati:**
> - 📊 [TIMELINE_V2.md](TIMELINE_V2.md) - Tracciamento progresso e issue (aggiornare ad ogni task!)
> - 📖 Questo file (PLAYBOOK_V2.md) - Definizione task e specifiche (statico)

---

## INDICE

0. [Setup Ambiente di Sviluppo](#0-setup-ambiente-di-sviluppo)
1. [Fondamenti e Principi](#1-fondamenti-e-principi)
2. [Architettura di Riferimento](#2-architettura-di-riferimento)
3. [Fase 0: Foundation (Week 1)](#fase-0-foundation-week-1)
4. [Fase 1: Execution Core (Week 2)](#fase-1-execution-core-week-2)
5. [Fase 2: Strategy Engine (Week 3)](#fase-2-strategy-engine-week-3)
6. [Fase 3: GUI & Operations (Week 4)](#fase-3-gui--operations-week-4)
7. [Operazioni e Manutenzione](#7-operazioni-e-manutenzione)
8. [Riferimenti](#8-riferimenti)

---

## 0. SETUP AMBIENTE DI SVILUPPO

**Prerequisito obbligatorio** prima di iniziare qualsiasi fase di sviluppo.

### 0.1 Rust Toolchain Installation

#### Task 0.1.1: Installazione Rust via rustup (30min)
**Priorità:** Critica

```bash
# Installazione rustup (gestore toolchain Rust)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verifica installazione
rustc --version  # es: rustc 1.85.0
cargo --version  # es: cargo 1.85.0
```

---

#### Task 0.1.2: Configurazione Toolchain Progetto (15min)
**Priorità:** Alta

```bash
# Installazione toolchain specifica per il progetto (stable)
rustup default stable
rustup update

# Componenti necessari per sviluppo
rustup component add rustfmt clippy rust-src
```

---

#### Task 0.1.3: Installazione Cargo Tools (15min)
**Priorità:** Alta

```bash
# cargo-watch: ricompilazione automatica
cargo install cargo-watch

# cargo-expand: espansione macro (utile per debug)
cargo install cargo-expand

# cargo-edit: gestione dipendenze da CLI
cargo install cargo-edit

# cargo-tarpaulin: code coverage
cargo install cargo-tarpaulin

# cargo-flamegraph: profiling
cargo install flamegraph

# cargo-audit: security audit dipendenze
cargo install cargo-audit

# Se Tauri non è installato globalmente
cargo install tauri-cli --version "^2.0"
```

---

### 0.2 Dipendenze di Sistema

#### Task 0.2.1: Installazione System Dependencies (30min)
**Priorità:** Critica

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    libsqlite3-dev \
    librust-atk-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    curl \
    wget \
    git

# Per Tauri v2 (librerie GUI)
sudo apt install -y \
    libappindicator3-dev \
    librsvg2-dev \
    libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev
```

---

### 0.3 Setup Progetto Locale

#### Task 0.3.1: Clone Repository (10min)
**Priorità:** Critica

```bash
# Clonare il repository
git clone <repository-url> trading-core
cd trading-core

# Verifica struttura
ls -la
tree -L 2  # dovrebbe mostrare crates/, db/, docker-compose.yml, etc.
```

---

#### Task 0.3.2: Verifica Ambiente Rust (15min)
**Priorità:** Critica

```bash
# Verifica toolchain attiva
rustup show

# Output atteso:
# Default toolchain: stable-x86_64-unknown-linux-gnu
# rustc: 1.85.0
# cargo: 1.85.0

# Verifica componenti installati
rustup component list --installed
# Output atteso:
# cargo-x86_64-unknown-linux-gnu
# clippy-x86_64-unknown-linux-gnu
# rustfmt-x86_64-unknown-linux-gnu
# rust-src
```

---

#### Task 0.3.3: Build Iniziale (30min)
**Priorità:** Critica

```bash
# Primo build (scarica e compila tutte le dipendenze)
cargo build

# Verifica che compili senza errori
# Nota: il primo build è lento (scarica crates da crates.io)

# Verifica toolchain funzionante
cargo test --no-run  # Compila i test senza eseguirli

# Verifica formatter
cargo fmt --check

# Verifica linter
cargo clippy -- -D warnings
```

---

### 0.4 Setup IDE/Editor

#### Task 0.4.1: Configurazione VS Code (20min)
**Priorità:** Alta

**Estensioni richieste:**
```bash
# Installare via CLI o marketplace
code --install-extension rust-lang.rust-analyzer
code --install-extension serayuzgur.crates
code --install-extension tamasfe.even-better-toml
code --install-extension vadimcn.vscode-lldb  # Debugger
```

**Configurazione `.vscode/settings.json`:**
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.checkOnSave.extraArgs": ["--", "-D", "warnings"],
  "rust-analyzer.procMacro.enable": true,
  "rust-analyzer.lens.enable": true,
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

---

#### Task 0.4.2: Configurazione JetBrains RustRover/IntelliJ (20min)
**Priorità:** Media (se si usa questo IDE)

1. Installare plugin Rust se non presente
2. Configurare toolchain: Settings → Languages & Frameworks → Rust
3. Abilitare cargo fmt on save
4. Configurare Clippy come external linter

---

### 0.5 Setup Docker Environment

#### Task 0.5.1: Installazione Docker (20min)
**Priorità:** Critica

**Ubuntu:**
```bash
# Installazione Docker
sudo apt install -y docker.io docker-compose

# Aggiungere utente al gruppo docker (richiede logout/login)
sudo usermod -aG docker $USER
newgrp docker

# Verifica
docker --version
docker-compose --version
docker run hello-world
```

---

#### Task 0.5.2: Avvio Servizi Infrastruttura (15min)
**Priorità:** Critica

```bash
# Dalla root del progetto
docker compose up -d

# Verifica servizi attivi
docker compose ps

# Verifica connessioni
pg_isready -h localhost -p 5432 -U trader  # deve rispondere "accepting connections"
redis-cli ping  # deve rispondere "PONG"
```

---

### 0.6 Verifica Completa Ambiente

#### Task 0.6.1: Environment Check Script (10min)
**Priorità:** Alta

Creare script `scripts/check-env.sh`:

```bash
#!/bin/bash
set -e

echo "🔍 Verifica ambiente di sviluppo..."

# Check Rust
echo -n "✓ Rust toolchain: "
rustc --version

echo -n "✓ Cargo: "
cargo --version

# Check Docker
echo -n "✓ Docker: "
docker --version

# Check componenti Rust
echo -n "✓ rustfmt: "
rustfmt --version

echo -n "✓ clippy: "
cargo clippy --version

# Check build
echo -n "✓ Build test: "
cargo check --quiet && echo "OK"

# Check servizi
echo -n "✓ PostgreSQL: "
pg_isready -h localhost -p 5432 -U trader > /dev/null 2>&1 && echo "OK" || echo "DOWN"

echo -n "✓ Redis: "
redis-cli ping > /dev/null 2>&1 && echo "OK" || echo "DOWN"

echo ""
echo "✅ Ambiente pronto per lo sviluppo!"
```

Eseguire:
```bash
chmod +x scripts/check-env.sh
./scripts/check-env.sh
```

---

### 0.7 Gestione Dipendenze Cargo

#### Task 0.7.1: Understanding Cargo.lock (10min)
**Priorità:** Media

```bash
# Cargo.lock viene generato automaticamente e tracciato in git
# garantisce build riproducibili tra diversi ambienti

# Per aggiornare dipendenze:
cargo update

# Per aggiornare solo una dipendenza specifica:
cargo update -p tokio

# Per vedere albero dipendenze:
cargo tree
cargo tree -d  # mostra solo duplicati

# Per audit sicurezza:
cargo audit
```

---

#### Task 0.7.2: Workspace Cargo Configuration (10min)
**Priorità:** Media

Il progetto usa workspace Cargo (vedi [`Cargo.toml`](Cargo.toml:1)):

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1.43", features = ["full"] }
# ... altre dipendenze condivise
```

**Comandi utili workspace:**
```bash
# Build tutti i crate
cargo build --workspace

# Test tutti i crate
cargo test --workspace

# Build solo un crate specifico
cargo build -p domain
cargo test -p application

# Check dipendenze workspace
cargo tree -e normal --prefix none
```

---

### 0.8 Troubleshooting Comune

| Problema | Soluzione |
|----------|-----------|
| `linker 'cc' not found` | `sudo apt install build-essential` |
| `openssl-sys` build fail | `sudo apt install libssl-dev pkg-config` |
| `pq-sys` build fail | `sudo apt install libpq-dev` |
| `gtk` related errors | `sudo apt install libgtk-3-dev` |
| `cargo` command not found | `source $HOME/.cargo/env` |
| Cannot connect to Docker | `sudo usermod -aG docker $USER` + logout/login |
| Port 5432 already in use | `sudo lsof -i :5432` poi `kill <PID>` |

---

## 1. FONDAMENTI E PRINCIPI

### 1.1 Principi Non-Negoziabili

| Principio | Descrizione | Metrica di Successo |
|-----------|-------------|---------------------|
| **YAGNI** | Non implementare ciò che non serve OGGI | Ogni task deve risolvere un problema reale |
| **Swappable** | Ogni componente sostituibile senza refactoring | Swap provider in < 2 giorni |
| **Verificabile** | Output testabile entro 1 giorno | Test passanti + demo funzionante |
| **Domain-Centric** | Business logic pura, isolata da framework | 100% test coverage domain layer |

### 1.2 Definizione di Done (DoD)

Ogni task è considerato completato quando:

- [ ] **Code:** Implementazione completa secondo specifiche
- [ ] **Tests:** Unit tests con coverage > 80%
- [ ] **Integration:** Test di integrazione passanti dove applicabile
- [ ] **Docs:** Documentazione inline (rustdoc) + aggiornamento README se necessario
- [ ] **Review:** Codice revisionato (self-review o peer)
- [ ] **Verification:** Output verificabile eseguito con successo

> 📝 **Nota:** Progressi, blocchi e issue vanno documentati in [TIMELINE_V2.md](TIMELINE_V2.md).\
> **Regola:** Aggiornare la timeline immediatamente dopo ogni task completato o blocco riscontrato.

---

## 2. ARCHITETTURA DI RIFERIMENTO

### 2.1 Layered Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         PRESENTATION LAYER                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                  │
│  │  CLI Tool    │  │ Tauri GUI    │  │ REST/gRPC    │                  │
│  └──────────────┘  └──────────────┘  └──────────────┘                  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        APPLICATION LAYER                                │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  TradingService    RiskEngine    StrategyEngine    EventBus    │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          DOMAIN LAYER (Puro)                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐  │
│  │   Entities   │  │Value Objects │  │   Events     │  │   Traits   │  │
│  │  (Bar, Order)│  │(Price, Symbol)│  │(TradingEvent)│  │(Repository)│  │
│  └──────────────┘  └──────────────┘  └──────────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      INFRASTRUCTURE LAYER                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐  │
│  │ TimescaleDB  │  │    Redis     │  │ IB Provider  │  │   gRPC     │  │
│  │ Repository   │  │    Cache     │  │  Adapter     │  │  Server    │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Dipendenze Tra Layer

```
Domain ←── Nessuna dipendenza (puro)
   ↑
Application ←── Dipende solo da Domain
   ↑
Infrastructure ←── Dipende da Domain + Application
   ↑
Presentation ←── Dipende da Application (via API)
```

---

## FASE 0: FOUNDATION (Week 1)
**Obiettivo:** Data ingestion stabile da IB → Storage  
**Milestone:** Dati che fluiscono e persistono verificabili

### Day 1: Environment Setup (8h)

#### Task 0.1.1: Docker Infrastructure (2h)
**Assegnazione:** DevOps/Backend  
**Priorità:** Critica

- [ ] Verificare docker-compose.yml esistente
- [ ] Aggiungere healthcheck TimescaleDB
- [ ] Aggiungere healthcheck Redis
- [ ] Configurare volumi persistenti
- [ ] Testare `docker compose up -d`

**Acceptance Criteria:**
```bash
docker compose up -d
docker compose ps  # All services healthy
pg_isready -h localhost -p 5432 -U trader  # accepting connections
redis-cli ping  # PONG
```

**Rischi:**
- Porte già in uso → Verificare con `netstat -tlnp | grep -E '5432|6379'`
- Permessi volumi → `sudo chown -R $USER:$USER ./db/data`

---

#### Task 0.1.2: Rust Workspace Setup (2h)
**Assegnazione:** Backend  
**Priorità:** Critica

- [ ] Inizializzare workspace cargo
- [ ] Creare crate `domain` (lib)
- [ ] Creare crate `application` (lib)
- [ ] Creare crate `infrastructure` (lib)
- [ ] Creare crate `cli` (bin)
- [ ] Configurare dependencies in workspace Cargo.toml
- [ ] Setup rustfmt e clippy config

**Acceptance Criteria:**
```bash
cargo build  # Compila senza errori
cargo test   # Test esistenti passano
cargo clippy # Nessun warning critico
```

---

#### Task 0.1.3: Database Schema Deploy (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

- [ ] Verificare script SQL in `db/init/`
- [ ] Aggiungere migrazioni mancanti se necessario
- [ ] Testare applicazione schema
- [ ] Verificare hypertable creation
- [ ] Testare compressione policy

**Acceptance Criteria:**
```bash
docker compose up -d
psql -h localhost -U trader -d trading_db -c "\dt"  # Tabelle create
psql -h localhost -U trader -d trading_db -c "SELECT * FROM timescaledb_information.hypertables;"
```

---

#### Task 0.1.4: Configuration Management (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

- [ ] Implementare Config loader con hot reload
- [ ] Supportare file YAML + env vars
- [ ] Definire strutture config per ogni environment
- [ ] Aggiungere validazione config

**Acceptance Criteria:**
```bash
cargo run --bin cli -- config --validate  # Config valida
cargo run --bin cli -- config --show      # Mostra config attuale
```

---

### Day 2: Domain Layer - Core Entities (8h)

#### Task 0.2.1: Value Objects (2h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/domain/src/value_objects.rs`

Implementare:
- [ ] `Price` (wrapping Decimal, validazione > 0)
- [ ] `Symbol` (validazione formato, uppercase)
- [ ] `Quantity` (wrapping Decimal, validazione > 0)
- [ ] `Volume` (wrapping i64)
- [ ] `OrderId` (wrapper String o UUID)

**Requisiti:**
- Type-safe, nessun primitive obsession
- Validazione al construction
- Implementare `Display`, `FromStr`, `Serialize`, `Deserialize`

**Tests:**
```rust
#[test]
fn price_cannot_be_negative() {
    assert!(Price::new(dec!(-1.0)).is_err());
}

#[test]
fn symbol_normalizes_to_uppercase() {
    assert_eq!(Symbol::new("nq").to_string(), "NQ");
}
```

---

#### Task 0.2.2: Core Entities (2h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/domain/src/entities/mod.rs`

Implementare:
- [ ] `Bar` (OHLCV + timestamp + symbol)
- [ ] `Tick` (price + size + timestamp + symbol + side)
- [ ] `Order` (id, symbol, side, quantity, type, tif, status)
- [ ] `Position` (symbol, quantity, avg_price, pnl)

**Requisiti:**
- Campi immutabili dopo construction
- Validazione invariants
- Metodi helper (es. `is_long()`, `notional_value()`)

---

#### Task 0.2.3: Domain Events (2h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**File:** `crates/domain/src/events.rs`

Implementare enum:
```rust
pub enum TradingEvent {
    BarReceived { bar: Bar, source: String },
    TickReceived { tick: Tick, source: String },
    OrderSubmitted { order_id: OrderId, order: Order },
    OrderFilled { order_id: OrderId, fill: Fill },
    OrderRejected { order_id: OrderId, reason: String },
    PositionUpdated { symbol: Symbol, position: Position },
    SignalGenerated { strategy: String, signal: Signal },
}
```

- [ ] Aggiungere timestamp a ogni evento
- [ ] Implementare `Clone`, `Debug`, `Serialize`
- [ ] Aggiungere metodo `event_type()` per filtering

---

#### Task 0.2.4: Error Types (2h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**File:** `crates/domain/src/errors.rs`

Implementare:
- [ ] `DomainError` (errori business logic)
- [ ] `ValidationError` (input non valido)
- [ ] `ProviderError` (errori da provider esterni)
- [ ] Conversioni automatiche con `thiserror`

---

### Day 3: Repository Traits (8h)

#### Task 0.3.1: Market Data Repository Traits (4h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/domain/src/repositories.rs`

```rust
#[async_trait]
pub trait BarRepository: Send + Sync {
    async fn save(&self, bar: &Bar) -> Result<(), DomainError>;
    async fn save_batch(&self, bars: &[Bar]) -> Result<(), DomainError>;
    async fn get_range(
        &self,
        symbol: &Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Bar>, DomainError>;
    async fn get_latest(&self, symbol: &Symbol, n: usize) -> Result<Vec<Bar>, DomainError>;
}

#[async_trait]
pub trait TickRepository: Send + Sync {
    async fn save(&self, tick: &Tick) -> Result<(), DomainError>;
    async fn get_range(
        &self,
        symbol: &Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Tick>, DomainError>;
}
```

- [ ] Implementare trait con tutti i metodi
- [ ] Aggiungere documentazione rustdoc
- [ ] Definire casi d'errore per ogni metodo

---

#### Task 0.3.2: Market Data Provider Trait (2h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/domain/src/providers.rs`

```rust
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn connect(&mut self) -> Result<(), ProviderError>;
    async fn disconnect(&mut self) -> Result<(), ProviderError>;
    async fn is_connected(&self) -> bool;
    
    async fn subscribe_bars(
        &self,
        symbol: &Symbol,
        timeframe: TimeFrame,
    ) -> Result<mpsc::Receiver<Bar>, ProviderError>;
    
    async fn subscribe_ticks(
        &self,
        symbol: &Symbol,
    ) -> Result<mpsc::Receiver<Tick>, ProviderError>;
    
    async fn unsubscribe(&self, symbol: &Symbol) -> Result<(), ProviderError>;
}
```

---

#### Task 0.3.3: Unit Tests Domain (2h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**Pattern:** TDD

- [ ] Test value objects (validazione, normalization)
- [ ] Test entities (invariants, business rules)
- [ ] Test events (serialization, deserialization)
- [ ] Mock implementations per testing

**Acceptance:**
```bash
cargo test -p domain --lib  # > 80% coverage
cargo tarpaulin -p domain   # Verifica coverage
```

---

### Day 4: Infrastructure - Database (8h)

#### Task 0.4.1: TimescaleDB Repository Implementation (4h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/infrastructure/src/persistence/timescale.rs`

Implementare:
- [ ] `TimescaleBarRepository` con sqlx
- [ ] `TimescaleTickRepository` con sqlx
- [ ] Connection pooling con deadpool
- [ ] Query parameterizzate (SQL injection safe)
- [ ] Retry logic per errori transienti

**Query ottimizzate:**
```rust
// Batch insert per performance
INSERT INTO bars (time, symbol, open, high, low, close, volume, provider)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT (time, symbol) DO UPDATE SET
    close = EXCLUDED.close,
    volume = bars.volume + EXCLUDED.volume;
```

---

#### Task 0.4.2: Redis Cache Implementation (2h)
**Assegnazione:** Backend  
**Priorità:** Media  
**File:** `crates/infrastructure/src/cache/redis.rs`

Implementare:
- [ ] Connessione Redis con connection pool
- [ ] Metodi `get`, `set`, `setex` (con TTL)
- [ ] Serializzazione con MessagePack (rmp-serde)
- [ ] Circuit breaker per failure

---

#### Task 0.4.3: Integration Tests DB (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

- [ ] Test container setup con testcontainers
- [ ] Test roundtrip save → get
- [ ] Test batch operations
- [ ] Test error handling

---

### Day 5: Infrastructure - IB Provider (8h)

#### Task 0.5.1: IB Client Wrapper (3h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/infrastructure/src/providers/ib/client.rs`

Implementare:
- [ ] Wrapper sicuro attorno a `ibapi` crate
- [ ] Gestione connessione con retry exponential backoff
- [ ] Riconnessione automatica su disconnect
- [ ] Heartbeat/ping per detectare connessione morta
- [ ] Logging dettagliato

```rust
pub struct IBClient {
    inner: Arc<Mutex<Option<Client>>>,
    config: IBConfig,
    reconnect_policy: ExponentialBackoff,
}

impl IBClient {
    pub async fn connect(&self) -> Result<(), ProviderError>;
    pub async fn ensure_connected(&self) -> Result<(), ProviderError>;
    pub async fn disconnect(&self) -> Result<(), ProviderError>;
}
```

---

#### Task 0.5.2: IB Market Data Provider (3h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/infrastructure/src/providers/ib/market_data.rs`

Implementare:
- [ ] `impl MarketDataProvider for IBMarketDataProvider`
- [ ] Mapping da IB data types a domain types
- [ ] Gestione reqId → symbol mapping
- [ ] Channel backpressure handling
- [ ] Error translation (IB errors → DomainError)

---

#### Task 0.5.3: CLI Ingest Command (2h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**File:** `crates/cli/src/commands/ingest.rs`

Implementare:
- [x] Subcommand `ingest` con clap
- [x] Argomenti: `--symbol`, `--duration`, `--provider`, `--timeframe`, `--output-db`, `--output-stdout`
- [x] Connessione provider → repository
- [x] Progress logging con StatsReporter
- [x] Graceful shutdown su Ctrl+C
- [x] Batch insert ottimizzato per TimescaleDB
- [x] Unit tests per parsing duration/timeframe

**Acceptance:**
```bash
cargo run --bin trading-core -- ingest --symbol NQ --duration 1h --provider ib
# [INFO] Connecting to IB Gateway at localhost:7496
# [INFO] Subscribed to NQ (M1)
# [DATA] NQ 2026-03-13 14:30:05 O:18234.50 H:18235.00 L:18233.75 C:18234.75 V:1420
# [STATS] Received: 60, Saved: 60, Rate: 12.0 bars/sec, Elapsed: 5s
# ...
# [INFO] Saved 720 bars to TimescaleDB
```

---

### Day 6-7: Polish & Integration (16h)

#### Task 0.6.1: Event Bus Implementation (4h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**File:** `crates/infrastructure/src/messaging/event_bus.rs`

Implementare:
- [x] EventBus con `tokio::sync::broadcast`
- [x] Subscribe/unsubscribe pattern
- [x] Topic filtering
- [x] Dead letter queue per errori
- [x] Metrics (events/sec, dropped messages)

---

#### Task 0.6.2: Data Pipeline Orchestrator (4h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**File:** `crates/application/src/data_pipeline.rs`

Implementare:
- [ ] Pipeline che collega provider → event bus → repository
- [ ] Buffering e batching per ottimizzazione DB
- [ ] Circuit breaker su errori DB
- [ ] Graceful shutdown
- [ ] Health check endpoint

---

#### Task 0.6.3: Logging & Observability (3h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] Setup tracing-subscriber con JSON format
- [ ] Structured logging per tutti i componenti
- [ ] Correlation IDs per tracciare richieste
- [ ] Log levels configurabili via env

---

#### Task 0.6.4: Metrics & Monitoring (3h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] Prometheus metrics endpoint
- [ ] Metriche: bars/sec, latency DB, connection status
- [ ] Health check endpoint `/health`
- [ ] Readiness/liveness probes

---

#### Task 0.6.5: End-to-End Testing (2h)
**Assegnazione:** QA/Backend  
**Priorità:** Alta

- [ ] E2E test completo: ingest → storage → query
- [ ] Performance test (target: > 1000 bars/sec)
- [ ] Failure scenario tests

---

## FASE 1: EXECUTION CORE (Week 2)
**Obiettivo:** Gestione ordini e risk management  
**Milestone:** Ordine inviato, tracciato e risk-managed

### Day 8: Order Domain (8h)

#### Task 1.1.1: Order Entity Enhancement (3h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/domain/src/entities/order.rs`

Implementare:
- [ ] Order lifecycle state machine
- [ ] Validazione ordini (market hours, symbol valido)
- [ ] Calcolo notional value
- [ ] Metodi: `can_cancel()`, `is_active()`

```rust
pub enum OrderStatus {
    Created,
    Submitted { at: DateTime<Utc> },
    Pending { at: DateTime<Utc> },
    PartiallyFilled { filled: Quantity, remaining: Quantity, avg_price: Price },
    Filled { at: DateTime<Utc> },
    Cancelled { at: DateTime<Utc>, reason: String },
    Rejected { at: DateTime<Utc>, reason: String },
}
```

---

#### Task 1.1.2: Fill Entity (2h)
**Assegnazione:** Backend  
**Priorità:** Critica

Implementare:
- [ ] `Fill` struct (order_id, symbol, price, quantity, timestamp, commission)
- [ ] Calcolo P&L per fill
- [ ] Aggregazione fills → position

---

#### Task 1.1.3: Execution Gateway Trait (3h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/domain/src/execution.rs`

```rust
#[async_trait]
pub trait ExecutionGateway: Send + Sync {
    async fn place_order(&self, order: Order) -> Result<OrderId, ExecutionError>;
    async fn cancel_order(&self, order_id: OrderId) -> Result<(), ExecutionError>;
    async fn modify_order(&self, order_id: OrderId, modifications: OrderModifications) -> Result<(), ExecutionError>;
    async fn get_order(&self, order_id: OrderId) -> Result<Option<Order>, ExecutionError>;
    async fn get_open_orders(&self) -> Result<Vec<Order>, ExecutionError>;
    async fn get_positions(&self) -> Result<Vec<Position>, ExecutionError>;
    
    // Streaming
    async fn subscribe_order_updates(&self) -> Result<mpsc::Receiver<OrderUpdate>, ExecutionError>;
    async fn subscribe_fill_updates(&self) -> Result<mpsc::Receiver<Fill>, ExecutionError>;
}
```

---

### Day 9: Order Repository & Persistence (8h)

#### Task 1.2.1: Order Repository Trait (2h)
**Assegnazione:** Backend  
**Priorità:** Critica

```rust
#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn save(&self, order: &Order) -> Result<(), DomainError>;
    async fn update(&self, order: &Order) -> Result<(), DomainError>;
    async fn get(&self, order_id: OrderId) -> Result<Option<Order>, DomainError>;
    async fn get_open(&self) -> Result<Vec<Order>, DomainError>;
    async fn get_by_symbol(&self, symbol: &Symbol) -> Result<Vec<Order>, DomainError>;
    async fn get_history(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Order>, DomainError>;
}
```

---

#### Task 1.2.2: TimescaleDB Order Repository (3h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/infrastructure/src/persistence/order_repo.rs`

Implementare:
- [ ] SQL schema per orders (già presente)
- [ ] CRUD operations con sqlx
- [ ] Transaction support per operazioni atomiche
- [ ] Query ottimizzate per status filtering

---

#### Task 1.2.3: Fill Repository (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] `FillRepository` trait
- [ ] TimescaleDB implementation
- [ ] Relazione con orders (foreign key)

---

#### Task 1.2.4: Position Repository (1h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] `PositionRepository` trait
- [ ] UPSERT logic per position updates
- [ ] P&L calculations

---

### Day 10: IB Execution Gateway (8h)

#### Task 1.3.1: IB Execution Gateway Implementation (5h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/infrastructure/src/providers/ib/execution.rs`

Implementare:
- [ ] `impl ExecutionGateway for IBExecutionGateway`
- [ ] Mapping Order → IBOrder
- [ ] Gestione orderId (next valid ID)
- [ ] Callback handlers (orderStatus, openOrder, execDetails)
- [ ] Error handling e timeout

**Note:**
- IB usa reqId per tracking, mappare a nostro OrderId
- Gestire delay tra place_order e orderStatus callback
- Implementare timeout configurable

---

#### Task 1.3.2: Paper Trading Gateway (3h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**File:** `crates/infrastructure/src/providers/paper_trading.rs`

Implementare:
- [ ] `impl ExecutionGateway for PaperTradingGateway`
- [ ] Simulazione fill a prezzo di mercato (da market data)
- [ ] Gestione posizioni fittizie
- [ ] Calcolo P&L in tempo reale
- [ ] Commissioni simulate

---

### Day 11: Risk Management (8h)

#### Task 1.4.1: Risk Engine Core (4h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/application/src/risk/engine.rs`

Implementare:
- [ ] `RiskEngine` struct con configurazione
- [ ] Pre-trade checks:
  - Position size limits
  - Daily loss limits
  - Drawdown limits
  - Symbol exposure limits
- [ ] Post-trade monitoring
- [ ] Circuit breaker automatico

```rust
pub struct RiskConfig {
    pub max_position_size: HashMap<Symbol, Quantity>,
    pub max_total_exposure: Money,
    pub daily_loss_limit: Money,
    pub max_drawdown_pct: f64,
    pub kill_switch_enabled: bool,
}

pub enum RiskDecision {
    Allow,
    Reject { reason: String },
    Reduce { max_allowed: Quantity },
}
```

---

#### Task 1.4.2: Risk Metrics Calculation (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] Calcolo exposure in tempo reale
- [ ] Calcolo daily P&L
- [ ] Calcolo drawdown
- [ ] Margin requirements (se applicabile)

---

#### Task 1.4.3: Risk Integration Tests (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

- [ ] Test reject su superamento limiti
- [ ] Test kill switch automatico
- [ ] Test recovery post circuit breaker

---

### Day 12: Trading Service (8h)

#### Task 1.5.1: Trading Service Core (4h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/application/src/trading_service.rs`

Implementare:
- [ ] `TradingService` orchestrator
- [ ] Workflow: validazione → risk check → execution → persistence
- [ ] Gestione errori e rollback
- [ ] Event publishing

```rust
pub struct TradingService {
    execution: Arc<dyn ExecutionGateway>,
    order_repo: Arc<dyn OrderRepository>,
    position_repo: Arc<dyn PositionRepository>,
    risk_engine: RiskEngine,
    event_bus: Arc<EventBus>,
}

impl TradingService {
    pub async fn place_order(&self, order: Order) -> Result<OrderId, TradingError> {
        // 1. Validate
        order.validate()?;
        
        // 2. Risk check
        let positions = self.position_repo.get_all().await?;
        match self.risk_engine.check_pre_trade(&order, &positions).await? {
            RiskDecision::Allow => {}
            RiskDecision::Reject { reason } => return Err(TradingError::RiskRejected(reason)),
            RiskDecision::Reduce { max_allowed } => {
                // Auto-reduce order size
            }
        }
        
        // 3. Persist as "Submitted"
        self.order_repo.save(&order).await?;
        
        // 4. Execute
        let order_id = self.execution.place_order(order.clone()).await?;
        
        // 5. Publish event
        self.event_bus.publish(TradingEvent::OrderSubmitted { 
            order_id: order_id.clone(), 
            order 
        });
        
        Ok(order_id)
    }
}
```

---

#### Task 1.5.2: Order Lifecycle Manager (3h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] Task che ascolta order updates da execution gateway
- [ ] Aggiornamento stato ordini in DB
- [ ] Gestione fills e aggiornamento posizioni
- [ ] Pubblicazione eventi

---

#### Task 1.5.3: CLI Trading Commands (1h)
**Assegnazione:** Backend  
**Priorità:** Alta  
**File:** `crates/cli/src/commands/trading.rs`

Implementare:
- [ ] `place-order` subcommand
- [ ] `cancel-order` subcommand
- [ ] `get-positions` subcommand
- [ ] `get-orders` subcommand

---

### Day 12b: IB Account & Portfolio Monitoring (8h)

#### Task 1.5.4: IB Account Data Service (4h)
**Assegnazione:** Backend
**Priorità:** Alta
**File:** `crates/infrastructure/src/providers/ib/account_service.rs`

Implementare servizio per ricevere e gestire dati account da IB:
- [ ] Subscription a `reqAccountUpdates` (IB API)
- [ ] Parsing account values (NetLiquidation, AvailableFunds, etc.)
- [ ] Aggiornamento in tempo reale via EventBus
- [ ] Persistenza storica account snapshots
- [ ] Alert su margin calls o low liquidity

```rust
pub struct IBAccountService {
    client: Arc<IBClient>,
    account_cache: Arc<RwLock<AccountData>>,
}

impl IBAccountService {
    pub async fn subscribe_account_updates(&self) -> Result<(), ProviderError>;
    pub async fn get_account_summary(&self) -> AccountData;
    pub async fn get_portfolio_positions(&self) -> Vec<Position>;
}
```

**Dati IB da monitorare:**
- NetLiquidation (equity totale)
- AvailableFunds / BuyingPower
- MaintMarginReq / ExcessLiquidity
- DailyPnL (realized + unrealized)
- Currency balances per currency

---

#### Task 1.5.5: IB Portfolio Sync (3h)
**Assegnazione:** Backend
**Priorità:** Alta

Implementare sincronizzazione portafoglio con IB:
- [ ] Recupero posizioni da IB (`reqPositions`)
- [ ] Reconciliation posizioni locali vs IB
- [ ] Mismatch detection e alerting
- [ ] Cost basis tracking da IB
- [ ] Realized P&L da IB executions

---

#### Task 1.5.6: IB Execution Detail Service (1h)
**Assegnazione:** Backend
**Priorità:** Media

Implementare tracking dettagliato esecuzioni:
- [ ] Subscription a `execDetails` (IB API)
- [ ] Tracking commissioni IB
- [ ] Tracking fill prices e slippage
- [ ] Audit log completo per compliance

---

### Day 13-14: Integration & Testing (16h)

#### Task 1.6.1: Integration Tests (6h)
**Assegnazione:** QA/Backend  
**Priorità:** Critica

- [ ] Test flusso completo: place → fill → position update
- [ ] Test risk rejection
- [ ] Test order cancellation
- [ ] Test paper trading vs real

---

#### Task 1.6.2: Error Handling & Recovery (4h)
**Assegnazione:** Backend  
**Priorità:** Alta

- [ ] Retry policy per errori transienti
- [ ] Circuit breaker pattern
- [ ] Dead letter queue per messaggi falliti
- [ ] Alerting su errori critici

---

#### Task 1.6.3: Performance Testing (3h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] Benchmark order placement latency
- [ ] Test throughput (ordini/sec)
- [ ] Memory profiling

---

#### Task 1.6.4: Documentation (3h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] API documentation (rustdoc)
- [ ] Sequence diagrams per flussi principali
- [ ] Runbook operazioni

---

## FASE 2: STRATEGY ENGINE (Week 3)
**Obiettivo:** Logica strategica che consuma dati ed emette segnali  
**Milestone:** Strategia attiva su paper trading

### Day 15: Strategy Framework (8h)

#### Task 2.1.1: Strategy Trait Definition (3h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/domain/src/strategy.rs`

```rust
#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    fn symbols(&self) -> &[Symbol];
    fn timeframe(&self) -> TimeFrame;
    
    // Lifecycle
    async fn init(&mut self, ctx: &mut StrategyContext) -> Result<(), StrategyError>;
    async fn shutdown(&mut self) -> Result<(), StrategyError>;
    
    // Event handlers
    async fn on_bar(&mut self, bar: &Bar, ctx: &StrategyContext) -> Option<Signal>;
    async fn on_tick(&mut self, tick: &Tick, ctx: &StrategyContext) -> Option<Signal>;
    async fn on_fill(&mut self, fill: &Fill, ctx: &StrategyContext);
    async fn on_position_update(&mut self, position: &Position, ctx: &StrategyContext);
}

pub struct Signal {
    pub id: SignalId,
    pub timestamp: DateTime<Utc>,
    pub symbol: Symbol,
    pub action: Action,  // Buy, Sell, Close
    pub quantity: Quantity,
    pub order_type: OrderType,
    pub confidence: f64,  // 0.0 - 1.0
    pub reason: String,
    pub metadata: HashMap<String, String>,
}
```

---

#### Task 2.1.2: Strategy Context (2h)
**Assegnazione:** Backend  
**Priorità:** Critica

Implementare:
- [ ] `StrategyContext` con accesso a:
  - Posizioni correnti
  - Storico barre (lookback window)
  - Indicators cache
  - Parametri strategia

---

#### Task 2.1.3: Signal Validator (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] Validazione segnali (coerenza con posizioni)
- [ ] Rate limiting (evitare spam)
- [ ] Duplicate detection

---

#### Task 2.1.4: Indicators Framework (1h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] Wrapper per crate `ta` o `yata`
- [ ] Caching indicatori per performance
- [ ] Streaming indicator updates

---

### Day 16: Strategy Engine (8h)

#### Task 2.2.1: Strategy Engine Core (5h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/application/src/strategy_engine.rs`

Implementare:
- [ ] `StrategyEngine` che orchestra multiple strategie
- [ ] Event loop che distribuisce eventi alle strategie
- [ ] Routing barre/ticks alle strategie interessate
- [ ] Gestione lifecycle strategie (start, stop, pause)

```rust
pub struct StrategyEngine {
    strategies: Vec<Box<dyn Strategy>>,
    data_provider: Arc<dyn MarketDataProvider>,
    trading_service: Arc<TradingService>,
    event_bus: Arc<EventBus>,
    running: AtomicBool,
}

impl StrategyEngine {
    pub async fn run(mut self) -> Result<(), EngineError> {
        let mut events = self.event_bus.subscribe();
        
        while self.running.load(Ordering::Relaxed) {
            match events.recv().await {
                Ok(TradingEvent::BarReceived { bar }) => {
                    self.process_bar(bar).await?;
                }
                Ok(TradingEvent::OrderFilled { fill }) => {
                    self.process_fill(fill).await?;
                }
                Ok(TradingEvent::PositionUpdated { symbol, position }) => {
                    self.process_position_update(symbol, position).await?;
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    async fn process_bar(&self, bar: Bar) -> Result<(), EngineError> {
        for strategy in &self.strategies {
            if strategy.symbols().contains(&bar.symbol) {
                let ctx = self.build_context().await?;
                
                if let Some(signal) = strategy.on_bar(&bar, &ctx).await {
                    if let Err(e) = self.execute_signal(signal).await {
                        error!("Failed to execute signal: {}", e);
                    }
                }
            }
        }
        Ok(())
    }
}
```

---

#### Task 2.2.2: Signal Executor (2h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] Conversione Signal → Order
- [ ] Gestione sizing (fixed, risk-based)
- [ ] Conferma esecuzione

---

#### Task 2.2.3: Strategy State Management (1h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] Persistenza stato strategie
- [ ] Recovery dopo restart
- [ ] Hot-reload configurazioni

---

### Day 17: Example Strategy Template (4h)

#### Task 2.3.1: "Pass Through" Example Strategy (2h)
**Assegnazione:** Backend
**Priorità:** Alta
**File:** `crates/application/src/strategies/example_template.rs`

Implementare una strategy di esempio MINIMALE che:
- [ ] Implementa il trait `Strategy`
- [ ] Logga eventi ricevuti (barre, tick, fills)
- [ ] NON contiene logica di trading (solo template)
- [ ] Mostra come accedere a StrategyContext
- [ ] Documenta il lifecycle (init → run → shutdown)

```rust
// Esempio - solo template educativo
pub struct ExampleStrategy {
    config: StrategyConfig,
}

#[async_trait]
impl Strategy for ExampleStrategy {
    fn name(&self) -> &str { "Example_Template" }
    
    async fn on_bar(&mut self, bar: &Bar, ctx: &StrategyContext) -> Option<Signal> {
        // Template: logga solo, nessuna logica di trading
        info!("Received bar: {:?}", bar);
        None // Nessun segnale
    }
}
```

**Nota:** Questo è SOLO un template per dimostrare l'interfaccia.\
La logica di trading è responsabilità dell'utente della piattaforma.

---

#### Task 2.3.2: Strategy Loader (2h)
**Assegnazione:** Backend
**Priorità:** Alta

Implementare sistema per caricare strategie esterne:
- [ ] Plugin system (dynamic loading opzionale)
- [ ] Configuration-based strategy registration
- [ ] Hot-swap capability (reload senza restart)

```yaml
# config/strategies.yaml
strategies:
  - name: "MyCustomStrategy"
    path: "/path/to/strategy.so"  # o wasm, o config
    symbols: ["NQ", "ES"]
    params:
      param1: 100
```

---

#### Task 2.3.3: Strategy Backtesting Interface (4h)
**Assegnazione:** Backend
**Priorità:** Media

Implementare INFRASTRUTTURA per backtesting (non strategie):
- [ ] Historical data replay engine
- [ ] Simulated execution with configurable latency/slippage
- [ ] Metrics calculation framework (P&L, drawdown, etc.)
- [ ] Export risultati (CSV, JSON)

**Agnostic:** Il backtester esegue QUALUNQUE strategia che implementi il trait.

---

### Day 18-19: Paper Trading Integration (16h)

#### Task 2.4.1: Paper Trading Mode (6h)
**Assegnazione:** Backend  
**Priorità:** Critica

Implementare:
- [ ] Modalità paper trading completa
- [ ] Simulazione fill realistico (slippage, latency)
- [ ] Tracking performance
- [ ] Export risultati

---

#### Task 2.4.2: Strategy Configuration (4h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] Configurazione strategie via YAML/JSON
- [ ] Validation configurazioni
- [ ] Hot-reload
- [ ] Secrets management (API keys)

---

#### Task 2.4.3: CLI Strategy Commands (3h)
**Assegnazione:** Backend  
**Priorità:** Alta

Implementare:
- [ ] `run-strategy` subcommand
- [ ] `backtest` subcommand
- [ ] `list-strategies` subcommand
- [ ] `strategy-status` subcommand

**Acceptance:**
```bash
cargo run --bin cli -- run-strategy --name MA_Cross --symbol NQ --mode paper --duration 4h
# [STRATEGY] MA_Cross_NQ activated
# [SIGNAL] Buy NQ @ 18234.50 (Fast MA crossed above Slow)
# [PAPER] Order filled @ 18234.50
# [POSITION] NQ +1, P&L: +$125.00
```

---

#### Task 2.4.4: Strategy Testing (3h)
**Assegnazione:** QA/Backend  
**Priorità:** Alta

- [ ] Unit tests strategie
- [ ] Paper trading test
- [ ] Performance comparison

---

### Day 20-21: Polish & Performance (16h)

#### Task 2.5.1: Performance Optimization (4h)
**Assegnazione:** Backend  
**Priorità:** Alta

- [ ] Profiling con flamegraph
- [ ] Ottimizzazione cache indicatori
- [ ] Batch processing dove possibile

---

#### Task 2.5.2: Metrics & Reporting (4h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] Strategy performance metrics
- [ ] Trade journaling
- [ ] Export CSV/JSON

---

#### Task 2.5.3: Integration Tests E2E (4h)
**Assegnazione:** QA/Backend  
**Priorità:** Critica

- [ ] Test E2E strategia completa
- [ ] Test con paper trading
- [ ] Test recovery

---

#### Task 2.5.4: Documentation (4h)
**Assegnazione:** Backend  
**Priorità:** Media

- [ ] Guide scrittura strategie
- [ ] API reference
- [ ] Esempi

---

## FASE 3: GUI & OPERATIONS (Week 4)
**Obiettivo:** Visualizzazione moderna e operations  
**Milestone:** Applicazione desktop funzionante

### Day 22-23: Backend API (16h)

#### Task 3.1.1: REST API (6h)
**Assegnazione:** Backend  
**Priorità:** Critica  
**File:** `crates/infrastructure/src/api/rest.rs`

Implementare endpoint:

**Core Endpoints:**
- [ ] `GET /health` - Health check
- [ ] `GET /positions` - Lista posizioni
- [ ] `GET /orders` - Lista ordini
- [ ] `POST /orders` - Crea ordine
- [ ] `DELETE /orders/{id}` - Cancella ordine
- [ ] `GET /market-data/{symbol}` - Ultimi dati
- [ ] `GET /strategies` - Lista strategie
- [ ] `POST /strategies/{id}/start` - Avvia strategia
- [ ] `POST /strategies/{id}/stop` - Ferma strategia

**IB Account Endpoints:**
- [ ] `GET /account/summary` - IB account summary (NetLiquidation, BuyingPower, etc.)
- [ ] `GET /account/balances` - Currency balances
- [ ] `GET /account/pnl` - Daily P&L (realized + unrealized)
- [ ] `GET /account/margins` - Margin requirements

**IB Portfolio Endpoints:**
- [ ] `GET /portfolio/positions` - Posizioni con cost basis da IB
- [ ] `GET /portfolio/performance` - Performance metrics (Sharpe, Drawdown, etc.)
- [ ] `GET /portfolio/exposure` - Sector/asset class exposure
- [ ] `GET /portfolio/history` - Historical portfolio snapshots

**IB Executions Endpoints:**
- [ ] `GET /executions` - Fill details con commissioni IB
- [ ] `GET /executions/{id}/details` - Dettaglio singola esecuzione

Framework: Axum o Actix-web

---

#### Task 3.1.2: WebSocket Server (5h)
**Assegnazione:** Backend
**Priorità:** Critica
**File:** `crates/infrastructure/src/api/websocket.rs`

Implementare:
- [ ] WebSocket endpoint `/ws`
- [ ] Streaming eventi real-time
- [ ] Subscription management (client può scegliere cosa ricevere)
- [ ] Heartbeat/ping-pong
- [ ] Rate limiting

**Topic IB da streamare:**
- [ ] `account.updates` - NetLiquidation, AvailableFunds, P&L
- [ ] `portfolio.updates` - Posizioni changes, P&L per posizione
- [ ] `orders.updates` - Order status changes
- [ ] `executions.updates` - Fill notifications
- [ ] `market.data` - Barre/tick per simboli sottoscritti

```typescript
// Esempio subscription
{
    "action": "subscribe",
    "topics": ["account.updates", "portfolio.updates", "orders.updates"]
}
```

---

#### Task 3.1.3: gRPC API (Opzionale) (3h)
**Assegnazione:** Backend  
**Priorità:** Bassa

Implementare:
- [ ] Definizione proto
- [ ] Service implementation
- [ ] Client generation

---

#### Task 3.1.4: Authentication (2h)
**Assegnazione:** Backend  
**Priorità:** Media

Implementare:
- [ ] API key authentication
- [ ] Rate limiting per client
- [ ] CORS configuration

---

### Day 24-25: Tauri Setup (16h)

#### Task 3.2.1: Tauri Project Setup (4h)
**Assegnazione:** Frontend  
**Priorità:** Critica

- [ ] Init Tauri v2 con React + TypeScript
- [ ] Configurazione build
- [ ] Setup hot reload
- [ ] Integrazione con workspace Rust

---

#### Task 3.2.2: State Management (3h)
**Assegnazione:** Frontend  
**Priorità:** Alta

Implementare:
- [ ] Zustand o Redux per stato globale
- [ ] WebSocket connection manager
- [ ] Data caching
- [ ] Optimistic updates

---

#### Task 3.2.3: UI Component Library (3h)
**Assegnazione:** Frontend  
**Priorità:** Alta

Setup:
- [ ] Tailwind CSS
- [ ] shadcn/ui o Radix UI components
- [ ] Tema dark mode
- [ ] Responsive layout

---

#### Task 3.2.4: Routing (2h)
**Assegnazione:** Frontend  
**Priorità:** Media

Implementare:
- [ ] React Router setup
- [ ] Route: /dashboard
- [ ] Route: /trading
- [ ] Route: /strategies
- [ ] Route: /settings

---

#### Task 3.2.5: Error Boundaries (2h)
**Assegnazione:** Frontend  
**Priorità:** Media

Implementare:
- [ ] Error boundaries per crash isolation
- [ ] Fallback UI
- [ ] Error reporting

---

#### Task 3.2.6: Loading States (2h)
**Assegnazione:** Frontend  
**Priorità:** Bassa

Implementare:
- [ ] Skeleton screens
- [ ] Loading spinners
- [ ] Progress indicators

---

### Day 26-27: Frontend Components (16h)

#### Task 3.3.1: Chart Component (5h)
**Assegnazione:** Frontend  
**Priorità:** Critica  
**File:** `gui/src/components/Chart.tsx`

Implementare:
- [ ] Lightweight Charts integration
- [ ] OHLCV candlestick
- [ ] Real-time updates via WebSocket
- [ ] Multiple timeframes
- [ ] Drawing tools (optional)
- [ ] Indicators overlay (optional)

```typescript
interface ChartProps {
    symbol: string;
    timeframe: TimeFrame;
    data: Bar[];
    onBarClick?: (bar: Bar) => void;
}
```

---

#### Task 3.3.2: Order Panel (3h)
**Assegnazione:** Frontend  
**Priorità:** Alta  
**File:** `gui/src/components/OrderPanel.tsx`

Implementare:
- [ ] Form creazione ordine
- [ ] Lista ordini aperti
- [ ] Order history
- [ ] Pulsanti cancel/modify
- [ ] Validazione form

---

#### Task 3.3.3: Positions Panel (3h)
**Assegnazione:** Frontend  
**Priorità:** Alta

Implementare:
- [ ] Tabella posizioni
- [ ] P&L real-time
- [ ] Pulsanti close position
- [ ] Aggregation per symbol

---

#### Task 3.3.4: Strategy Panel (3h)
**Assegnazione:** Frontend  
**Priorità:** Alta

Implementare:
- [ ] Lista strategie
- [ ] Toggle start/stop
- [ ] Performance metrics
- [ ] Log strategie

---

#### Task 3.3.5: Market Data Ticker (2h)
**Assegnazione:** Frontend
**Priorità:** Media

Implementare:
- [ ] Ticker prezzi real-time
- [ ] Change/change%
- [ ] Color coding (green/red)
- [ ] Multi-symbol support

---

#### Task 3.3.6: IB Account Monitor (3h)
**Assegnazione:** Frontend
**Priorità:** Alta
**File:** `gui/src/components/IBAccountPanel.tsx`

Implementare monitoraggio account IB in tempo reale:
- [ ] Net Liquidation Value (equity totale)
- [ ] Available Funds (buying power)
- [ ] Maintenance Margin
- [ ] Excess Liquidity
- [ ] Daily P&L (realized + unrealized)
- [ ] Currency breakdown (base + altre)

```typescript
interface IBAccountData {
    netLiquidation: Money;
    availableFunds: Money;
    buyingPower: Money;
    maintenanceMargin: Money;
    excessLiquidity: Money;
    dailyPnL: Money;
    currencies: Map<Currency, CurrencyData>;
}
```

**Source:** Dati da IB via `AccountUpdates` callback

---

#### Task 3.3.7: IB Portfolio Detail Panel (3h)
**Assegnazione:** Frontend
**Priorità:** Alta

Panel dettagliato posizioni con dati IB-specific:
- [ ] Position size e market price
- [ ] Average cost (cost basis da IB)
- [ ] Unrealized P&L per posizione
- [ ] Realized P&L (oggi)
- [ ] Greeks (per opzioni, se presenti)
- [ ] Sector/Asset class breakdown
- [ ] Connettore visualizzazione posizioni IB vs internal tracking

---

#### Task 3.3.8: IB Orders & Execution Monitor (2h)
**Assegnazione:** Frontend
**Priorità:** Alta

Panel ordini con dati esecuzione IB:
- [ ] Lista ordini attivi (da IB)
- [ ] Order status in tempo reale (Submitted, Filled, Cancelled)
- [ ] Execution details (fill price, commissioni IB)
- [ ] Audit trail completo
- [ ] Mismatch detection (ordini locali vs ordini IB)

---

### Day 28-29: Dashboard & Emergency (16h)

#### Task 3.4.1: Main Dashboard (6h)
**Assegnazione:** Frontend  
**Priorità:** Critica  
**File:** `gui/src/pages/Dashboard.tsx`

Implementare:
- [ ] Layout responsive
- [ ] Grid layout (charts + panels)
- [ ] Connection status indicator
- [ ] System health widget
- [ ] Account summary
- [ ] Recent activity feed

```
┌─────────────────────────────────────────────────────┐
│  Header (Connection Status | Account | Settings)    │
├──────────────────┬──────────────────────────────────┤
│  Market Ticker   │  Positions Summary               │
├──────────────────┼──────────────────────────────────┤
│                  │                                  │
│  Chart NQ        │  Order Panel                     │
│                  │                                  │
├──────────────────┤                                  │
│                  ├──────────────────────────────────┤
│  Chart ES        │  Strategy Panel                  │
│                  │                                  │
├──────────────────┴──────────────────────────────────┤
│  Emergency Panel                                    │
└─────────────────────────────────────────────────────┘
```

---

#### Task 3.4.2: Emergency Controls (4h)
**Assegnazione:** Frontend  
**Priorità:** Critica  
**File:** `gui/src/components/EmergencyPanel.tsx`

Implementare:
- [ ] **FLATTEN ALL**: Chiude tutte le posizioni immediatamente
- [ ] **KILL SWITCH**: Cancella tutti gli ordini, ferma tutte le strategie
- [ ] **PANIC STOP**: Disconnette tutto, salva stato
- [ ] Conferma modale con type "FLATTEN ALL" per evitare errori
- [ ] Shortcut tastiera (Ctrl+Shift+F, Ctrl+Shift+K)
- [ ] Logging di ogni azione

```typescript
export function EmergencyPanel() {
    const handleFlattenAll = async () => {
        const confirmed = await confirmDialog({
            title: 'EMERGENCY: Flatten All Positions',
            message: 'This will MARKET CLOSE all open positions immediately. Continue?',
            confirmText: 'FLATTEN ALL',
            type: 'danger'
        });
        
        if (confirmed) {
            await api.emergency.flattenAll();
            toast.success('All positions flattened');
        }
    };
    
    return (
        <div className="emergency-panel">
            <Button variant="warning" onClick={handleFlattenAll}>
                ⚠️ FLATTEN ALL
            </Button>
            <Button variant="danger" onClick={handleKillSwitch}>
                🛑 KILL SWITCH
            </Button>
        </div>
    );
}
```

---

#### Task 3.4.3: Settings Panel (3h)
**Assegnazione:** Frontend  
**Priorità:** Media

Implementare:
- [ ] API connection settings
- [ ] Risk limits configuration
- [ ] Display preferences
- [ ] Export/import settings

---

#### Task 3.4.4: Notifications (3h)
**Assegnazione:** Frontend  
**Priorità:** Media

Implementare:
- [ ] Toast notifications
- [ ] Order fill alerts
- [ ] Error notifications
- [ ] Configurable notification rules

---

### Day 30: Build & Polish (8h)

#### Task 3.5.1: Production Build (3h)
**Assegnazione:** DevOps  
**Priorità:** Critica
- [ ] Ottimizzazione build Rust (LTO, strip)
- [ ] Ottimizzazione build frontend
- [ ] Target: binary < 15MB

```bash
# Build ottimizzato
cargo build --release
cd crates/gui && cargo tauri build

# Output atteso:
# - Linux: trading-core_0.1.0_amd64.AppImage (~12MB) o binary standalone
```
```

---

#### Task 3.5.2: E2E Testing (3h)
**Assegnazione:** QA  
**Priorità:** Alta

- [ ] Test flusso completo GUI
- [ ] Test emergency controls
- [ ] Test reconnection
- [ ] Performance test rendering

---

#### Task 3.5.3: Documentation (2h)
**Assegnazione:** Backend/Frontend  
**Priorità:** Media

- [ ] User guide
- [ ] Installation instructions
- [ ] Troubleshooting guide

---

## 7. OPERAZIONI E MANUTENZIONE

### 7.1 Deployment

#### CI/CD Pipeline

```yaml
# .github/workflows/ci.yml
name: CI/CD

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-action@stable
      - run: cargo test --all
      - run: cargo clippy --all -- -D warnings
      - run: cargo fmt --check

  build:
    needs: test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-action@stable
      - run: cargo build --release
```

### 7.2 Monitoring

#### Metriche Chiave

| Metrica | Target | Alert |
|---------|--------|-------|
| Order latency | < 50ms | > 100ms |
| DB write latency | < 10ms | > 50ms |
| Memory usage | < 1GB | > 2GB |
| CPU usage | < 50% | > 80% |
| Connection status | 100% | Disconnect |

### 7.3 Backup & Recovery

```bash
# Backup giornaliero
docker exec timescaledb pg_dump -U trader trading_db > backup_$(date +%Y%m%d).sql

# Restore
psql -h localhost -U trader -d trading_db < backup_YYYYMMDD.sql
```

### 7.4 Security Checklist

- [ ] API keys in environment variables, mai in codice
- [ ] Network isolation (IB Gateway in VLAN separata)
- [ ] Audit log di tutte le operazioni
- [ ] Rate limiting su API pubbliche
- [ ] Input validation su tutti gli endpoint

---

## 8. RIFERIMENTI

### 8.1 Architecture Decision Records (ADRs)

Template ADR:
```markdown
# ADR-001: Scelta TimescaleDB per Storage

## Stato
Accepted

## Contexto
Necessitiamo di storage time-series per dati di mercato.

## Decisione
Utilizzare TimescaleDB su PostgreSQL.

## Conseguenze
- ✅ SQL standard, query familiari
- ✅ Compressione automatica
- ✅ Hypertables per performance
- ❌ Overhead PostgreSQL vs InfluxDB
```

### 8.2 Risorse Utili

- [Tokio Docs](https://docs.rs/tokio/latest/tokio/)
- [ibapi Crate](https://docs.rs/ibapi/latest/ibapi/)
- [TimescaleDB](https://docs.timescale.com/)
- [Tauri v2](https://v2.tauri.app/)

### 8.3 Contatti

- **Tech Lead:** [Nome]
- **Risk Manager:** [Nome]
- **DevOps:** [Nome]

---

*Playbook V2 generato: Marzo 2026*  
*Metodologia: Pragmatic Architecture + Professional Development*
