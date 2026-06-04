# APEX System Architecture

## Executive Summary

APEX is a high-performance, locally-hosted algorithmic trading terminal engineered for absolute speed, reliability, and data privacy. Designed for institutional-grade quantitative trading without the institutional lock-in, APEX executes completely on the local machine to avoid latency overheads and protect proprietary algorithms and portfolio data.

The system features sub-millisecond internal messaging, lock-free concurrency in its critical paths, and an aggressively optimized rust core. Its primary responsibilities include ingesting high-throughput market data from diverse sources, rapidly computing technical indicators and machine learning model inferences, enforcing strict pre-trade risk validations, and dispatching orders via a unified broker adapter interface.

## System Architecture Diagram

```mermaid
graph TD
    subgraph UI["UI Layer (Tauri + React)"]
        UI_App[Desktop Shell]
        UI_Charts[Charting & Visualization]
        UI_IDE[Strategy IDE]
    end

    subgraph Core["Application Core (Rust)"]
        MDA[Market Data Aggregator]
        OTM[Order & Trade Manager]
        SE[Strategy Orchestrator]
        RE[Risk Engine]
        NE[News Engine]
        AE[Alert Engine]
        MBus((Internal Message Bus))
    end

    subgraph ML["Python Sidecar (ML & Strategy Runtime)"]
        ML_Models[scikit-learn / PyTorch]
        Strat_Scripts[Custom Python Scripts]
    end

    subgraph Ports["Port Interfaces"]
        Port_MD[MarketDataPort]
        Port_Ex[ExecutionPort]
        Port_St[StoragePort]
        Port_Nw[NewsPort]
    end

    subgraph Adapters["Adapter Implementations"]
        Adapter_MD[Zerodha / Yahoo / Binance]
        Adapter_Ex[IBKR / Alpaca / Paper]
        Adapter_St[TimescaleDB / Redis / DuckDB]
        Adapter_Nw[RSS / Alpha Vantage]
    end

    %% Flow UI to Core
    UI_App <-->|Tauri IPC| MBus
    UI_IDE --> SE
    UI_Charts <--> MBus

    %% Flow Core to Core
    MDA -->|Publishes Ticks| MBus
    OTM -->|Publishes Updates| MBus
    NE -->|Publishes News| MBus
    AE -->|Publishes Alerts| MBus
    SE -->|Emits Signals| OTM
    OTM <-->|Risk Checks| RE

    %% Flow Core to Python Sidecar
    SE <-->|Unix Socket / msgpack| Strat_Scripts
    MBus -->|Tick Streams| Strat_Scripts
    Strat_Scripts <--> ML_Models

    %% Flow Core to Ports
    MDA --> Port_MD
    OTM --> Port_Ex
    MDA --> Port_St
    OTM --> Port_St
    NE --> Port_Nw

    %% Flow Ports to Adapters
    Port_MD -.-> Adapter_MD
    Port_Ex -.-> Adapter_Ex
    Port_St -.-> Adapter_St
    Port_Nw -.-> Adapter_Nw
```

## Design Philosophy

### Hexagonal Architecture (Ports & Adapters)
APEX isolates all business logic from external dependencies using the Ports & Adapters pattern. The core domain defines Rust traits (**MarketDataPort**, **ExecutionPort**, **StoragePort**, **NewsPort**) that define *what* the system needs. Adapters implement *how* those needs are fulfilled (e.g., fetching from Zerodha, storing in TimescaleDB). Adding a new broker or data feed requires zero changes to the core trading engine.

### Speed First, Ergonomics Second
The critical path—from receiving a market data tick to emitting an execution signal—must execute in under 1 millisecond. To achieve this, APEX employs:
*   **Lock-free Data Structures:** `crossbeam` queues and atomic operations prevent thread contention on the hot path.
*   **Asynchronous I/O:** `tokio` manages thousands of concurrent data streams without blocking execution threads.
*   **In-Memory Caching:** Hot state like the **Quote** cache and open positions reside in concurrent `DashMap` instances, bypassing database round-trips for immediate retrieval.

### Fail Loudly, Recover Silently
Reliability is prioritized through aggressive failure containment. Every external adapter is wrapped in a state-machine based Circuit Breaker. If a broker's API degrades, the circuit opens, preventing cascading timeouts and allowing failover logic to take over. The system journals state transitions to SQLite, enabling a silent, consistent recovery of position state upon restart.

### Local-First and Privacy-Centric
All sensitive data—API keys, trading algorithms, portfolio positions, and historical market data—remains strictly on the host machine. The system integrates embedded and local databases (DuckDB, SQLite, local Redis, TimescaleDB) to deliver analytical capabilities matching cloud-hosted platforms without the data exfiltration risks.