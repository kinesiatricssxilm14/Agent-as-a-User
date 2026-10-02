#!/usr/bin/env python3
"""Create a benchmark-style Docker context from an agent-generated artifact."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import tomllib
from pathlib import Path


DEFAULT_BENCH_ROOT = Path("/path/to/anonymous-artifact")
DEFAULT_OUTPUT_ROOT = Path("/path/to/anonymous-artifact")


def copy_path(src: Path, dst: Path) -> None:
    if src.is_dir():
        shutil.copytree(src, dst)
    elif src.exists():
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)


def infer_command_name(artifact_name: str) -> str:
    return artifact_name.lower()


def infer_rust_binary_name(artifact_dir: Path, fallback: str) -> str:
    cargo_toml = artifact_dir / "Cargo.toml"
    if not cargo_toml.exists():
        return fallback
    try:
        cargo = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError:
        return fallback
    bins = cargo.get("bin")
    if isinstance(bins, list):
        for bin_config in bins:
            if isinstance(bin_config, dict) and bin_config.get("name"):
                return str(bin_config["name"])
    package = cargo.get("package")
    if isinstance(package, dict) and package.get("name"):
        return str(package["name"])
    return fallback


def detect_project_type(artifact_dir: Path) -> str:
    if (artifact_dir / "Cargo.toml").exists():
        return "rust"
    if (artifact_dir / "go.mod").exists() or any(artifact_dir.glob("*.go")):
        return "go"
    if (
        (artifact_dir / "pyproject.toml").exists()
        or (artifact_dir / "setup.py").exists()
        or (artifact_dir / "requirements.txt").exists()
        or any(artifact_dir.glob("*.py"))
    ):
        return "python"
    raise SystemExit(
        "Cannot infer project type for "
        f"{artifact_dir}. Expected Cargo.toml, go.mod/Go files, or Python files."
    )


def write_go_dockerfile(
    path: Path,
    *,
    artifact_dir: Path,
    artifact_name: str,
    binary_name: str,
    cmd_name: str,
) -> None:
    mod_download = "RUN go mod download\n" if (artifact_dir / "go.mod").exists() else ""
    path.write_text(
        f"""# Generated benchmark Dockerfile for agent artifact {artifact_name}
FROM golang:1.26-alpine AS builder

RUN apk add --no-cache git ca-certificates
WORKDIR /src/{artifact_name}
COPY {artifact_name}/ ./
{mod_download.rstrip()}
RUN go build -o /out/{binary_name} .

FROM debian:12-slim
LABEL bench.generated=\"true\" bench.lang=\"go\"

RUN apt-get update -qq \\
    && apt-get install -y --no-install-recommends ca-certificates apt-utils sudo wget curl bash sqlite3 git \\
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/{binary_name} /usr/local/bin/{cmd_name}

