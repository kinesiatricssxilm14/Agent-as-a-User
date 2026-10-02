"""Pre-warmed bench slot pool — isolated project clones for parallel startup."""

from __future__ import annotations

import fcntl
import json
import os
import random
import shutil
import threading
import time
from contextlib import contextmanager
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Callable, Iterator

from ..bench_client import BenchClient, BenchError


class SlotState(str, Enum):
    IDLE = "idle"
    WARMING = "warming"
    READY = "ready"
    BUSY = "busy"
    FAILED = "failed"


# Docker build context must be real files (symlinks break `docker build` on macOS).
_SLOT_COPY_FILES = (
    "Dockerfile",
    "entrypoint.sh",
)
_SLOT_COPY_DIRS = (
    "seed",
)
# Shared read-only; safe to symlink (not sent to docker build context).
_SLOT_SYMLINKS = (
    "bench.spec.json",
    "oracle",
)


def sync_slot_from_source(
    source_project: Path,
    slot_dir: Path,
    *,
    task_id: str | None = None,
) -> None:
    """Refresh slot fingerprints from the source bench before warming."""
    source_project = source_project.resolve()
    slot_dir = slot_dir.resolve()
    dst_oracle = slot_dir / ".oracle"
    dst_oracle.mkdir(parents=True, exist_ok=True)
    (dst_oracle / "fingerprint_scope").write_text(
        f"{source_project.name}\n",
        encoding="utf-8",
    )
    src_oracle = source_project / ".oracle"
    if src_oracle.is_dir():
        for fp in src_oracle.glob("*_fingerprints.json"):
            shutil.copy2(fp, dst_oracle / fp.name)
    if task_id:
        staged = dst_oracle / "staged_seed" / task_id.upper()
        if staged.exists():
            shutil.rmtree(staged)


def clone_project_for_slot(source_project: Path, slot_dir: Path) -> Path:
    """Create an isolated bench project dir (shared content, private ``.oracle/``)."""
    source_project = source_project.resolve()
    slot_dir = slot_dir.resolve()
    if slot_dir.exists():
        shutil.rmtree(slot_dir)
    slot_dir.mkdir(parents=True)

    for name in _SLOT_COPY_FILES:
        src = source_project / name
        if src.is_file():
            shutil.copy2(src, slot_dir / name)

    for name in _SLOT_COPY_DIRS:
        src = source_project / name
        if src.is_dir():
            shutil.copytree(src, slot_dir / name, symlinks=False)

    for name in _SLOT_SYMLINKS:
        src = source_project / name
        if not src.exists():
            continue
        dst = slot_dir / name
        dst.symlink_to(src)

    sync_slot_from_source(source_project, slot_dir)
    return slot_dir


@dataclass
class PoolSlot:
    slot_id: int
    project_dir: Path
    web_port: int
    state: SlotState = SlotState.IDLE
    message: str = ""
    task_id: str = ""
    error: str = ""
    session_id: str = ""
    _lock: threading.Lock = field(default_factory=threading.Lock, repr=False)

    def snapshot(self) -> dict:
        with self._lock:
            return {
                "slot_id": self.slot_id,
                "project_dir": str(self.project_dir),
                "web_port": self.web_port,
                "state": self.state.value,
                "message": self.message,
                "task_id": self.task_id,
                "error": self.error,
                "session_id": self.session_id,
            }


@dataclass
class PoolCheckout:
    """A checked-out warm slot for one human session."""

    slot: PoolSlot
    project_dir: Path
    web_port: int
    task_id: str
    bench: BenchClient
    _release: Callable[[], None]

    def release(self) -> None:
        self._release()


@dataclass
class WarmPoolConfig:
    pool_size: int = 3
    base_port: int = 8765
    acquire_timeout_sec: float = 600.0
    rebuild: bool = False
    no_build: bool = False
    single_concurrent_warm: bool = False
    warm_lock_path: Path | None = None


@dataclass(frozen=True)
class WarmPoolPolicy:
    """Per-project warm pool limits (from bench.spec.json ``warm_pool``)."""

    max_slots: int | None = None
    single_concurrent_warm: bool = False


