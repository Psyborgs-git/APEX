"""Tests for the Strategy base class and SDK types."""

from __future__ import annotations

import math
from unittest.mock import MagicMock

from apex_sdk.strategy import Strategy
from apex_sdk.types import Bar, Signal, Tick, Timeframe


# ------------------------------------------------------------------
# Helpers
# ------------------------------------------------------------------


def _make_strategy(strategy_id: str = "test-1") -> tuple[Strategy, MagicMock]:
    ipc = MagicMock()
    return Strategy(strategy_id=strategy_id, ipc_client=ipc), ipc


class DummyStrategy(Strategy):
    """Minimal concrete strategy for testing."""

    def __init__(self, *args: object, **kwargs: object) -> None:
        super().__init__(*args, **kwargs)  # type: ignore[arg-type]
        self.bars_received: list[Bar] = []
        self.ticks_received: list[Tick] = []

    def on_bar(self, symbol: str, bar: Bar) -> None:
        self.bars_received.append(bar)

    def on_tick(self, symbol: str, tick: Tick) -> None:
        self.ticks_received.append(tick)


# ------------------------------------------------------------------
# Strategy tests
# ------------------------------------------------------------------


class TestStrategy:
    def test_subscribe_records_symbols(self) -> None:
        strat, _ = _make_strategy()
        strat.subscribe(["AAPL", "GOOG"], Timeframe.M5)
        assert len(strat._subscriptions) == 1
        assert strat._subscriptions[0] == (["AAPL", "GOOG"], Timeframe.M5)

    def test_indicator_returns_nan_on_miss(self) -> None:
        strat, _ = _make_strategy()
        assert math.isnan(strat.indicator("rsi", "AAPL", 14))

    def test_indicator_returns_cached_value(self) -> None:
        strat, _ = _make_strategy()
        strat._indicator_cache[("rsi", "AAPL", (14,))] = 63.5
        assert strat.indicator("rsi", "AAPL", 14) == 63.5

    def test_emit_sends_signal_via_ipc(self) -> None:
        strat, ipc = _make_strategy()
        sig = Signal(symbol="AAPL", direction="long", strength=0.8)
        strat.emit(sig)
        ipc.send.assert_called_once_with(
            "emit_signal",
            {"strategy_id": "test-1", "signal": sig.to_dict()},
        )

    def test_log_sends_message_via_ipc(self) -> None:
        strat, ipc = _make_strategy()
        strat.log("hello")
        ipc.send.assert_called_once_with(
            "strategy_log",
            {"strategy_id": "test-1", "message": "hello"},
        )

    def test_on_bar_default_is_noop(self) -> None:
        strat, _ = _make_strategy()
        bar = Bar("AAPL", Timeframe.M1, 100, 101, 99, 100.5, 1000, 0)
        strat.on_bar("AAPL", bar)  # should not raise

    def test_on_tick_default_is_noop(self) -> None:
        strat, _ = _make_strategy()
        tick = Tick("AAPL", 100.0, 10.0, 0)
        strat.on_tick("AAPL", tick)  # should not raise


# ------------------------------------------------------------------
# Subclass tests
# ------------------------------------------------------------------


class TestDummyStrategy:
    def test_on_bar_collects_bars(self) -> None:
        strat = DummyStrategy(strategy_id="d-1", ipc_client=MagicMock())
        bar = Bar("TSLA", Timeframe.H1, 200, 210, 195, 205, 5000, 1)
        strat.on_bar("TSLA", bar)
        assert strat.bars_received == [bar]

    def test_on_tick_collects_ticks(self) -> None:
        strat = DummyStrategy(strategy_id="d-1", ipc_client=MagicMock())
        tick = Tick("TSLA", 205.0, 50.0, 1)
        strat.on_tick("TSLA", tick)
        assert strat.ticks_received == [tick]


# ------------------------------------------------------------------
# Types tests
# ------------------------------------------------------------------


class TestTypes:
    def test_signal_to_dict(self) -> None:
        sig = Signal(symbol="SPY", direction="short", strength=0.6, metadata={"reason": "RSI"})
        d = sig.to_dict()
        assert d["symbol"] == "SPY"
        assert d["direction"] == "short"
        assert d["strength"] == 0.6
        assert d["metadata"]["reason"] == "RSI"

    def test_bar_is_frozen(self) -> None:
        bar = Bar("AAPL", Timeframe.D1, 100, 110, 90, 105, 1e6, 0)
        try:
            bar.open = 999  # type: ignore[misc]
            raise AssertionError("Expected FrozenInstanceError")
        except AttributeError:
            pass

    def test_tick_is_frozen(self) -> None:
        tick = Tick("AAPL", 100.0, 10.0, 0)
        try:
            tick.price = 999  # type: ignore[misc]
            raise AssertionError("Expected FrozenInstanceError")
        except AttributeError:
            pass

    def test_timeframe_values(self) -> None:
        assert Timeframe.M1.value == "1m"
        assert Timeframe.D1.value == "1d"
