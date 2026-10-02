# TUI-Bench Oracle English-only text

**English-only text**

| English-only text | English-only text |
|------|------|
| **`bench.spec.json`** | **English-only text**：English-only text + `observation` + `docker_image`（CLI English-only text） |
| **`task_note.md`** | **English-only text**：English-only text + oracle English-only text |
| **`tasks.json`** | English-only text/English-only text，**CLI English-only text** |
| **`oracle/Txx.sh`** | **English-only text**English-only text |
| **`.oracle/`** | English-only text：English-only text、English-only text、session manifest |

---

## English-only text

```bash
export BENCH_PROJECT=../Finshed/P01-aptui
BENCH=./scripts/bench.sh

# English-only text
"$BENCH" start-all --human
"$BENCH" task && "$BENCH" use
"$BENCH" verify --human          # English-only text：[PASS] P01 T01 (shell)
"$BENCH" next --human            # → English-only text
"$BENCH" stop
```

English-only text：**[`ORACLE_GUIDE.md`](ORACLE_GUIDE.md)**

---

## `bench.spec.json`

```json
{
  "project_id": "P01",
  "slug": "aptui",
  "docker_image": "bench-p01",
  "tasks": [
    {
      "id": "T01",
      "description": "English-only text，English-only text {{file_content}} English-only text",
      "observation": "shell",
      "difficulty": "medium",
      "type": "create"
    }
  ]
}
```

Schema：[`schema/bench.spec.schema.json`](schema/bench.spec.schema.json)

---

## CLI English-only text

| English-only text | English-only text |
|------|------|
| `build` | English-only text `bench.spec.json` English-only text `docker_image` English-only text `docker build`（English-only text） |
| `session start T01` | English-only text Docker+tmux；English-only text `.oracle/current_session.json` |
| `session verify` | English-only text oracle English-only text PASS/FAIL（English-only text current_session） |
| `session stop` | English-only text tmux；English-only text current_session.json |
| `session current` | English-only text current_session.json |
| `session status` | English-only text manifest（English-only text current_session） |
| `list` | English-only text bench.spec.json English-only text |
| `run` / `inject-task` / `gen-fingerprints` | English-only text |
| `check …` | English-only text check English-only text（`screen` / `screen-absent` / `filesystem` / `shell` English-only text，English-only text ORACLE_SCRIPT.md） |

---

## English-only text

```
P01-aptui/
├── bench.spec.json       # English-only text ★
├── task_note.md          # English-only text ★
├── oracle/T01.sh …       # English-only text ★
├── .oracle/              # English-only text（gitignore）
│   ├── current_session.json   # English-only text session ★
│   ├── {run_id}_T01.json
│   ├── {run_id}_T01.injected.json
│   └── {run_id}_session.json
└── seed/ …
```

English-only text：[`examples/P01-aptui/`](examples/P01-aptui/)

---

## English-only text

- Oracle check API（`check_screen`、`check_screen_absent`、`check_filesystem` English-only text）：[`ORACLE_SCRIPT.md`](ORACLE_SCRIPT.md)
- Seed English-only text：[`SEED_MANIFEST.md`](SEED_MANIFEST.md)
- agent-tui（TUI English-only text）：[`../agent-tui/README.md`](../agent-tui/README.md)
- English-only text：[`scripts/bench.sh`](scripts/bench.sh)
