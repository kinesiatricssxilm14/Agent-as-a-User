# Oracle English-only text（v3）

**English-only text：** `evaluation/scripts/bench.sh`  
**English-only text：** `.oracle/current_session.json` + `session_Txx.json` + `Txx_fingerprints.json`  
**English-only text ID，English-only text run_id。**

---

## 1. English-only text（English-only text）

```bash
export BENCH_PROJECT=/path/to/anonymous-artifact
BENCH=/path/to/anonymous-artifact

$BENCH start T01 --human     # [READY] P01 T01 (shell)
$BENCH task                  # English-only text
# English-only text：$BENCH use English-only text TUI English-only text；Agent：English-only text agent-tui（English-only text §3）
$BENCH verify --human        # [PASS] P01 T01 (shell)  English-only text  [FAIL] ...
$BENCH stop
```

## 2. English-only text（English-only text）

```bash
export BENCH_PROJECT=/path/to/anonymous-artifact
BENCH=/path/to/anonymous-artifact

$BENCH start-all --human --rebuild      # English-only text，English-only text T01 — 1/4
$BENCH task                   # ① English-only text（JSON）
# ② English-only text/Agent English-only text TUI —— English-only text §3，bench.sh English-only text
$BENCH verify --human         # ③ English-only text：[PASS] P01 T01 (shell)
$BENCH next --human           # English-only text T01，English-only text T02 — 2/4
# English-only text：task → English-only text TUI → verify → next
$BENCH stop                   # English-only text stop English-only text
```

---

## 3. English-only text：English-only text / Agent English-only text TUI

`**bench.sh` English-only text「English-only text」English-only text「English-only text」，English-only text。**  
`task` English-only text `verify` English-only text，English-only text**English-only text**English-only text**English-only text Agent** English-only text TUI English-only text。

### English-only text


| English-only text              | English-only text                                                                           |
| -------------- | ----------------------------------------------------------------------------- |
| `**bench.sh`** | `start` / `start-all` English-only text Docker+tmux；`task` English-only text；`verify` English-only text oracle English-only text PASS/FAIL |
| **English-only text**          | `bench.sh use` English-only text tmux，English-only text aptui English-only text TUI                                       |
| **Agent**      | English-only text `bench.sh task` English-only text JSON，English-only text `**agent-tui`** CLI English-only text/English-only text（English-only text bench.sh）           |


### English-only text（English-only text）

```bash
$BENCH task                   # English-only text description：English-only text「English-only text aptui English-only text tree English-only text」
$BENCH use                    # English-only text tmux（attach English-only text TUI）
# English-only text TUI English-only text、English-only text、English-only text……
# English-only text Ctrl+B English-only text D English-only text detach English-only text shell，English-only text
$BENCH verify --human
```

`use` English-only text `agent-tui use <session_id>`，session_id English-only text `.oracle/session_Txx.json`，English-only text。

### Agent English-only text（English-only text）

Agent **English-only text** `bench.sh use`（English-only text）。English-only text：

```bash
# 1. English-only text（Agent English-only text JSON）
TASK=$($BENCH task)
# English-only text：task_id, observation, description, fingerprints

# 2. English-only text tmux session（English-only text；English-only text agent-tui English-only text session）
agent-tui use "$(jq -r .session_id "$BENCH_PROJECT/.oracle/session_T01.json")"

# 3. English-only text（session English-only text start/start-all English-only text，English-only text start）
agent-tui snapshot --format semantic
agent-tui press down
agent-tui type "/tree\n"
agent-tui press enter
agent-tui wait --text tree
# … English-only text description English-only text press / type / snapshot …

# 4. English-only text
$BENCH verify --human
```

P03 flow English-only text TUI：English-only text，English-only text `r` English-only text；English-only text `bench.sh stop` English-only text `start`。

English-only text **agent-tui** English-only text（English-only text `[agent-tui/README.md](../agent-tui/README.md)`）：


| English-only text                                     | English-only text                  |
| -------------------------------------- | ------------------- |
| `agent-tui snapshot --format semantic` | English-only text（English-only text/English-only text，English-only text Agent） |
| `agent-tui press <key>`                | English-only text、Enter、Esc English-only text     |
| `agent-tui type "<text>\n"`            | English-only text             |
| `agent-tui wait --text <pattern>`      | English-only text            |


Agent English-only text `**agent-tui use` English-only text**：English-only text session English-only text `snapshot` / `press` / `type`；English-only text「English-only text session」，English-only text `agent-tui use $(jq -r .session_id .oracle/session_T01.json)` English-only text `current_session.json` English-only text active English-only text `session_*.json`。

### English-only text（English-only text）

```
start-all ──► task（English-only text）──► 【English-only text/Agent English-only text TUI】──► verify ──► next ──► …
                  │                    │                    │
            bench.sh            agent-tui English-only text           bench.sh
                              English-only text tmux
```

---

## 4. `.oracle/` English-only text


| English-only text                      | English-only text                                            |
| ----------------------- | --------------------------------------------- |
| `current_session.json`  | English-only text suite：active_task、task_order、English-only text status     |
| `session_T01.json`      | T01 English-only text container_id / session_id / description |
| `session_T02.json`      | T02 English-only text …                                       |
| `T01_fingerprints.json` | T01 English-only text（English-only text + session English-only text，English-only text）               |


`stop` English-only text suite English-only text `current_session.json` English-only text `session_*.json`；English-only text。

### English-only text


| English-only text                      | English-only text                                | English-only text                                                             |
| ----------------------- | --------------------------------- | -------------------------------------------------------------- |
| English-only text + oracle `check_*` | verify                            | `description` English-only text `{{screen}}`；English-only text `load_fingerprints` → `$FP_*` |
| English-only text seed English-only text            | **start English-only text** staging + `docker cp` | English-only text `seed/` English-only text `{{FP_SCREEN}}`；board English-only text                        |


English-only text `[ORACLE_SCRIPT.md](./ORACLE_SCRIPT.md)` §5。

---

## 5. English-only text


| English-only text                                                     | English-only text                                     |
| ------------------------------------------------------ | -------------------------------------- |
| `start-all`                                            | English-only text suite：English-only text，English-only text                  |
| `start T01`                                            | English-only text                                   |
| `next`                                                 | English-only text → English-only text                            |
| `verify`                                               | English-only text `[PASS/FAIL] Pxx Txx (observation)` |
| `task` / `use` / `stop` / `current` / `list` / `build` | English-only text `bench.sh --help`                    |


---

## 6. English-only text

- English-only text：`bench.spec.json`（CLI English-only text）
- English-only text：`task_note.md`
- English-only text：`oracle/Txx.sh`
- check API：`[ORACLE_SCRIPT.md](ORACLE_SCRIPT.md)`
- English-only text CLI（English-only text）：`bench-oracle --project-dir … session verify --json`

