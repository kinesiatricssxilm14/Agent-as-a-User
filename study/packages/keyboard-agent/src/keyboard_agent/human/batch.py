from __future__ import annotations

import os
from pathlib import Path


def _find_kit_benchmark() -> Path | None:
    here = Path(__file__).resolve()
    locale = os.environ.get("BENCH_LOCALE", "cn")
    preferred = f"benchmark_{locale}"
    for parent in here.parents:
        candidates = [
            parent / preferred,
            parent / "curated-study-kit" / preferred,
            parent / "benchmark_cn",
            parent / "benchmark_en",
            parent / "benchmark",
            parent / "curated-study-kit" / "benchmark_cn",
            parent / "curated-study-kit" / "benchmark_en",
            parent / "curated-study-kit" / "benchmark",
        ]
        for candidate in candidates:
            if candidate.is_dir() and any(candidate.glob("P*/bench.spec.json")):
                return candidate.resolve()
    return None


def default_cn_bench_root() -> Path:
    """Benchmark corpus root: env > bundled kit benchmark directory."""
    if env := os.environ.get("BENCH_CN_ROOT"):
        return Path(env).expanduser().resolve()
    if env := os.environ.get("BENCH_SUITE_ROOT"):
        return Path(env).expanduser().resolve()
    if found := _find_kit_benchmark():
        return found
    raise FileNotFoundError(
        "benchmark root not found; set BENCH_CN_ROOT or BENCH_SUITE_ROOT"
    )


def _default_cn_bench_root_or_cwd() -> Path:
    try:
        return default_cn_bench_root()
    except FileNotFoundError:
        return Path.cwd()


DEFAULT_CN_BENCH_ROOT = _default_cn_bench_root_or_cwd()


def discover_bench_projects(root: str | Path) -> list[Path]:
    """Return Pxx-* benchmark projects that contain bench.spec.json."""
    root_path = Path(root).expanduser().resolve()
    if not root_path.is_dir():
        raise ValueError(f"Benchmark root does not exist: {root_path}")
    return sorted(
        (
            path
            for path in root_path.iterdir()
            if path.is_dir()
            and path.name.startswith("P")
            and (path / "bench.spec.json").is_file()
        ),
        key=lambda path: path.name,
    )


def _find_bench_script() -> Path | None:
    here = Path(__file__).resolve()
    rel_paths = (
        ("evaluation", "scripts", "bench.sh"),
        ("packages", "evaluation", "scripts", "bench.sh"),
        ("curated-study-kit", "packages", "evaluation", "scripts", "bench.sh"),
    )
    for parent in here.parents:
        for rel in rel_paths:
            candidate = parent.joinpath(*rel)
            if candidate.is_file():
                return candidate.resolve()
    return None


def default_bench_script() -> Path:
    """Resolve bench.sh from env or relative kit/monorepo layout."""
    if env := os.environ.get("BENCH"):
        path = Path(env).expanduser().resolve()
        if path.is_file():
            return path
    if found := _find_bench_script():
        return found
    raise FileNotFoundError(
        "bench.sh not found; pass --bench or set BENCH to evaluation/scripts/bench.sh"
    )