def warm_pool_policy(project_dir: Path) -> WarmPoolPolicy:
    """Read warm-pool limits for a bench project."""
    project_dir = project_dir.resolve()
    spec_path = project_dir / "bench.spec.json"
    if not spec_path.is_file():
        return WarmPoolPolicy()
    try:
        spec = json.loads(spec_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return WarmPoolPolicy()
    raw = spec.get("warm_pool")
    if not isinstance(raw, dict):
        return WarmPoolPolicy()
    max_slots = raw.get("max_slots")
    return WarmPoolPolicy(
        max_slots=int(max_slots) if max_slots is not None else None,
        single_concurrent_warm=bool(raw.get("single_concurrent_warm", False)),
    )


def resolve_warm_pool_config(
    *,
    source_project: Path,
    output_dir: Path,
    pool_size: int,
    base_port: int,
    rebuild: bool,
    no_build: bool,
) -> tuple[WarmPoolConfig, WarmPoolPolicy]:
    policy = warm_pool_policy(source_project)
    effective_size = pool_size
    if policy.max_slots is not None:
        effective_size = min(pool_size, policy.max_slots)
    lock_path: Path | None = None
    if policy.single_concurrent_warm:
        lock_path = output_dir / ".warm_pool" / f"{source_project.name}.warm.lock"
    return (
        WarmPoolConfig(
            pool_size=max(1, effective_size),
            base_port=base_port,
            rebuild=rebuild,
            no_build=no_build,
            single_concurrent_warm=policy.single_concurrent_warm,
            warm_lock_path=lock_path,
        ),
        policy,
    )


def load_project_task_ids(
    project_dir: Path,
    *,
    task_ids: list[str] | None = None,
    all_tasks: bool = False,
) -> list[str]:
    """Return task IDs from ``bench.spec.json`` (all or an explicit subset)."""
    spec_path = project_dir / "bench.spec.json"
    if not spec_path.is_file():
        raise BenchError(f"Missing bench.spec.json in {project_dir}")
    raw = json.loads(spec_path.read_text(encoding="utf-8"))
    spec_tasks = raw.get("tasks") or []
    available = [
        str(task["id"]).upper()
        for task in spec_tasks
        if isinstance(task, dict) and task.get("id")
    ]
    if not available:
        raise BenchError(f"No tasks defined in {spec_path}")
    if all_tasks:
        return available
    if not task_ids:
        raise BenchError("Specify task_ids or all_tasks=True")
    wanted = [task_id.upper() for task_id in task_ids]
    missing = [task_id for task_id in wanted if task_id not in available]
    if missing:
        raise BenchError(
            f"Unknown task(s) {', '.join(missing)}; available: {', '.join(available)}"
        )
    return wanted


def port_stride_for_pool(pool_size: int) -> int:
    """Gap between per-task web port bases so slot ports never collide."""
    return max(pool_size, 1) + 5


def _pool_lock_path(pool_root: Path) -> Path:
    return pool_root / ".pool.lock"


@contextmanager
def _pool_file_lock(pool_root: Path, *, shared: bool = False) -> Iterator[None]:
    pool_root.mkdir(parents=True, exist_ok=True)
    lock_path = _pool_lock_path(pool_root)
    fd = os.open(str(lock_path), os.O_CREAT | os.O_RDWR)
    try:
        fcntl.flock(fd, fcntl.LOCK_SH if shared else fcntl.LOCK_EX)
        yield
    finally:
        fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)


def _atomic_write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    os.replace(tmp, path)


def _read_pool_state(path: Path) -> dict | None:
    if not path.is_file():
        return None
    text = path.read_text(encoding="utf-8").strip()
    if not text:
        return None
    return json.loads(text)


def _slot_session_healthy(bench: BenchClient, task_id: str) -> bool:
    """True when the slot has a live bench session for ``task_id``."""
    try:
        return bench.has_active_session() and bench.is_container_alive(task_id)
    except BenchError:
        return False


