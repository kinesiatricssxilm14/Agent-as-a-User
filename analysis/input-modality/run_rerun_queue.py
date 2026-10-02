#!/usr/bin/env python3
"""Rerun infrastructure-affected attempts and replace canonical task artifacts."""

from __future__ import annotations

import argparse
import csv
import fcntl
import json
import multiprocessing as mp
import os
import shutil
import subprocess
import time
from datetime import datetime
from pathlib import Path


TUI = Path("/path/to/anonymous-artifact")
KIT = TUI / "curated-study-kit"
FINAL = TUI / "agent-result-final"
HERE = FINAL / "analysis/input-modality"
QUEUE = HERE / "rerun_required_all_conditions.csv"
RUNTIME = HERE / "rerun-runtime"
STATE = RUNTIME / "state.jsonl"
LOCK = RUNTIME / "state.lock"
BENCHMARK = KIT / "benchmark_en"
BENCH_SH = KIT / "packages/evaluation/scripts/bench.sh"
TEXT_CONFIG = KIT / "config.deepseek.yaml"
VISION_CONFIG = KIT / "config.deepseek-v4-flash-vision.yaml"
ENV_FILE = TUI / ".env"

IMAGE_PREFIX = {
    "original-tui": None,
    "deepseek-harness-v4-pro": "agent-deepseek-harness-v4-pro",
    "claude-code-opus-5": "agent-claude-code-claude-opus-5",
    "codex-gpt-5.6-sol": "agent-codex",
}
READY = {
    "P01-aptui": {"timeout_sec": 90, "stable_sec": 0, "ready_pattern": "TOOLA"},
    "P02-flow": {"timeout_sec": 180, "ready_pattern": "toolb"},
    "P03-elio": {"timeout_sec": 180, "ready_pattern": "toolc"},
    "P04-helius": {"timeout_sec": 180, "ready_pattern": "toold"},
    "P05-rgx": {"timeout_sec": 90, "stable_sec": 0, "ready_pattern": "Pattern"},
    "P06-rura": {"timeout_sec": 180, "ready_pattern": "Pipeline"},
    "P07-ec": {"timeout_sec": 90, "stable_sec": 0, "ready_pattern": "OURS"},
    "P08-snip": {"timeout_sec": 180, "ready_pattern": "toolh"},
    "P09-dusk": {"timeout_sec": 180, "ready_pattern": "Treemap"},
    "P10-tredis": {"timeout_sec": 180, "ready_pattern": "PubSub"},
    "P11-datui": {"timeout_sec": 180, "ready_pattern": "toolk"},
    "P12-tuxedo": {"timeout_sec": 180, "ready_pattern": "TOOLL"},
    "P13-easydocker": {"timeout_sec": 180, "ready_pattern": "toolm"},
    "P14-glazepkg": {"timeout_sec": 90, "ready_pattern": "tooln"},
    "P15-sqv": {"timeout_sec": 180, "ready_pattern": "toolo"},
}
BAD_ERROR_FRAGMENTS = (
    "connection error",
    "request timed out",
    "maximum context length",
    "error code: 429",
    "error code: 500",
    "error code: 502",
    "error code: 503",
    "error code: 504",
    "error code: 402",
    "insufficient balance",
)


def load_dotenv() -> None:
    if not ENV_FILE.is_file():
        return
    for line in ENV_FILE.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip())


def append_state(record: dict) -> None:
    record = {"ts": time.time(), **record}
    with LOCK.open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        with STATE.open("a") as state:
            state.write(json.dumps(record, ensure_ascii=False) + "\n")


def latest_state() -> dict[str, dict]:
    latest = {}
    if STATE.is_file():
        for line in STATE.read_text().splitlines():
            if line.strip():
                record = json.loads(line)
                latest[str(record["rerun_id"])] = record
    return latest


def claim(worker: str) -> dict | None:
    with QUEUE.open(newline="") as queue:
        rows = list(csv.DictReader(queue))
    # Long PNG and context-overflow jobs first.
    rows.sort(
        key=lambda row: (
            1 if "context_overflow" in row["reason"] else 0,
            0 if row["observation"] == "png" else 1,
            int(row["rerun_id"]),
        )
    )
    with LOCK.open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        latest = latest_state()
        for row in rows:
            status = latest.get(row["rerun_id"], {}).get("status")
            if status in {"running", "attempt", "replaced", "failed"}:
                continue
            append = {
                "rerun_id": row["rerun_id"],
                "status": "running",
                "worker": worker,
                "key": "|".join(row[field] for field in (
                    "app", "judge_model", "observation", "repeat", "project", "task_id"
                )),
            }
            with STATE.open("a") as state:
                state.write(json.dumps({"ts": time.time(), **append}) + "\n")
            return row
    return None


