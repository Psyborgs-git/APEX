"""Tests for strategy subprocess isolation (sandbox)."""

from __future__ import annotations

import subprocess
import sys
from unittest.mock import MagicMock, patch

import pytest

from runtime.sandbox import (
    MAX_RESTARTS,
    ProcessStatus,
    StrategySandbox,
    StrategyProcess,
    launch_strategy,
)


# ------------------------------------------------------------------
# launch_strategy
# ------------------------------------------------------------------


class TestLaunchStrategy:
    @patch("runtime.sandbox.subprocess.Popen")
    def test_returns_strategy_process(self, mock_popen: MagicMock) -> None:
        mock_proc = MagicMock()
        mock_proc.pid = 12345
        mock_popen.return_value = mock_proc

        sp = launch_strategy("my_strat.py", "strat-1")

        assert isinstance(sp, StrategyProcess)
        assert sp.strategy_id == "strat-1"
        assert sp.script_path == "my_strat.py"
        assert sp.status == ProcessStatus.RUNNING
        assert sp.restart_count == 0

    @patch("runtime.sandbox.subprocess.Popen")
    def test_passes_socket_arg(self, mock_popen: MagicMock) -> None:
        mock_popen.return_value = MagicMock(pid=1)
        launch_strategy("s.py", "s-1", socket_path="/tmp/test.sock")

        cmd = mock_popen.call_args[0][0]
        assert "--socket" in cmd
        assert "/tmp/test.sock" in cmd

    @patch("runtime.sandbox.subprocess.Popen")
    def test_no_socket_arg_by_default(self, mock_popen: MagicMock) -> None:
        mock_popen.return_value = MagicMock(pid=1)
        launch_strategy("s.py", "s-1")

        cmd = mock_popen.call_args[0][0]
        assert "--socket" not in cmd


# ------------------------------------------------------------------
# StrategySandbox
# ------------------------------------------------------------------


class TestStrategySandbox:
    @patch("runtime.sandbox.launch_strategy")
    def test_start_tracks_process(self, mock_launch: MagicMock) -> None:
        mock_sp = StrategyProcess(
            process=MagicMock(), strategy_id="s-1", script_path="a.py",
        )
        mock_launch.return_value = mock_sp

        sandbox = StrategySandbox()
        result = sandbox.start("a.py", "s-1")

        assert result is mock_sp
        assert "s-1" in sandbox._processes

    @patch("runtime.sandbox.launch_strategy")
    def test_stop_terminates_process(self, mock_launch: MagicMock) -> None:
        proc = MagicMock()
        proc.poll.return_value = None  # still running
        mock_sp = StrategyProcess(process=proc, strategy_id="s-1", script_path="a.py")
        mock_launch.return_value = mock_sp

        sandbox = StrategySandbox()
        sandbox.start("a.py", "s-1")
        sandbox.stop("s-1")

        proc.terminate.assert_called_once()
        assert mock_sp.status == ProcessStatus.STOPPED

    @pytest.mark.asyncio
    @patch("runtime.sandbox.launch_strategy")
    async def test_monitor_restarts_crashed_strategy(
        self,
        mock_launch: MagicMock,
    ) -> None:
        # First process crashes
        crashed_proc = MagicMock()
        crashed_proc.poll.return_value = 1  # non-zero exit
        crashed_proc.stderr.read.return_value = b"segfault"

        # Replacement process stays alive
        alive_proc = MagicMock()
        alive_proc.poll.return_value = None
        alive_proc.pid = 99

        first_sp = StrategyProcess(
            process=crashed_proc,
            strategy_id="s-1",
            script_path="a.py",
            status=ProcessStatus.RUNNING,
        )

        replacement_sp = StrategyProcess(
            process=alive_proc,
            strategy_id="s-1",
            script_path="a.py",
            status=ProcessStatus.RUNNING,
        )
        mock_launch.return_value = replacement_sp

        sandbox = StrategySandbox()
        sandbox._processes["s-1"] = first_sp

        # Run one iteration of the monitor
        import asyncio

        task = asyncio.create_task(sandbox.monitor(poll_interval=0.01))
        await asyncio.sleep(0.05)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task

        # Should have restarted
        assert sandbox._processes["s-1"].process is alive_proc

    @pytest.mark.asyncio
    async def test_monitor_marks_failed_after_max_restarts(self) -> None:
        proc = MagicMock()
        proc.poll.return_value = 1
        proc.stderr.read.return_value = b"error"

        sp = StrategyProcess(
            process=proc,
            strategy_id="s-1",
            script_path="a.py",
            status=ProcessStatus.RUNNING,
            restart_count=MAX_RESTARTS,
        )

        sandbox = StrategySandbox()
        sandbox._processes["s-1"] = sp

        import asyncio

        task = asyncio.create_task(sandbox.monitor(poll_interval=0.01))
        await asyncio.sleep(0.05)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task

        assert sp.status == ProcessStatus.FAILED

    def test_alert_callback_invoked_on_crash(self) -> None:
        """Verify alert callback is called when we emit an alert."""
        sandbox = StrategySandbox()
        alerts: list[dict] = []
        sandbox.set_alert_callback(lambda a: alerts.append(a))

        proc = MagicMock()
        sp = StrategyProcess(
            process=proc, strategy_id="s-1", script_path="a.py",
        )
        sandbox._emit_alert(sp, retcode=1, stderr_output="oops")

        assert len(alerts) == 1
        assert alerts[0]["strategy_id"] == "s-1"
        assert alerts[0]["exit_code"] == 1
