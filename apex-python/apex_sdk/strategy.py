"""Base class for all APEX strategies.

Users subclass :class:`Strategy` and override the event hooks
(:meth:`on_init`, :meth:`on_bar`, :meth:`on_tick`, :meth:`on_stop`).

``on_bar`` / ``on_tick`` must complete in < 1 ms — the runner will emit a
warning (not an error) if the deadline is exceeded so that latency regressions
are visible in logs without silently killing a strategy.
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING

from apex_sdk.types import Bar, Signal, Tick, Timeframe

if TYPE_CHECKING:
    from runtime.sidecar import IPCClient


class Strategy:
    """Base class for all APEX strategies.  Users subclass this."""

    def __init__(self, strategy_id: str, ipc_client: IPCClient) -> None:
        self._id: str = strategy_id
        self._ipc: IPCClient = ipc_client
        self._subscriptions: list[tuple[list[str], Timeframe]] = []
        self._indicator_cache: dict[tuple[str, str, tuple[object, ...]], float] = {}

    # ------------------------------------------------------------------
    # Public helpers
    # ------------------------------------------------------------------

    def subscribe(self, symbols: list[str], timeframe: Timeframe) -> None:
        """Register interest in *symbols* at *timeframe*."""
        self._subscriptions.append((symbols, timeframe))

    def indicator(self, name: str, symbol: str, *params: object) -> float:
        """Return cached indicator value for the current bar, or NaN."""
        key = (name, symbol, params)
        return self._indicator_cache.get(key, math.nan)

    def emit(self, signal: Signal) -> None:
        """Send a trading signal to the Rust OTM."""
        self._ipc.send(
            "emit_signal",
            {"strategy_id": self._id, "signal": signal.to_dict()},
        )

    def log(self, message: str) -> None:
        """Forward a log line to the Rust core via IPC."""
        self._ipc.send(
            "strategy_log",
            {"strategy_id": self._id, "message": message},
        )

    # ------------------------------------------------------------------
    # Event hooks — override in subclasses
    # ------------------------------------------------------------------

    def on_init(self, params: dict[str, object]) -> None:  # noqa: ARG002
        """Called once when the strategy subprocess starts."""

    def on_bar(self, symbol: str, bar: Bar) -> None:  # noqa: ARG002
        """Called on every new bar.  Must complete in < 1 ms."""

    def on_tick(self, symbol: str, tick: Tick) -> None:  # noqa: ARG002
        """Called on every tick.  Must complete in < 1 ms."""

    def on_stop(self) -> None:
        """Called when the strategy is being shut down."""
