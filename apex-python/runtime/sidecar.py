"""Async sidecar process that bridges Rust core ↔ Python strategies/ML.

Communication uses a Unix domain socket with length-prefixed msgpack frames:

    ┌──────────┬───────────────────────┐
    │ 4 bytes  │  N bytes              │
    │ big-end  │  msgpack payload      │
    │ length   │                       │
    └──────────┴───────────────────────┘

Request:  ``{ "id": <uuid>, "method": <str>, "params": <dict> }``
Response: ``{ "id": <uuid>, "result": <any> }``  or
          ``{ "id": <uuid>, "error": <str> }``
"""

from __future__ import annotations

import asyncio
import logging
import os
import struct
import uuid
from pathlib import Path
from typing import Any, Callable, Coroutine

import msgpack

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# IPC Client (used by strategies inside subprocesses)
# ---------------------------------------------------------------------------


class IPCClient:
    """Blocking client that sends one-shot messages over a Unix domain socket."""

    def __init__(self, socket_path: str | Path) -> None:
        self._socket_path = str(socket_path)

    def send(self, method: str, params: dict[str, Any]) -> Any:
        """Send a request and return the result (blocking)."""
        return asyncio.get_event_loop().run_until_complete(
            self._async_send(method, params),
        )

    async def _async_send(self, method: str, params: dict[str, Any]) -> Any:
        reader, writer = await asyncio.open_unix_connection(self._socket_path)
        try:
            msg_id = str(uuid.uuid4())
            payload = msgpack.packb(
                {"id": msg_id, "method": method, "params": params},
                use_bin_type=True,
            )
            writer.write(struct.pack(">I", len(payload)) + payload)
            await writer.drain()

            length_bytes = await reader.readexactly(4)
            length = struct.unpack(">I", length_bytes)[0]
            data = await reader.readexactly(length)
            response: dict[str, Any] = msgpack.unpackb(data, raw=False)

            if "error" in response:
                raise RuntimeError(response["error"])
            return response.get("result")
        finally:
            writer.close()
            await writer.wait_closed()


# ---------------------------------------------------------------------------
# Method dispatcher
# ---------------------------------------------------------------------------

DispatchHandler = Callable[..., Coroutine[Any, Any, Any]]

_dispatch_table: dict[str, DispatchHandler] = {}


def register_method(name: str) -> Callable[[DispatchHandler], DispatchHandler]:
    """Decorator that registers *func* as a handler for *name*."""

    def _decorator(func: DispatchHandler) -> DispatchHandler:
        _dispatch_table[name] = func
        return func

    return _decorator


async def dispatch(request: dict[str, Any]) -> dict[str, Any]:
    """Route an incoming request to the matching handler."""
    msg_id: str = request.get("id", "")
    method: str = request.get("method", "")
    params: dict[str, Any] = request.get("params", {})

    handler = _dispatch_table.get(method)
    if handler is None:
        return {"id": msg_id, "error": f"unknown method: {method}"}

    try:
        result = await handler(**params)
        return {"id": msg_id, "result": result}
    except Exception as exc:  # noqa: BLE001
        logger.exception("Handler %s raised", method)
        return {"id": msg_id, "error": str(exc)}


# ---------------------------------------------------------------------------
# Built-in handlers
# ---------------------------------------------------------------------------


@register_method("ping")
async def _handle_ping() -> str:
    return "pong"


# ---------------------------------------------------------------------------
# Server
# ---------------------------------------------------------------------------


async def handle_connection(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
) -> None:
    """Process requests on a single connection until EOF."""
    peer = writer.get_extra_info("peername")
    logger.info("New connection from %s", peer)
    try:
        while True:
            length_bytes = await reader.readexactly(4)
            length = struct.unpack(">I", length_bytes)[0]
            data = await reader.readexactly(length)
            request: dict[str, Any] = msgpack.unpackb(data, raw=False)

            result = await dispatch(request)

            response_bytes = msgpack.packb(result, use_bin_type=True)
            writer.write(struct.pack(">I", len(response_bytes)) + response_bytes)
            await writer.drain()
    except asyncio.IncompleteReadError:
        logger.info("Connection closed by peer %s", peer)
    except Exception:
        logger.exception("Unexpected error on connection %s", peer)
    finally:
        writer.close()
        await writer.wait_closed()


async def run_sidecar(socket_path: str | Path | None = None) -> None:
    """Start the sidecar server on *socket_path*.

    If *socket_path* is ``None`` the path is read from
    ``$APEX_SIDECAR_SOCKET`` or defaults to ``/tmp/apex_sidecar.sock``.
    """
    if socket_path is None:
        socket_path = os.environ.get(
            "APEX_SIDECAR_SOCKET", "/tmp/apex_sidecar.sock"
        )
    socket_path = Path(socket_path)

    # Remove stale socket file
    socket_path.unlink(missing_ok=True)

    server = await asyncio.start_unix_server(handle_connection, path=str(socket_path))
    logger.info("Sidecar listening on %s", socket_path)

    async with server:
        await server.serve_forever()


# ---------------------------------------------------------------------------
# CLI entry-point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    asyncio.run(run_sidecar())
