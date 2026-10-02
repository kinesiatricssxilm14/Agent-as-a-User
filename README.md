# Agent-as-a-User — Anonymous Research Artifact

This repository contains the English-only code used to construct and run the
task-based TUI usability experiments. It is prepared for anonymous peer review:
author identities, machine-specific paths, credentials, raw participant data,
and experiment outputs are not included.

## Contents

- `study/benchmark/`: 15 benchmark projects and 84 English tasks, including
  Dockerfiles, deterministic seeds, and task oracles.
- `study/packages/`: the `agent-tui`, `evaluation`, and `keyboard-agent`
  packages used by both human and agent runs.
- `study/`: English launch scripts and experiment configurations.
- `generated-tuis/`: source-only snapshots of the Claude Code, Codex, and
  DeepSeek Harness implementations evaluated in RQ3.
- `generation/`: scripts used to generate and containerize generated TUIs.
- `analysis/`: scripts used to aggregate the runs and analyze observation
  representations.

Raw trajectories and terminal images are intentionally hosted separately
because they are too large for GitHub. The reviewer website provides online
trajectory browsing.

## Requirements

- macOS or Linux
- Python 3.10 or newer
- Docker with the daemon running
- tmux
- at least 30 GB free disk space to build all 15 reference images

Agent experiments additionally require a compatible API key supplied through
the environment. No credential file is committed.

## One-command quick start

From the repository root:

```bash
./artifact.sh quickstart
```

This command runs the anonymity/integrity checks, installs the three local
Python packages in editable mode, writes a machine-local `.env`, and prints the
84-task human-study schedule without starting containers.

To choose a Python interpreter:

```bash
PYTHON=python3.12 ./artifact.sh quickstart
```

## Common commands

```bash
# Verify package structure, English-only policy, and anonymity
./artifact.sh verify

# Install local packages and create .env
./artifact.sh setup

# Build all 15 benchmark Docker images (30–60 minutes)
./artifact.sh build

# Preview the human task order
./artifact.sh human --operator tester1 --seed 42 --dry-run

# Run the human protocol
./artifact.sh human --operator tester1 --seed 42

# Resume unfinished human tasks
./artifact.sh human --operator tester1 --resume

# Preview the agent schedule
DEEPSEEK_API_KEY=... ./artifact.sh agent --dry-run

# Run the agent protocol
DEEPSEEK_API_KEY=... ./artifact.sh agent -c study/config.deepseek-v4-pro.yaml --max-turns 100
```

Outputs are written to the ignored `runs/` directory. Use anonymous operator
labels such as `tester1`; do not enter names, email addresses, employee IDs, or
other participant identifiers.

## Experimental workflow

1. `study/benchmark/P*/Dockerfile` builds a pinned upstream reference TUI.
2. `study/benchmark/P*/seed/` creates deterministic task state.
3. `study/packages/evaluation/scripts/bench.sh` controls the container and
   invokes the per-task oracle.
4. `study/packages/keyboard-agent` records keystrokes, turns, terminal SVGs,
   reasoning metadata, and oracle results.
5. Human runs use `keyboard-agent human run-suite`; agent runs use
   `keyboard-agent run`.
6. `analysis/` aggregates pass rates, turns, reasoning cost, operation cost,
   and observation-condition comparisons.

The benchmark contains synthetic fixture records (including fictional names
and example-form email addresses) required by database-oriented tasks. They are
generated task data, not participant or author information.

## Benchmark inventory

| ID | Project | Tasks |
|---|---|---:|
| P01 | aptui | 4 |
| P02 | flow | 4 |
| P03 | elio | 4 |
| P04 | helius | 4 |
| P05 | rgx | 5 |
| P06 | rura | 6 |
| P07 | ec | 5 |
| P08 | snip | 3 |
| P09 | dusk | 5 |
| P10 | tredis | 7 |
| P11 | datui | 11 |
| P12 | tuxedo | 5 |
| P13 | easydocker | 8 |
| P14 | glazepkg | 7 |
| P15 | sqv | 6 |
| **Total** | **15 projects** | **84** |

## Data not included

The repository excludes raw human runs, model outputs, terminal screenshots,
Docker images, build logs, caches, API credentials, local `.env` files, and Git
history from the source workspace. These exclusions keep the GitHub artifact
small and anonymous; they do not remove code needed to run a new experiment.

## Reproducibility notes

- Upstream reference implementations are fetched at the commits pinned in each
  benchmark Dockerfile.
- Docker image names and oracle behavior are defined by each `bench.spec.json`.
- Agent configuration files intentionally contain no API keys.
- Analysis scripts expect released result tables or a local run directory; data
  paths must be supplied by the reviewer.

## Publishing from an anonymous GitHub account

The ZIP intentionally contains no Git history. After extracting it, verify and
create a fresh repository:

```bash
cd github-artifact
./artifact.sh verify
git init
git add .
git -c user.name="Anonymous Artifact"     -c user.email="anonymous@example.invalid"     commit -m "Add reviewer artifact"
git branch -M main
git remote add origin https://github.com/ANONYMOUS_ACCOUNT/ANONYMOUS_REPOSITORY.git
git push -u origin main
```

Before pushing, confirm that the GitHub account profile, repository owner,
commit author, and remote URL do not reveal author or institution information.
Do not copy the original workspace `.git` directory into this repository.

## Troubleshooting

- If Docker checks fail, start Docker Desktop or the Docker daemon.
- If `tmux` is missing, install it with Homebrew or the system package manager.
- If pip uses a broken corporate proxy, unset proxy environment variables or
  set `PIP_INDEX_URL` before running setup.
- Run `./artifact.sh verify` after copying or modifying the repository.
