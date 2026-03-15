"""Strategy subprocess isolation.

Each strategy is launched in its own subprocess so that a crash in one
strategy does not affect others or the sidecar.  The :class:`StrategySandbox`
manager monitors child processes and auto-restarts them up to
:data:`MAX_RESTARTS` times before marking a strategy as ``FAILED``.
"""

from __future__ import annotations

import asyncio
import enum
import logging
import subprocess
import sys
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)

MAX_RESTARTS: int = 3


class ProcessStatus(str, enum.Enum):
    """Lifecycle states of a strategy subprocess."""

    STARTING = "starting"
    RUNNING = "running"
    STOPPED = "stopped"
    FAILED = "failed"


@dataclass
class StrategyProcess:
    """Bookkeeping for a single strategy subprocess."""

    process: subprocess.Popen[bytes]
    strategy_id: str
    script_path: str
    params: dict[str, Any] = field(default_factory=dict)
    status: ProcessStatus = ProcessStatus.STARTING
    restart_count: int = 0


def launch_strategy(
    script_path: str,
    strategy_id: str,
    params: dict[str, Any] | None = None,
    socket_path: str | None = None,
) -> StrategyProcess:
    """Spawn a strategy in a child process and return its handle."""
    cmd = [
        sys.executable,
        "-m",
        "runtime.strategy_runner",
        "--id",
        strategy_id,
        "--script",
        script_path,
    ]
    if socket_path is not None:
        cmd.extend(["--socket", socket_path])

    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    sp = StrategyProcess(
        process=proc,
        strategy_id=strategy_id,
        script_path=script_path,
        params=params or {},
        status=ProcessStatus.RUNNING,
    )
    logger.info(
        "Launched strategy %s (pid=%d) from %s",
        strategy_id,
        proc.pid,
        script_path,
    )
    return sp


class StrategySandbox:
    """Manages a collection of :class:`StrategyProcess` instances.

    Call :meth:`start` to launch strategies and :meth:`monitor` in an async
    loop to watch for crashes and auto-restart.
    """

    def __init__(self, socket_path: str | None = None) -> None:
        self._processes: dict[str, StrategyProcess] = {}
        self._socket_path = socket_path
        self._alert_callback: Any | None = None

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def set_alert_callback(self, callback: Any) -> None:
        """Register a callable invoked on strategy failure/restart."""
        self._alert_callback = callback

    def start(
        self,
        script_path: str,
        strategy_id: str,
        params: dict[str, Any] | None = None,
    ) -> StrategyProcess:
        """Launch a new strategy subprocess."""
        sp = launch_strategy(
            script_path, strategy_id, params, self._socket_path,
        )
        self._processes[strategy_id] = sp
        return sp

    def stop(self, strategy_id: str) -> None:
        """Gracefully terminate a strategy subprocess."""
        sp = self._processes.get(strategy_id)
        if sp is None or sp.process.poll() is not None:
            return
        sp.process.terminate()
        try:
            sp.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            sp.process.kill()
        sp.status = ProcessStatus.STOPPED
        logger.info("Stopped strategy %s", strategy_id)

    def stop_all(self) -> None:
        """Terminate every tracked strategy."""
        for sid in list(self._processes):
            self.stop(sid)

    async def monitor(self, poll_interval: float = 0.5) -> None:
        """Poll subprocesses and restart crashed ones.

        Runs indefinitely; call from an ``asyncio.create_task``.
        """
        while True:
            for sp in list(self._processes.values()):
                if sp.status in (ProcessStatus.STOPPED, ProcessStatus.FAILED):
                    continue

                retcode = sp.process.poll()
                if retcode is None:
                    continue  # still running

                # Process exited
                stderr_output = ""
                if sp.process.stderr is not None:
                    stderr_output = sp.process.stderr.read().decode(
                        errors="replace",
                    )

                if retcode == 0:
                    sp.status = ProcessStatus.STOPPED
                    logger.info("Strategy %s exited cleanly", sp.strategy_id)
                    continue

                # Non-zero exit → crash
                logger.error(
                    "Strategy %s crashed (rc=%d):\n%s",
                    sp.strategy_id,
                    retcode,
                    stderr_output,
                )

                self._emit_alert(sp, retcode, stderr_output)

                if sp.restart_count < MAX_RESTARTS:
                    sp.restart_count += 1
                    logger.info(
                        "Restarting strategy %s (attempt %d/%d)",
                        sp.strategy_id,
                        sp.restart_count,
                        MAX_RESTARTS,
                    )
                    new_sp = launch_strategy(
                        sp.script_path,
                        sp.strategy_id,
                        sp.params,
                        self._socket_path,
                    )
                    new_sp.restart_count = sp.restart_count
                    self._processes[sp.strategy_id] = new_sp
                else:
                    sp.status = ProcessStatus.FAILED
                    logger.error(
                        "Strategy %s exceeded max restarts — marked FAILED",
                        sp.strategy_id,
                    )

            await asyncio.sleep(poll_interval)

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    def _emit_alert(
        self,
        sp: StrategyProcess,
        retcode: int,
        stderr_output: str,
    ) -> None:
        if self._alert_callback is not None:
            try:
                self._alert_callback(
                    {
                        "strategy_id": sp.strategy_id,
                        "exit_code": retcode,
                        "stderr": stderr_output[:4096],
                        "restart_count": sp.restart_count,
                    },
                )
            except Exception:  # noqa: BLE001
                logger.exception("Alert callback failed")