def prepare_clone(worker: str, row: dict) -> Path:
    clone = RUNTIME / "benches" / worker
    clone.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "rsync", "-a", "--delete",
            "--exclude", ".oracle/current_session.json",
            "--exclude", ".oracle/session_*.json",
            "--exclude", ".oracle/run_counter",
            "--exclude", ".oracle/staged_seed",
            f"{BENCHMARK}/", f"{clone}/",
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    prefix = IMAGE_PREFIX[row["app"]]
    if prefix:
        for spec_path in sorted(clone.glob("P*-*/bench.spec.json")):
            spec = json.loads(spec_path.read_text())
            spec["docker_image"] = f"{prefix}-{spec_path.parent.name.lower()}"
            if spec_path.parent.name in READY:
                spec["startup"] = READY[spec_path.parent.name]
            spec_path.write_text(json.dumps(spec, indent=2) + "\n")
    return clone


def run_once(worker: str, row: dict, attempt: int, deadline: float) -> tuple[Path | None, str]:
    if time.time() >= deadline:
        return None, "deadline"
    clone = prepare_clone(worker, row)
    attempt_root = RUNTIME / "attempts" / f"{int(row['rerun_id']):03d}" / f"try{attempt}"
    suite = attempt_root / "suite"
    if attempt_root.exists():
        shutil.rmtree(attempt_root)
    suite.mkdir(parents=True)
    config = VISION_CONFIG if row["observation"] == "png" else TEXT_CONFIG
    max_turns = (
        "200"
        if row["app"] == "original-tui"
        and row["judge_model"] == "deepseek-v4-flash"
        and row["observation"] in {"plain", "semantic"}
        else "100"
    )
    sock = f"tui-modality-rerun-{worker}"
    env = os.environ.copy()
    env["PATH"] = "/opt/homebrew/Caskroom/miniforge/base/bin:" + env.get("PATH", "")
    env["BENCH"] = str(BENCH_SH)
    env["BENCH_LOCALE"] = "en"
    env["BENCH_SUITE_ROOT"] = str(clone)
    env["TUI_BENCH_TMUX_SOCKET"] = sock
    env["TUI_BENCH_NAME_PREFIX"] = f"mr{worker}"
    env["AGENT_TUI_HOME"] = str(RUNTIME / "agent-home" / worker)
    Path(env["AGENT_TUI_HOME"]).mkdir(parents=True, exist_ok=True)
    Path(env["AGENT_TUI_HOME"], "active_session").unlink(missing_ok=True)
    subprocess.run(["tmux", "-L", sock, "kill-server"], capture_output=True)
    cmd = [
        "keyboard-agent", "run",
        "-c", str(config),
        "-p", str(clone / row["project"]),
        "--bench", str(BENCH_SH),
        "-o", str(attempt_root / "output"),
        "--run-dir", str(suite),
        "--task", row["task_id"],
        "--observation-mode", row["observation"],
        "--model", row["judge_model"],
        "--max-turns", max_turns,
    ]
    log = attempt_root / "worker.log"
    with log.open("w") as output:
        proc = subprocess.run(
            cmd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=output,
            stderr=subprocess.STDOUT,
        )
    subprocess.run(["tmux", "-L", sock, "kill-server"], capture_output=True)
    task_dir = suite / row["project"] / row["task_id"]
    result = task_dir / "result.json"
    transcript = task_dir / "transcript.jsonl"
    if not result.is_file() or not transcript.is_file():
        return None, f"missing_artifact_rc_{proc.returncode}"
    text = transcript.read_text(errors="replace").lower()
    found = sorted(fragment for fragment in BAD_ERROR_FRAGMENTS if fragment in text)
    if found:
        return None, "infra_error:" + ",".join(found)
    try:
        json.loads(result.read_text())
    except (json.JSONDecodeError, OSError) as exc:
        return None, f"invalid_result:{exc}"
    return task_dir, "valid"


