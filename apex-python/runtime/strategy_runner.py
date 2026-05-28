"""Entry-point for a strategy subprocess.

Launched by :mod:`runtime.sandbox` as::

    python -m runtime.strategy_runner --id <strategy_id> --script <path>

The runner:

1. Connects to the sidecar IPC socket.
2. Dynamically loads the user script.
3. Instantiates the first :class:`Strategy` subclass found.
4. Calls ``on_init``, then enters a loop dispatching ``on_bar`` / ``on_tick``
   messages from the sidecar until told to stop.
"""

from __future__ import annotations

import argparse
import importlib.util
import inspect
import json
import logging
import os
import sys
import time
from types import ModuleType
from typing import Any

from apex_sdk.strategy import Strategy
from apex_sdk.types import Bar, Tick, Timeframe

_SIDECAR_IMPORT_ERROR: Exception | None = None

try:
    from runtime.sidecar import IPCClient
except Exception as exc:  # noqa: BLE001
    IPCClient = None  # type: ignore[assignment]
    _SIDECAR_IMPORT_ERROR = exc

logger = logging.getLogger(__name__)

LATENCY_WARN_NS: int = 1_000_000  # 1 ms


class _NoopIPCClient:
    """Fallback IPC client used when the sidecar transport is unavailable."""

    def send(self, method: str, params: dict[str, Any]) -> None:
        logger.debug("Dropping IPC message %s with params %s", method, params)


# ------------------------------------------------------------------
# Script loading
# ------------------------------------------------------------------


def _load_strategy_module(script_path: str) -> ModuleType:
    """Import a user strategy file by filesystem path."""
    spec = importlib.util.spec_from_file_location("user_strategy", script_path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load strategy script: {script_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _find_strategy_class(module: ModuleType) -> type[Strategy]:
    """Return the first ``Strategy`` subclass defined in *module*."""
    for _name, obj in inspect.getmembers(module, inspect.isclass):
        if issubclass(obj, Strategy) and obj is not Strategy:
            return obj
    raise TypeError(
        f"No Strategy subclass found in {module.__file__}"
    )


# ------------------------------------------------------------------
# Event dispatch with latency guard
# ------------------------------------------------------------------


def _dispatch_event(
    strategy: Strategy,
    method: str,
    payload: dict[str, Any],
) -> None:
    """Call the matching strategy hook, warning if it exceeds 1 ms."""
    start = time.perf_counter_ns()

    if method == "bar":
        bar = Bar(
            symbol=payload["symbol"],
            timeframe=Timeframe(payload["timeframe"]),
            open=payload["open"],
            high=payload["high"],
            low=payload["low"],
            close=payload["close"],
            volume=payload["volume"],
            timestamp_ns=payload["timestamp_ns"],
        )
        strategy.on_bar(payload["symbol"], bar)
    elif method == "tick":
        tick = Tick(
            symbol=payload["symbol"],
            price=payload["price"],
            size=payload["size"],
            timestamp_ns=payload["timestamp_ns"],
        )
        strategy.on_tick(payload["symbol"], tick)
    else:
        logger.warning("Unknown event method: %s", method)
        return

    elapsed_ns = time.perf_counter_ns() - start
    if elapsed_ns > LATENCY_WARN_NS:
        logger.warning(
            "Strategy handler %s took %.3f ms (limit 1 ms)",
            method,
            elapsed_ns / 1_000_000,
        )


# ------------------------------------------------------------------
# Main
# ------------------------------------------------------------------


def main(argv: list[str] | None = None) -> None:
    """Parse args, load strategy, and run the event loop."""
    parser = argparse.ArgumentParser(description="APEX strategy subprocess runner")
    parser.add_argument("--id", required=True, help="Strategy ID")
    parser.add_argument("--script", required=True, help="Path to strategy .py file")
    parser.add_argument(
        "--socket",
        default=None,
        help="IPC socket path (default: $APEX_SIDECAR_SOCKET)",
    )
    parser.add_argument(
        "--params",
        default="{}",
        help="JSON params passed to strategy.on_init()",
    )
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    socket_path = args.socket or os.environ.get(
        "APEX_SIDECAR_SOCKET", "/tmp/apex_strategy.sock"
    )

    try:
        params = json.loads(args.params or "{}")
    except json.JSONDecodeError as exc:
        logger.error("Invalid --params JSON: %s", exc)
        raise SystemExit(2) from exc

    if not isinstance(params, dict):
        logger.error("--params must decode to a JSON object")
        raise SystemExit(2)

    if IPCClient is None:
        logger.warning(
            "runtime.sidecar unavailable (%s); using no-op IPC client",
            _SIDECAR_IMPORT_ERROR,
        )
        ipc_client = _NoopIPCClient()
    else:
        ipc_client = IPCClient(socket_path)

    try:
        module = _load_strategy_module(args.script)
        cls = _find_strategy_class(module)
        strategy = cls(strategy_id=args.id, ipc_client=ipc_client)

        logger.info("Initializing strategy %s (%s)", args.id, cls.__name__)
        strategy.on_init(params)

        # Event-loop wiring is still pending; for IDE execution we at least
        # perform init/stop hooks so strategies can validate imports, params,
        # subscriptions, and logging behavior.
        logger.info(
            "Strategy %s initialized — no event stream configured, shutting down",
            args.id,
        )
        strategy.on_stop()
        logger.info("Strategy %s stopped", args.id)
    except Exception as exc:  # noqa: BLE001
        logger.exception("Strategy %s failed", args.id)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