RUN mkdir -p /bench/data /bench/config /root/.config/{cmd_name}
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT [\"/entrypoint.sh\"]
CMD [\"sudo\", \"{cmd_name}\"]
""",
        encoding="utf-8",
    )


def python_wrapper_script(cmd_name: str) -> str:
    # This file is copied over /usr/local/bin/{cmd_name}, so it must never
    # `exec {cmd_name}` — that re-enters the wrapper forever and the TUI
    # never paints (screen stays a single block character).
    return f"""#!/bin/sh
set -eu
cd /opt/app
if [ -f "{cmd_name}.py" ]; then
  exec python "{cmd_name}.py" "$@"
fi
if [ -f "main.py" ]; then
  exec python "main.py" "$@"
fi
if [ -f "app.py" ]; then
  exec python "app.py" "$@"
fi
if [ -d "src/{cmd_name}" ]; then
  PYTHONPATH="/opt/app/src${{PYTHONPATH:+:$PYTHONPATH}}"
  export PYTHONPATH
fi
if [ -d "{cmd_name}" ] || python -c "import {cmd_name}" >/dev/null 2>&1; then
  if python -c "import {cmd_name}.__main__" >/dev/null 2>&1; then
    exec python -m "{cmd_name}" "$@"
  fi
  if python -c "from {cmd_name}.cli import main" >/dev/null 2>&1; then
    exec python -c "import sys; from {cmd_name}.cli import main; raise SystemExit(main())" "$@"
  fi
fi
first_py="$(find . -maxdepth 2 -type f -name '*.py' | sort | head -n 1)"
if [ -n "$first_py" ]; then
  exec python "$first_py" "$@"
fi
echo "No Python entrypoint found for {cmd_name}" >&2
exit 127
"""


def python_wrapper_name(cmd_name: str) -> str:
    return f".agent-{cmd_name}-wrapper.sh"


def json_cmd(cmd_name: str, original_text: str) -> str:
    if re.search(r'(?m)^CMD\s+\[\s*"sudo"', original_text):
        return f'CMD ["sudo", "{cmd_name}"]'
    return f'CMD ["{cmd_name}"]'


CLONE_BLOCK_RE = re.compile(
    r"(?m)^(?:ARG SOURCE_SHA=.+\n)?"
    r"RUN git clone[^\n\\]*(?:\\[ \t]*\n[^\n]*)*\n"
)


def replace_git_clone(text: str, replacement: str) -> str:
    """Swap the benchmark's git clone (often backslash-continued) for COPY."""
    updated, n = CLONE_BLOCK_RE.subn(replacement, text, count=1)
    if n:
        return updated
    return re.sub(r"(?m)^RUN git clone .+$", replacement.rstrip("\n"), text, count=1)


def patch_original_dockerfile(
    *,
    original_text: str,
    project_type: str,
    artifact_name: str,
    artifact_dir: Path,
    binary_name: str,
    cmd_name: str,
) -> str:
    text = original_text
    if project_type == "go":
        mod_download = "RUN go mod download\n" if (artifact_dir / "go.mod").exists() else ""
        build_block = (
            f"WORKDIR /src/{artifact_name}\n"
            f"COPY {artifact_name}/ ./\n"
            f"{mod_download}"
            f"RUN go build -o /out/{cmd_name} ."
        )
        text, n_clone = re.subn(
            r"(?m)^(?:ARG SOURCE_SHA=.+\n)?"
            r"RUN git clone [^\n]+(?:\\\n[^\n]+)*\n"
            r"WORKDIR /src\n"
            r"RUN go build [^\n]+$",
            build_block,
            text,
            count=1,
        )
        if n_clone == 0:
            text = re.sub(r"(?m)^RUN go install .+$", build_block, text)
            text = re.sub(r"(?m)^RUN git clone .+$", build_block, text)
            text = re.sub(
                r"(?m)^RUN go build -o \S+ .+$",
                f"RUN go build -o /out/{cmd_name} .",
                text,
            )
        text = re.sub(
            r"(?m)^COPY --from=builder \S+ /usr/local/bin/\S+\s*$",
            f"COPY --from=builder /out/{cmd_name} /usr/local/bin/{cmd_name}",
            text,
            count=1,
        )
    elif project_type == "rust":
        build_binary_name = infer_rust_binary_name(artifact_dir, binary_name)
        text = replace_git_clone(text, f"COPY {artifact_name}/ /src\n")
        # Original-repo lockfile workarounds (e.g. datui/ethnum) do not apply
        # to agent-generated Cargo.lock files.
        text = re.sub(r"(?m)^# .+\nRUN cargo update .+\n", "", text)
        text = re.sub(r"(?m)^RUN cargo update .+\n", "", text)
        text = re.sub(
            r"(?m)^COPY --from=builder /src/target/release/\S+ /usr/local/bin/\S+\s*$",
            f"COPY --from=builder /src/target/release/{build_binary_name} /usr/local/bin/{cmd_name}",
            text,
            count=1,
        )
        text = re.sub(
            r"(?m)^RUN ln -s /usr/local/bin/\S+ /usr/local/bin/\S+\s*$",
            "",
            text,
        )
    elif project_type == "python":
        wrapper_name = python_wrapper_name(cmd_name)
        install_block = (
            f"WORKDIR /opt/app\n"
            f"COPY {artifact_name}/ /opt/app/\n"
            "RUN if [ -f requirements.txt ]; then pip install --no-cache-dir -r requirements.txt; fi \\\n"
            "    && if [ -f pyproject.toml ] || [ -f setup.py ]; then pip install --no-cache-dir .; fi\n"
            f"COPY {wrapper_name} /usr/local/bin/{cmd_name}\n"
            f"RUN chmod +x /usr/local/bin/{cmd_name}\n"
        )
        text = replace_git_clone(text, "")
        text, n_pip = re.subn(
            r"(?m)^RUN pip install .+$",
            install_block.rstrip("\n"),
            text,
            count=1,
        )
        if n_pip == 0:
            text = install_block + text

    text = re.sub(
        r"(?m)^RUN chmod \+x /usr/local/bin/\S+\s*$",
        f"RUN chmod +x /usr/local/bin/{cmd_name}",
        text,
    )
    text = re.sub(
        r"(?m)^RUN ln -s /usr/local/bin/\S+ /usr/local/bin/\S+\s*$",
        "",
        text,
    )
    text = re.sub(r"(?m)^CMD \[.*\]\s*$", json_cmd(cmd_name, original_text), text)
    return text


def write_python_dockerfile(path: Path, artifact_name: str, cmd_name: str) -> None:
    wrapper = python_wrapper_script(cmd_name).replace("\\", "\\\\").replace("'", "'\"'\"'")
    path.write_text(
        f"""# Generated benchmark Dockerfile for agent artifact {artifact_name}
FROM python:3.13-slim
LABEL bench.generated=\"true\" bench.lang=\"python\"

RUN apt-get update -qq \\
    && apt-get install -y --no-install-recommends ca-certificates apt-utils sudo wget curl bash sqlite3 git openssh-client openssh-server vim \\
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/app
COPY {artifact_name}/ /opt/app/
RUN if [ -f requirements.txt ]; then pip install --no-cache-dir -r requirements.txt; fi \\
    && if [ -f pyproject.toml ] || [ -f setup.py ]; then pip install --no-cache-dir .; fi
RUN printf '%s' '{wrapper}' > /usr/local/bin/{cmd_name} \\
    && chmod +x /usr/local/bin/{cmd_name}

RUN mkdir -p /bench/data /bench/config /root/.config/{cmd_name}
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT [\"/entrypoint.sh\"]
CMD [\"sudo\", \"{cmd_name}\"]
""",
        encoding="utf-8",
    )


def write_rust_dockerfile(path: Path, artifact_name: str, binary_name: str, cmd_name: str) -> None:
    path.write_text(
        f"""# Generated benchmark Dockerfile for agent artifact {artifact_name}
FROM rust:1.91-slim AS builder

RUN apt-get update -qq \\
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \\
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src/{artifact_name}
COPY {artifact_name}/ ./
RUN cargo build --release
RUN install -Dm755 target/release/{binary_name} /out/{binary_name}

FROM debian:12-slim
LABEL bench.generated=\"true\" bench.lang=\"rust\"

RUN apt-get update -qq \\
    && apt-get install -y --no-install-recommends ca-certificates apt-utils sudo wget curl bash sqlite3 git openssh-client openssh-server vim libssl3 \\
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/{binary_name} /usr/local/bin/{cmd_name}

RUN mkdir -p /bench/data /bench/config /root/.config/{cmd_name}
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT [\"/entrypoint.sh\"]
CMD [\"sudo\", \"{cmd_name}\"]
""",
        encoding="utf-8",
    )


def write_dockerfile(
    path: Path,
    *,
    artifact_dir: Path,
    artifact_name: str,
    binary_name: str,
    cmd_name: str,
) -> str:
    project_type = detect_project_type(artifact_dir)
    if project_type == "rust":
        write_rust_dockerfile(path, artifact_name, binary_name, cmd_name)
    elif project_type == "python":
        write_python_dockerfile(path, artifact_name, cmd_name)
    else:
        write_go_dockerfile(
            path,
            artifact_dir=artifact_dir,
            artifact_name=artifact_name,
            binary_name=binary_name,
            cmd_name=cmd_name,
        )
    return project_type


def normalize_go_mod(path: Path) -> None:
    go_mod = path / "go.mod"
    if not go_mod.exists():
        return

    text = go_mod.read_text(encoding="utf-8")
    text = re.sub(r"(?m)^go (\d+\.\d+)\.\d+\s*$", r"go \1", text)
    go_mod.write_text(text, encoding="utf-8")


def create_context(
    *,
    benchmark_dir: Path,
    output_dir: Path,
    artifact_name: str,
    context_dir: Path,
    image_name: str,
    binary_name: str,
    cmd_name: str,
    overwrite: bool,
) -> None:
    artifact_dir = output_dir / artifact_name
    if not artifact_dir.is_dir():
        raise SystemExit(f"Artifact directory not found: {artifact_dir}")
    if not benchmark_dir.is_dir():
        raise SystemExit(f"Benchmark directory not found: {benchmark_dir}")
    if context_dir.exists():
        if not overwrite:
            raise SystemExit(f"Context already exists: {context_dir}. Use --overwrite.")
        shutil.rmtree(context_dir)

    context_dir.mkdir(parents=True)

    for prompt_path in sorted(benchmark_dir.glob("P*-prompt.md")):
        copy_path(prompt_path, context_dir / prompt_path.name)

    for name in [
        "README.md",
        "NETWORK_FIX.md",
        "bench.spec.json",
        "tasks.json",
        "task_note.md",
        "note.md",
        "entrypoint.sh",
        "seed",
        "oracle",
    ]:
        copy_path(benchmark_dir / name, context_dir / name)

    shutil.copytree(artifact_dir, context_dir / artifact_name)
    copied_artifact_dir = context_dir / artifact_name
    normalize_go_mod(copied_artifact_dir)
    project_type = detect_project_type(copied_artifact_dir)
    if project_type == "python":
        wrapper_path = context_dir / python_wrapper_name(cmd_name)
        wrapper_path.write_text(python_wrapper_script(cmd_name), encoding="utf-8")
    source_dockerfile = benchmark_dir / "Dockerfile"
    if source_dockerfile.exists():
        patched_dockerfile = patch_original_dockerfile(
            original_text=source_dockerfile.read_text(encoding="utf-8"),
            project_type=project_type,
            artifact_name=artifact_name,
            artifact_dir=copied_artifact_dir,
            binary_name=binary_name,
            cmd_name=cmd_name,
        )
        (context_dir / "Dockerfile").write_text(patched_dockerfile, encoding="utf-8")
    else:
        project_type = write_dockerfile(
            context_dir / "Dockerfile",
            artifact_dir=copied_artifact_dir,
            artifact_name=artifact_name,
            binary_name=binary_name,
            cmd_name=cmd_name,
        )

    meta = {
        "benchmark_dir": str(benchmark_dir),
        "agent_output_dir": str(output_dir),
        "artifact": artifact_name,
        "project_type": project_type,
        "image_name": image_name,
        "build_command": f"docker build -t {image_name} {context_dir}",
        "run_command": f"docker run -it --rm {image_name}",
    }
    (context_dir / "agent_context_meta.json").write_text(
        json.dumps(meta, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", required=True, help="Project dir name, e.g. P01-aptui")
    parser.add_argument("--combo", required=True, help="Output combo, e.g. openhands__deepseek-v4-pro")
    parser.add_argument("--artifact", default="toolA")
    parser.add_argument("--benchmark-root", type=Path, default=DEFAULT_BENCH_ROOT)
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT_ROOT)
    parser.add_argument("--context-root", type=Path)
    parser.add_argument("--image-name")
    parser.add_argument("--binary-name")
    parser.add_argument("--cmd-name")
    parser.add_argument("--overwrite", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    benchmark_dir = args.benchmark_root / args.project
    output_dir = args.output_root / args.combo / args.project
    context_root = args.context_root or args.output_root / "_docker_contexts"
    context_dir = context_root / args.combo / args.project
    image_name = args.image_name or f"agent-{args.combo}-{args.project}".lower().replace("_", "-")
    cmd_name = args.cmd_name or infer_command_name(args.artifact)
    binary_name = args.binary_name or cmd_name

    create_context(
        benchmark_dir=benchmark_dir,
        output_dir=output_dir,
        artifact_name=args.artifact,
        context_dir=context_dir,
        image_name=image_name,
        binary_name=binary_name,
        cmd_name=cmd_name,
        overwrite=args.overwrite,
    )

    print(f"Context: {context_dir}")
    print(f"Build:   docker build -t {image_name} {context_dir}")
    print(f"Run:     docker run -it --rm {image_name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
