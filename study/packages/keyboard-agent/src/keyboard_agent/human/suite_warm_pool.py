from __future__ import annotations

import threading
from pathlib import Path
from typing import Callable

from .warm_pool import (
    WarmPool,
    WarmPoolConfig,
    claim_slot_from_served_pool,
    PoolCheckout,
)


class SuiteLookaheadWarmer:
    """Background warm pools for upcoming (project, task_id) pairs across a suite."""

    def __init__(
        self,
        *,
        suite_root: Path,
        bench_script: Path,
        output_dir: Path,
        log: Callable[[str], None] | None = None,
    ) -> None:
        self.suite_root = suite_root.resolve()
        self.bench_script = bench_script.resolve()
        self.output_dir = output_dir.resolve()
        self._log = log or (lambda _msg: None)
        self._pools: dict[str, WarmPool] = {}
        self._lock = threading.Lock()

    @staticmethod
    def _key(project: str, task_id: str) -> str:
        return f"{project}::{task_id.upper()}"

    def _pool_root(self, project: str, task_id: str) -> Path:
        safe = self._key(project, task_id).replace("::", "__")
        return self.output_dir / ".suite_warm_pool" / safe

    def ensure_warming(
        self,
        upcoming: list[tuple[str, str]],
        *,
        slots_per_task: int = 1,
    ) -> None:
        for project, task_id in upcoming:
            key = self._key(project, task_id)
            with self._lock:
                if key in self._pools:
                    continue
                project_dir = self.suite_root / project
                pool = WarmPool(
                    source_project=project_dir,
                    bench_script=self.bench_script,
                    task_id=task_id,
                    pool_root=self._pool_root(project, task_id),
                    config=WarmPoolConfig(pool_size=max(1, slots_per_task), no_build=True),
                    log=self._log,
                )
                self._pools[key] = pool
            pool.start_warming()
            self._log(f"⏳ English-only text: {project} {task_id.upper()}")

    def claim(
        self,
        project: str,
        task_id: str,
        *,
        timeout_sec: float = 8.0,
    ) -> PoolCheckout | None:
        key = self._key(project, task_id)
        with self._lock:
            pool = self._pools.get(key)
        if pool is None:
            return None
        try:
            return claim_slot_from_served_pool(
                pool_root=pool.pool_root,
                task_id=task_id,
                bench_script=self.bench_script,
                acquire_timeout_sec=timeout_sec,
                log=self._log,
            )
        except Exception:
            return None

    def release_used(self, project: str, task_id: str) -> None:
        key = self._key(project, task_id)
        with self._lock:
            pool = self._pools.pop(key, None)
        if pool is not None:
            pool.shutdown()

    def shutdown(self) -> None:
        with self._lock:
            pools = list(self._pools.values())
            self._pools.clear()
        for pool in pools:
            pool.shutdown()
