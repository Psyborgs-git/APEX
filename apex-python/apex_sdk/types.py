"""Canonical data types shared between the SDK, sidecar, and ML layer."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from enum import Enum
from typing import Callable, TypeVar


_T = TypeVar("_T")


def _slotted_dataclass(*, frozen: bool = False) -> Callable[[type[_T]], type[_T]]:
    """Use slotted dataclasses when supported, with a safe fallback otherwise."""

    def decorator(cls: type[_T]) -> type[_T]:
        try:
            return dataclass(frozen=frozen, slots=True)(cls)
        except TypeError:
            return dataclass(frozen=frozen)(cls)

    return decorator


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


@_slotted_dataclass(frozen=True)
class Tick:
    """A single price tick from a market data feed."""

    symbol: str
    price: float
    size: float
    timestamp_ns: int


@_slotted_dataclass(frozen=True)
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


@_slotted_dataclass()
class Signal:
    """A trading signal emitted by a strategy to the Rust OTM."""

    symbol: str
    direction: str  # "long" | "short" | "flat"
    strength: float  # 0.0 – 1.0
    metadata: dict[str, object] = field(default_factory=dict)

    def to_dict(self) -> dict[str, object]:
        """Serialise to a plain dict suitable for msgpack transport."""
        return asdict(self)