def atomic_replace(source: Path, row: dict) -> None:
    destination = FINAL / "agent" / row["artifact_relpath"]
    if not destination.is_dir():
        raise FileNotFoundError(destination)
    incoming = destination.with_name(destination.name + ".rerun-incoming")
    previous = destination.with_name(destination.name + ".rerun-previous")
    shutil.rmtree(incoming, ignore_errors=True)
    shutil.rmtree(previous, ignore_errors=True)
    shutil.copytree(source, incoming)
    destination.rename(previous)
    try:
        incoming.rename(destination)
    except BaseException:
        previous.rename(destination)
        raise
    shutil.rmtree(previous)


def worker_main(worker_number: int, deadline: float, queue: str, runtime: str) -> None:
    global QUEUE, RUNTIME, STATE, LOCK
    QUEUE = Path(queue)
    RUNTIME = Path(runtime)
    STATE = RUNTIME / "state.jsonl"
    LOCK = RUNTIME / "state.lock"
    worker = f"w{worker_number:02d}"
    load_dotenv()
    while time.time() < deadline:
        row = claim(worker)
        if row is None:
            break
        notes = []
        replaced = False
        max_attempts = 1 if "context_overflow" in row["reason"] else 3
        for attempt in range(1, max_attempts + 1):
            append_state({
                "rerun_id": row["rerun_id"],
                "status": "attempt",
                "worker": worker,
                "attempt": attempt,
            })
            try:
                source, note = run_once(worker, row, attempt, deadline)
                notes.append(note)
                if source is not None:
                    atomic_replace(source, row)
                    result = json.loads((source / "result.json").read_text())
                    append_state({
                        "rerun_id": row["rerun_id"],
                        "status": "replaced",
                        "worker": worker,
                        "attempt": attempt,
                        "passed": bool(result.get("passed")),
                        "stop_reason": result.get("stop_reason"),
                        "notes": notes,
                    })
                    print(
                        f"RERUN_REPLACED id={row['rerun_id']} worker={worker} "
                        f"{row['app']} {row['observation']} {row['repeat']} "
                        f"{row['project']} {row['task_id']} passed={result.get('passed')}",
                        flush=True,
                    )
                    replaced = True
                    break
            except Exception as exc:
                notes.append(f"exception:{type(exc).__name__}:{exc}")
            if time.time() < deadline:
                time.sleep(10)
        if not replaced:
            append_state({
                "rerun_id": row["rerun_id"],
                "status": "failed",
                "worker": worker,
                "notes": notes,
            })
            print(
                f"RERUN_FAILED id={row['rerun_id']} worker={worker} notes={notes}",
                flush=True,
            )


def status_summary() -> dict:
    latest = latest_state()
    counts = {}
    for record in latest.values():
        status = record.get("status", "unknown")
        counts[status] = counts.get(status, 0) + 1
    return {"queue": sum(1 for _ in csv.DictReader(QUEUE.open())), "status": counts}


def parse_deadline(value: str) -> float:
    return datetime.fromisoformat(value).timestamp()


def main() -> None:
    global QUEUE, RUNTIME, STATE, LOCK
    parser = argparse.ArgumentParser()
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--deadline", required=True, help="Local ISO datetime")
    parser.add_argument("--queue", type=Path, default=QUEUE)
    parser.add_argument("--runtime", type=Path, default=RUNTIME)
    parser.add_argument("--fresh", action="store_true")
    args = parser.parse_args()
    QUEUE = args.queue.resolve()
    RUNTIME = args.runtime.resolve()
    STATE = RUNTIME / "state.jsonl"
    LOCK = RUNTIME / "state.lock"
    RUNTIME.mkdir(parents=True, exist_ok=True)
    LOCK.touch(exist_ok=True)
    if args.fresh:
        STATE.unlink(missing_ok=True)
    deadline = parse_deadline(args.deadline)
    print(
        f"RERUN_START workers={args.workers} deadline={args.deadline} summary={status_summary()}",
        flush=True,
    )
    processes = [
        mp.Process(
            target=worker_main,
            args=(number, deadline, str(QUEUE), str(RUNTIME)),
            daemon=False,
        )
        for number in range(1, args.workers + 1)
    ]
    for process in processes:
        process.start()
    for process in processes:
        process.join()
    print(f"RERUN_COMPLETE {json.dumps(status_summary(), sort_keys=True)}", flush=True)


if __name__ == "__main__":
    main()
