# Database Migrations

Questo directory contiene le migrazioni del database per ATOM Trading Platform.

## Struttura

```
db/
├── init/
│   └── 01_schema.sql          # Schema iniziale (TimescaleDB)
├── migrations/
│   ├── README.md              # Questo file
│   └── <timestamp>_*.sql      # File di migrazione
```

## Convenzioni Naming

Per sqlx migrate:
```
<timestamp>_<descrizione>.sql
```

Esempio:
```
20260313160000_add_indexes.sql
```

## Comandi SQLx

### Installazione sqlx-cli
```bash
cargo install sqlx-cli --features native-tls,postgres
```

### Creazione nuova migrazione
```bash
sqlx migrate add <descrizione>
```

### Esecuzione migrazioni
```bash
# Setup database URL
export DATABASE_URL="postgres://trader:dev_password@localhost:5432/trading_db"

# Run migrations
sqlx migrate run
```

### Verifica stato
```bash
sqlx migrate info
```

## Migrazioni Manuali (alternativa)

Per migrazioni manuali senza sqlx:

1. Creare file `db/migrations/YYYYMMDD_HHMMSS_descrizione.sql`
2. Eseguire con psql:
   ```bash
   psql -h localhost -U trader -d trading_db -f db/migrations/YYYYMMDDHHMMSS_descrizione.sql
   ```
3. Registrare la migrazione nella tabella `_sqlx_migrations`:
   ```sql
   INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
   VALUES (...);
   ```

## Schema Versioning

| Versione | Data | Descrizione |
|----------|------|-------------|
| 1.0.0 | 2026-03-13 | Schema iniziale con hypertables e compressione |

## Tabelle Hypertable

- `bars` - OHLCV data (compress after 7 days)
- `ticks` - Tick data (compress after 1 day)
- `fills` - Order fills (compress after 30 days)
- `trading_events` - System events (compress after 7 days)

## Tabelle Standard

- `orders` - Active orders
- `positions` - Current positions

## Note TimescaleDB

- Le chiavi primarie su hypertables devono includere la colonna `time`
- La compressione è configurata per bilanciare storage vs performance
- I chunk sono creati con intervallo di 1 giorno