class WarmPool:
    """
    Maintain N bench instances for one (project, task).

    Each slot uses a cloned project directory so ``.oracle/`` state never races.
    Works with default terminal attach; ``--web`` is optional.
    """

    def __init__(
        self,
        *,
        source_project: Path,
        bench_script: Path,
        task_id: str,
        pool_root: Path,
        config: WarmPoolConfig | None = None,
        log: Callable[[str], None] | None = None,
        restore: bool = False,
    ) -> None:
        self.source_project = source_project.resolve()
        self.bench_script = bench_script.resolve()
        self.task_id = task_id.upper()
        self.pool_root = pool_root.resolve()
        self.config = config or WarmPoolConfig()
        self._log = log or (lambda _msg: None)
        self._slots: list[PoolSlot] = []
        self._threads: list[threading.Thread] = []
        self._stop = threading.Event()
        self._persist_lock = threading.Lock()
        if restore:
            self._restore_slots()
        else:
            self._init_slots()

    @property
    def slots(self) -> list[PoolSlot]:
        return list(self._slots)

    def _restore_slots(self) -> None:
        state = WarmPool.load_state(self.pool_root)
        if not state or not state.get("slots"):
            raise BenchError(f"Cannot restore warm pool — missing state at {self.pool_root}")
        self.config.pool_size = len(state["slots"])
        for slot_data in state["slots"]:
            self._slots.append(
                PoolSlot(
                    slot_id=int(slot_data["slot_id"]),
                    project_dir=Path(slot_data["project_dir"]),
                    web_port=int(slot_data["web_port"]),
                    state=SlotState(slot_data.get("state", SlotState.IDLE.value)),
                    message=str(slot_data.get("message") or ""),
                    task_id=str(slot_data.get("task_id") or self.task_id),
                    error=str(slot_data.get("error") or ""),
                    session_id=str(slot_data.get("session_id") or ""),
                )
            )

    def _init_slots(self) -> None:
        self.pool_root.mkdir(parents=True, exist_ok=True)
        for i in range(self.config.pool_size):
            slot_dir = self.pool_root / f"slot-{i}"
            clone_project_for_slot(self.source_project, slot_dir)
            self._slots.append(
                PoolSlot(
                    slot_id=i,
                    project_dir=slot_dir,
                    web_port=self.config.base_port + i,
                )
            )

    def start_warming(self) -> None:
        """Kick off background warm threads for every slot."""
        self._stop.clear()
        self._threads.clear()
        for slot in self._slots:
            thread = threading.Thread(
                target=self._warm_loop,
                args=(slot,),
                name=f"warm-slot-{slot.slot_id}",
                daemon=True,
            )
            thread.start()
            self._threads.append(thread)
        self._persist_state()

    def shutdown(self) -> None:
        self._stop.set()
        for slot in self._slots:
            self._stop_slot_bench(slot)
        for thread in self._threads:
            thread.join(timeout=2.0)

    def acquire(self) -> PoolCheckout:
        """Block until a READY slot is available, then check it out."""
        deadline = time.monotonic() + self.config.acquire_timeout_sec
        last_status = ""
        while time.monotonic() < deadline:
            if self._stop.is_set():
                raise BenchError("Warm pool shut down while waiting for a ready slot")

            ready = [s for s in self._slots if s.state == SlotState.READY]
            if ready:
                slot = random.choice(ready)
                with slot._lock:
                    if slot.state != SlotState.READY:
                        continue
                    slot.state = SlotState.BUSY
                    slot.message = "English-only text"
                self._persist_state()
                bench = BenchClient(slot.project_dir, self.bench_script)
                session_id = bench.session_id(self.task_id) if bench.has_active_session() else ""
                self._log(f"✅ English-only text：English-only text slot-{slot.slot_id}（{session_id or 'ready'}）")

                def release() -> None:
                    self._release_slot(slot)

                return PoolCheckout(
                    slot=slot,
                    project_dir=slot.project_dir,
                    web_port=slot.web_port,
                    task_id=self.task_id,
                    bench=bench,
                    _release=release,
                )

            status = self._format_pool_status()
            if status != last_status:
                self._log(status)
                last_status = status
            time.sleep(0.5)

        raise BenchError(
            f"Timed out after {self.config.acquire_timeout_sec}s waiting for a warm slot. "
            f"Last status: {self._format_pool_status()}"
        )

    def status_lines(self) -> list[str]:
        return [self._slot_line(s) for s in self._slots]

    def _format_pool_status(self) -> str:
        counts = {state: 0 for state in SlotState}
        for slot in self._slots:
            counts[slot.state] += 1
        ready = counts[SlotState.READY]
        total = len(self._slots)
        warming = counts[SlotState.WARMING]
        failed = counts[SlotState.FAILED]
        msg = f"⏳ English-only text：{ready}/{total} English-only text"
        if warming:
            msg += f"，{warming} English-only text"
        if failed:
            msg += f"，{failed} English-only text"
        return msg

    def _slot_line(self, slot: PoolSlot) -> str:
        with slot._lock:
            extra = f" — {slot.message}" if slot.message else ""
            if slot.error:
                extra = f" — {slot.error}"
            sid = f" {slot.session_id}" if slot.session_id else ""
            return f"  slot-{slot.slot_id}: {slot.state.value}{extra}{sid}"

    def _release_slot(self, slot: PoolSlot) -> None:
        with slot._lock:
            slot.state = SlotState.IDLE
            slot.message = ""
            slot.session_id = ""
        if not self._stop.is_set():
            self._stop_slot_bench(slot)
            thread = threading.Thread(
                target=self._warm_loop,
                args=(slot,),
                name=f"rewarm-slot-{slot.slot_id}",
                daemon=True,
            )
            thread.start()
            self._threads.append(thread)
        self._persist_state()

    @contextmanager
    def _maybe_warm_lock(self) -> Iterator[None]:
        lock_path = self.config.warm_lock_path
        if not lock_path:
            yield
            return
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(str(lock_path), os.O_CREAT | os.O_RDWR)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX)
            yield
        finally:
            fcntl.flock(fd, fcntl.LOCK_UN)
            os.close(fd)

    def _warm_loop(self, slot: PoolSlot) -> None:
        while not self._stop.is_set():
            with slot._lock:
                if slot.state in (SlotState.BUSY, SlotState.READY):
                    return
                slot.state = SlotState.WARMING
                slot.message = "⏳ English-only text…"
                slot.error = ""
                slot.task_id = self.task_id

            try:
                sync_slot_from_source(
                    self.source_project,
                    slot.project_dir,
                    task_id=self.task_id,
                )
                bench = BenchClient(slot.project_dir, self.bench_script)
                if bench.has_active_session():
                    bench.stop()
                with self._maybe_warm_lock():
                    bench.start(
                        self.task_id,
                        rebuild=self.config.rebuild,
                        no_build=self.config.no_build,
                    )
                session_id = bench.session_id(self.task_id)
                if not _slot_session_healthy(bench, self.task_id):
                    raise BenchError(
                        "bench start finished but session/container is not healthy "
                        f"({self.task_id})"
                    )
                with slot._lock:
                    slot.state = SlotState.READY
                    slot.message = "✅ English-only text"
                    slot.session_id = session_id
                    slot.error = ""
                self._log(f"✅ English-only text slot-{slot.slot_id} English-only text（{session_id}）")
                self._persist_state()
                return
            except Exception as exc:
                with slot._lock:
                    slot.state = SlotState.FAILED
                    slot.error = str(exc)
                    slot.message = ""
                self._log(f"✗ English-only text slot-{slot.slot_id} English-only text: {exc}")
                self._persist_state()
                if self._stop.is_set():
                    return
                time.sleep(5.0)

    def _stop_slot_bench(self, slot: PoolSlot) -> None:
        try:
            bench = BenchClient(slot.project_dir, self.bench_script)
            if bench.has_active_session():
                bench.stop()
        except Exception:
            pass

    def _persist_state(self) -> None:
        payload = {
            "source_project": str(self.source_project),
            "bench_script": str(self.bench_script),
            "task_id": self.task_id,
            "pool_root": str(self.pool_root),
            "slots": [s.snapshot() for s in self._slots],
            "updated_at": time.time(),
        }
        path = self.pool_root / "pool.json"
        with self._persist_lock:
            with _pool_file_lock(self.pool_root):
                _atomic_write_json(path, payload)

    def sync_from_disk(self) -> None:
        """Apply external slot claims/releases written by ``human run --use-pool``."""
        path = self.pool_root / "pool.json"
        with _pool_file_lock(self.pool_root, shared=True):
            state = _read_pool_state(path)
        if not state:
            return

        slot_by_id = {slot.slot_id: slot for slot in self._slots}
        for slot_data in state.get("slots", []):
            slot_id = int(slot_data["slot_id"])
            slot = slot_by_id.get(slot_id)
            if slot is None:
                continue
            disk_state = SlotState(slot_data.get("state", SlotState.IDLE.value))
            with slot._lock:
                if disk_state == SlotState.BUSY and slot.state == SlotState.READY:
                    slot.state = SlotState.BUSY
                    slot.message = str(slot_data.get("message") or "English-only text")
                    slot.session_id = str(slot_data.get("session_id") or slot.session_id)
                elif disk_state == SlotState.IDLE and slot.state in (
                    SlotState.BUSY,
                    SlotState.READY,
                ):
                    slot.state = SlotState.IDLE
                    slot.message = ""
                    slot.session_id = ""
                    self._stop_slot_bench(slot)
                    if not self._stop.is_set():
                        thread = threading.Thread(
                            target=self._warm_loop,
                            args=(slot,),
                            name=f"rewarm-slot-{slot.slot_id}",
                            daemon=True,
                        )
                        thread.start()
                        self._threads.append(thread)

    @classmethod
    def default_pool_root(cls, output_dir: Path, source_project: Path, task_id: str) -> Path:
        return output_dir / ".warm_pool" / f"{source_project.name}-{task_id.upper()}"

    @classmethod
    def load_state(cls, pool_root: Path) -> dict | None:
        path = pool_root / "pool.json"
        with _pool_file_lock(pool_root, shared=True):
            return _read_pool_state(path)


