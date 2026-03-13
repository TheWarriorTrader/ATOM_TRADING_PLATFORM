# Trading Core

[![Rust](https://img.shields.io/badge/Rust-1.85+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()

> High-performance algorithmic trading system built in Rust for professional traders

## 🎯 Overview

Trading Core è un sistema di trading algoritmico end-to-end progettato per il trading ad alta frequenza e la gestione automatizzata di strategie. Costruito con Rust per prestazioni massime e sicurezza memory-safe.

### Caratteristiche Principali

- ⚡ **High Performance**: Async runtime con Tokio, latency sub-millisecond
- 📊 **Market Data**: Ingestione real-time da Interactive Brokers
- 🗄️ **Time-Series Storage**: TimescaleDB per dati di mercato con compressione automatica
- 🎯 **Strategy Engine**: Framework per strategie custom con backtesting
- 🛡️ **Risk Management**: Circuit breaker, position limits, kill switch
- 🖥️ **Desktop GUI**: Interfaccia Tauri v2 moderna e reattiva
- 🔌 **Provider Agnostic**: Facile swap tra provider dati (IB, Databento, etc.)

---

## 🏗️ Architecture

```
trading-core/
├── crates/
│   ├── domain/          # Core business logic (no async/IO)
│   │   └── Entities, Value Objects, Events, Traits
│   ├── application/     # Use cases and orchestration
│   │   └── TradingService, RiskEngine, StrategyEngine
│   ├── infrastructure/  # Concrete implementations
│   │   ├── providers/   # IB, Databento adapters
│   │   ├── persistence/ # TimescaleDB, Redis
│   │   └── api/         # REST, WebSocket, gRPC
│   ├── cli/            # CLI entry point
│   └── gui/            # Tauri v2 desktop app
├── db/                 # Database migrations and schema
├── scripts/            # Setup and utility scripts
├── config/             # Configuration templates
├── docs/               # Additional documentation
├── PLAYBOOK_V2.md      # Development guide
└── TIMELINE_V2.md      # Progress tracking
```

### Layered Architecture

```
┌─────────────────────────────────────────────┐
│  Presentation (Tauri GUI / CLI)            │
├─────────────────────────────────────────────┤
│  Application (Trading, Risk, Strategy)     │
├─────────────────────────────────────────────┤
│  Domain (Entities, Events, Traits)         │
├─────────────────────────────────────────────┤
│  Infrastructure (DB, Providers, API)       │
└─────────────────────────────────────────────┘
```

---

## 🚀 Quick Start

### Prerequisites

- **OS**: Ubuntu 24.04 LTS (x86_64)
- **RAM**: 8GB+ (16GB consigliati)
- **Disk**: 50GB+ disponibili
- **Rust**: 1.85+ (via rustup)
- **Docker**: 24.0+ con Docker Compose

### Installation

```bash
# 1. Clone repository
git clone https://github.com/yourusername/trading-core.git
cd trading-core

# 2. Verify environment
./scripts/check-env.sh

# 3. Start infrastructure services
docker compose up -d

# 4. Build project
cargo build --release

# 5. Run tests
cargo test --workspace

# 6. Start CLI
cargo run --bin cli -- --help
```

### Configuration

```bash
# Copy example configuration
cp .env.example .env

# Edit configuration
vim .env

# Required settings:
# - DATABASE_URL=postgres://trader:password@localhost:5432/trading_db
# - REDIS_URL=redis://localhost:6379
# - IB_GATEWAY_HOST=localhost
# - IB_GATEWAY_PORT=7496
```

---

## 📖 Usage

### CLI Commands

```bash
# Data ingestion
cargo run --bin cli -- ingest --symbol NQ --duration 1h

# Place order
cargo run --bin cli -- place-order \
    --symbol NQ \
    --side Buy \
    --quantity 1 \
    --order-type Market

# Run strategy on paper trading
cargo run --bin cli -- run-strategy \
    --name MA_Crossover \
    --symbol NQ \
    --mode paper

# Monitor positions
cargo run --bin cli -- positions

# Emergency: flatten all positions
cargo run --bin cli -- emergency flatten-all
```

### GUI Application

```bash
# Start Tauri development server
cd crates/gui
cargo tauri dev

# Build production binary
cargo tauri build
```

---

## 🛠️ Development

### Setup Development Environment

See [PLAYBOOK_V2.md](PLAYBOOK_V2.md) Section 0 for detailed setup instructions.

### Common Commands

```bash
# Start all services
make dev

# Run linting and formatting
make lint
make fmt

# Run tests with coverage
cargo tarpaulin --out Html

# Generate documentation
cargo doc --workspace --open

# Watch mode for development
cargo watch -x 'run --bin cli'
```

### Project Structure

| Crate | Purpose | Public API |
|-------|---------|------------|
| `domain` | Business logic | Entities, Events, Traits |
| `application` | Use cases | TradingService, RiskEngine |
| `infrastructure` | Implementations | Repositories, Providers |
| `cli` | Command line | CLI commands |
| `gui` | Desktop app | Tauri frontend |

---

## 🧪 Testing

```bash
# Unit tests
cargo test --lib

# Integration tests
cargo test --test '*'

# Specific crate tests
cargo test -p domain
cargo test -p application

# E2E tests
cargo test --test e2e

# With coverage
cargo tarpaulin --ignore-tests
```

---

## 📊 Performance Benchmarks

| Metric | Target | Status |
|--------|--------|--------|
| Order latency | < 10ms | ✅ |
| Market data throughput | > 10k msgs/sec | ✅ |
| DB write latency | < 5ms | ✅ |
| Memory footprint | < 1GB | ✅ |
| Binary size | < 15MB | ✅ |

---

## 📚 Documentation

- **[PLAYBOOK_V2.md](PLAYBOOK_V2.md)** - Complete development guide with 119 tasks
- **[TIMELINE_V2.md](TIMELINE_V2.md)** - Project progress tracking
- **[API Documentation](target/doc)** - Generated with `cargo doc`
- **[Architecture Decision Records](docs/adr/)** - Design decisions log

---

## 🔧 Troubleshooting

### Common Issues

**Build fails with linker error:**
```bash
sudo apt install build-essential libssl-dev pkg-config
```

**PostgreSQL connection refused:**
```bash
docker compose up -d postgres
pg_isready -h localhost -p 5432 -U trader
```

**Port already in use:**
```bash
sudo lsof -i :5432  # Find process
sudo kill -9 <PID>   # Kill process
```

**Rust not found:**
```bash
source $HOME/.cargo/env
```

### Getting Help

- Check [TIMELINE_V2.md](TIMELINE_V2.md) for known issues
- Review [PLAYBOOK_V2.md](PLAYBOOK_V2.md) troubleshooting section
- Open an issue with logs and reproduction steps

---

## 🗺️ Roadmap

### Phase 0: Foundation ✅
- [x] Project setup and architecture
- [x] Docker infrastructure
- [x] Domain layer implementation
- [x] IB market data integration

### Phase 1: Execution Core 🚧
- [ ] Order management system
- [ ] Risk engine
- [ ] Paper trading gateway

### Phase 2: Strategy Engine 📋
- [ ] Strategy framework
- [ ] Backtesting engine
- [ ] Performance analytics

### Phase 3: GUI & Operations 📋
- [ ] Tauri desktop app
- [ ] Real-time charts
- [ ] Emergency controls

---

## 🤝 Contributing

1. Fork the repository
2. Create feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open Pull Request

See [PLAYBOOK_V2.md](PLAYBOOK_V2.md) for coding standards and development workflow.

---

## ⚠️ Disclaimer

**Trading involves substantial risk of loss.** This software is provided for educational and research purposes. Always test strategies thoroughly on paper trading before live deployment. Past performance does not guarantee future results.

---

## 📄 License

This project is licensed under the MIT License - see [LICENSE](LICENSE) file for details.

---

## 🙏 Acknowledgments

- [Tokio](https://tokio.rs/) - Async runtime
- [Tauri](https://tauri.app/) - Desktop framework
- [TimescaleDB](https://www.timescale.com/) - Time-series database
- [Interactive Brokers](https://www.interactivebrokers.com/) - Market data and execution

---

<p align="center">
  Built with ❤️ and ☕ in Rust
</p>
