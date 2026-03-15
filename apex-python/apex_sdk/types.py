"""Canonical data types shared between the SDK, sidecar, and ML layer."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from enum import Enum


class Timeframe(str, Enum):
    """Supported bar timeframes."""

    S1 = "1s"
    M1 = "1m"
    M5 = "5m"
    M15 = "15m"
    H1 = "1h"
    H4 = "4h"
    D1 = "1d"
    W1 = "1w"


@dataclass(frozen=True, slots=True)
class Tick:
    """A single price tick from a market data feed."""

    symbol: str
    price: float
    size: float
    timestamp_ns: int


@dataclass(frozen=True, slots=True)
class Bar:
    """An OHLCV bar for a given symbol and timeframe."""

    symbol: str
    timeframe: Timeframe
    open: float
    high: float
    low: float
    close: float
    volume: float
    timestamp_ns: int


@dataclass(slots=True)
class Signal:
    """A trading signal emitted by a strategy to the Rust OTM."""

    symbol: str
    direction: str  # "long" | "short" | "flat"
    strength: float  # 0.0 – 1.0
    metadata: dict[str, object] = field(default_factory=dict)

    def to_dict(self) -> dict[str, object]:
        """Serialise to a plain dict suitable for msgpack transport."""
        return asdict(self)