@dataclass
class TaskWarmPoolGroup:
    """One process managing warm pools for several tasks in the same project."""

    pools: list[WarmPool]

    @property
    def task_ids(self) -> list[str]:
        return [pool.task_id for pool in self.pools]

    def start_warming(self) -> None:
        for pool in self.pools:
            pool.start_warming()

    def shutdown(self) -> None:
        for pool in self.pools:
            pool.shutdown()

    def sync_from_disk(self) -> None:
        for pool in self.pools:
            pool.sync_from_disk()

    def status_lines(self) -> list[str]:
        lines: list[str] = []
        for pool in self.pools:
            lines.append(f"=== {pool.task_id} ===")
            lines.append(pool._format_pool_status())
            lines.extend(pool.status_lines())
        return lines


def build_project_image(
    *,
    source_project: Path,
    bench_script: Path,
    rebuild: bool = True,
    log: Callable[[str], None] | None = None,
) -> None:
    """Build docker image once from the source project (not warm-pool slot clones)."""
    _log = log or (lambda _msg: None)
    _log(f"🔨 English-only text{'English-only text' if rebuild else ''}English-only text：{source_project.name}…")
    bench = BenchClient(source_project, bench_script)
    bench.build(rebuild=rebuild)
    _log(f"✅ English-only text：{source_project.name}")


def create_warm_pool_group(
    *,
    source_project: Path,
    bench_script: Path,
    output_dir: Path,
    task_ids: list[str],
    pool_size: int,
    base_port: int,
    rebuild: bool,
    no_build: bool = False,
    log: Callable[[str], None] | None = None,
) -> TaskWarmPoolGroup:
    stride = port_stride_for_pool(pool_size)
    pools: list[WarmPool] = []
    for index, task_id in enumerate(task_ids):
        slot_config, _ = resolve_warm_pool_config(
            source_project=source_project,
            output_dir=output_dir,
            pool_size=pool_size,
            base_port=base_port + index * stride,
            rebuild=rebuild,
            no_build=no_build,
        )
        pools.append(
            WarmPool(
                source_project=source_project,
                bench_script=bench_script,
                task_id=task_id,
                pool_root=WarmPool.default_pool_root(output_dir, source_project, task_id),
                config=slot_config,
                log=log,
            )
        )
    return TaskWarmPoolGroup(pools=pools)


def claim_slot_from_served_pool(
    *,
    pool_root: Path,
    task_id: str,
    bench_script: Path,
    acquire_timeout_sec: float = 600.0,
    log: Callable[[str], None] | None = None,
) -> PoolCheckout:
    """Atomically claim a READY slot from a pool started by ``human pool serve``."""
    _log = log or (lambda _msg: None)
    path = pool_root / "pool.json"
    deadline = time.monotonic() + acquire_timeout_sec
    last_status = ""

    while time.monotonic() < deadline:
        with _pool_file_lock(pool_root):
            state = _read_pool_state(path)
            if state is None:
                raise BenchError(
                    f"No warm pool at {pool_root}. Start one with: "
                    f"keyboard-agent human pool serve -p <project> -t {task_id}"
                )
            ready = [s for s in state.get("slots", []) if s.get("state") == SlotState.READY.value]
            if ready:
                random.shuffle(ready)
                resolved_bench = bench_script.resolve()
                if state.get("bench_script") and Path(state["bench_script"]).resolve() != resolved_bench:
                    state["bench_script"] = str(resolved_bench)
                stale = False
                for slot_data in ready:
                    project_dir = Path(slot_data["project_dir"])
                    bench = BenchClient(project_dir, resolved_bench)
                    if not _slot_session_healthy(bench, task_id):
                        slot_data["state"] = SlotState.IDLE.value
                        slot_data["message"] = ""
                        slot_data["session_id"] = ""
                        slot_data["error"] = "stale ready (no active session)"
                        stale = True
                        continue

                    slot_data["state"] = SlotState.BUSY.value
                    slot_data["message"] = "English-only text"
                    _atomic_write_json(path, state)

                    session_id = bench.session_id(task_id)
                    slot_id = int(slot_data["slot_id"])
                    _log(f"✅ English-only text：English-only text slot-{slot_id}（{session_id}）")

                    def release() -> None:
                        release_slot_to_served_pool(
                            pool_root=pool_root,
                            slot_id=slot_id,
                            project_dir=project_dir,
                            bench_script=resolved_bench,
                        )

                    slot = PoolSlot(
                        slot_id=slot_id,
                        project_dir=project_dir,
                        web_port=int(slot_data["web_port"]),
                        state=SlotState.BUSY,
                        message="English-only text",
                        task_id=task_id,
                        session_id=session_id,
                    )
                    return PoolCheckout(
                        slot=slot,
                        project_dir=project_dir,
                        web_port=int(slot_data["web_port"]),
                        task_id=task_id,
                        bench=bench,
                        _release=release,
                    )

                if stale:
                    _atomic_write_json(path, state)
                    _log("⚠ English-only text ready slot（English-only text bench session），English-only text…")

            ready_count = sum(1 for s in state.get("slots", []) if s.get("state") == SlotState.READY.value)
            warming_count = sum(1 for s in state.get("slots", []) if s.get("state") == SlotState.WARMING.value)
            failed_count = sum(1 for s in state.get("slots", []) if s.get("state") == SlotState.FAILED.value)
            total = len(state.get("slots", []))
            status = f"⏳ English-only text：{ready_count}/{total} English-only text"
            if warming_count:
                status += f"，{warming_count} English-only text"
            if failed_count:
                status += f"，{failed_count} English-only text"

        if status != last_status:
            _log(status)
            last_status = status
        time.sleep(0.5)

    raise BenchError(
        f"Timed out after {acquire_timeout_sec}s waiting for a warm slot. Last status: {last_status or 'unknown'}"
    )


def release_slot_to_served_pool(
    *,
    pool_root: Path,
    slot_id: int,
    project_dir: Path,
    bench_script: Path,
) -> None:
    """Return a slot to the served pool so ``pool serve`` can re-warm it."""
    try:
        bench = BenchClient(project_dir, bench_script)
        if bench.has_active_session():
            bench.stop()
    except Exception:
        pass

    path = pool_root / "pool.json"
    with _pool_file_lock(pool_root):
        state = _read_pool_state(path)
        if state is None:
            return
        for slot_data in state.get("slots", []):
            if int(slot_data.get("slot_id", -1)) == slot_id:
                slot_data["state"] = SlotState.IDLE.value
                slot_data["message"] = ""
                slot_data["session_id"] = ""
                slot_data["error"] = ""
                break
        _atomic_write_json(path, state)
