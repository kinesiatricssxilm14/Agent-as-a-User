# Oracle English-only text（`oracle/Txx.sh`）

English-only text Benchmark English-only text **`oracle/T01.sh`** English-only text：English-only text、English-only text、English-only text。

English-only text：

| English-only text | English-only text |
|------|------|
| [`scripts/oracle_lib.sh`](scripts/oracle_lib.sh) | oracle English-only text bash English-only text（**English-only text API English-only text**） |
| [`templates/oracle/Txx.sh.template`](templates/oracle/Txx.sh.template) | English-only text |
| [`examples/P01-aptui/oracle/`](examples/P01-aptui/oracle/) | English-only text |
| [`ORACLE_GUIDE.md`](ORACLE_GUIDE.md) | English-only text（Docker / agent-tui） |

---

## 1. English-only text

| English-only text | English-only text | English-only text |
|------|--------|------|
| **`tasks.json`** | English-only text `description` + `observation` | English-only text、Agent、Harness |
| **`oracle/Txx.sh`** | **English-only text**English-only text | `bench-oracle run` |
| **`.oracle/Txx_fingerprints.json`** | English-only text run English-only text token | oracle English-only text、`inject-task` |

**English-only text：** English-only text `oracle/Txx.sh`，English-only text `tasks.json`。

---

## 2. English-only text

- English-only text **English-only text** English-only text，English-only text `oracle/T01.sh`
- **`exit 0` = PASS**，English-only text `check_*` English-only text（exit 1）+ `set -e` → English-only text **FAIL**
- English-only text `set -euo pipefail`：
  - `-e`：English-only text
  - `-u`：English-only text
  - `-o pipefail`：English-only text

---

## 3. English-only text

```bash
#!/usr/bin/env bash
set -euo pipefail

# ① English-only text（run English-only text bench-oracle English-only text ORACLE_FP_FILE）
: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

# ② English-only text
source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

# ③ English-only text：FP_FILE_NAME, FP_FILE_CONTENT, FP_SCREEN, FP_ANSWER, FP_RUN_ID
load_fingerprints

# ④ English-only text（English-only text，English-only text，English-only text FAIL）
check_shell "..."
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"

oracle_msg "T01 PASS"
```

**English-only text（English-only text）：**

```bash
export BENCH_ORACLE_LIB=/path/to/evaluation/scripts/oracle_lib.sh
export PROJECT=/path/to/dockerfiles/bench50-core/P01-aptui
```

---

## 4. `bench-oracle run` English-only text

| English-only text | English-only text | English-only text |
|------|------|------|
| `ORACLE_PROJECT_DIR` | `.../P01-aptui` | English-only text |
| `ORACLE_TASK_ID` | `T01` | English-only text ID |
| `ORACLE_OBSERVATION` | `filesystem` | English-only text `tasks.json` |
| `ORACLE_FP_FILE` | `.../.oracle/T01_fingerprints.json` | English-only text JSON English-only text |
| `ORACLE_DOCKER_CONTAINER` | `p01-bench` | shell/filesystem English-only text `docker exec` |
| `AGENT_TUI_SESSION` | `p01-bench` | screen English-only text tmux session |
| `ORACLE_AGENT_ANSWER` | `ans_p01_...` | answer English-only text |
| `ORACLE_SEARCH_ROOTS` | `/bench/data,/tmp` | English-only text（English-only text） |
| `ORACLE_TERMINAL_THEME` | `dark`（English-only text） | screen English-only text：`dark` / `light` |

**`run` English-only text：**

```bash
bench-oracle --project-dir "$PROJECT" run T01 \
  --docker-container p01-bench \   # filesystem / shell English-only text
  --session p01-bench \            # screen English-only text
  --human                          # English-only text PASS/FAIL English-only text
```

| `observation` | English-only text `--docker-container` | English-only text `--session` |
|---------------|---------------------------|------------------|
| `filesystem` | ✅ | ❌ |
| `screen` | ❌ | ✅ |
| `answer` | ❌ | ❌（English-only text `--agent-answer`，**English-only text**） |

---

## 5. English-only text：English-only text

English-only text `bench.sh start` / `start-all` English-only text `.oracle/Txx_fingerprints.json`。English-only text token English-only text，English-only text：

| English-only text | English-only text | English-only text | English-only text |
|------|------|------|----------|
| **English-only text / Oracle English-only text** | verify English-only text | `load_fingerprints` → `FP_*`；`bench.spec.json` English-only text `{{screen}}` English-only text | `check_screen "$FP_SCREEN"`、`check_filesystem_content "$FP_FILE_CONTENT"` |
| **Seed English-only text materialize** | **TUI English-only text** | English-only text staging → `docker cp` English-only text | English-only text `FLOW-4.md` English-only text `{{FP_SCREEN}}`；board English-only text（T02 English-only text） |

### 5.1 Oracle English-only text：`load_fingerprints`

`load_fingerprints` English-only text `ORACLE_FP_FILE` English-only text JSON，**English-only text**English-only text `check_*` English-only text：

| English-only text | JSON English-only text | English-only text |
|------|-----------|----------|
| `FP_FILE_NAME` | `file_name` | English-only text token |
| `FP_FILE_CONTENT` | `file_content` | English-only text token |
| `FP_SCREEN` | `screen` | English-only text token |
| `FP_ANSWER` | `answer` | Agent English-only text token（English-only text） |
| `FP_RUN_ID` | `run_id` | English-only text run English-only text |

**English-only text `$FP_*`，English-only text。**

### 5.2 Seed English-only text：English-only text materialize（English-only text）

English-only text `seed/` English-only text。**English-only text `bench.spec.json`。**

**English-only text：**

1. Dockerfile English-only text `COPY seed/ /bench/`
2. English-only text `seed/` English-only text
3. English-only text `seed/init.sh` English-only text `cp -r /bench/SRC /bench/DEST` English-only text copy English-only text（English-only text P03 → `/bench/data/board`）
4. English-only text `.oracle/staged_seed/Txx/` English-only text + English-only text（**English-only text seed**）
5. `docker cp` English-only text → `docker exec /entrypoint.sh flow` English-only text TUI

**English-only text：**

1. `bench.sh start` English-only text
2. staging：English-only text seed English-only text `.oracle/staged_seed/Txx/`，English-only text staged English-only text `{{FP_*}}`
3. `docker run -d … sleep` English-only text（English-only text TUI）
4. `docker cp` staged English-only text → `/bench/data/board`（English-only text）
5. `docker exec /entrypoint.sh flow` English-only text TUI

English-only text（English-only text）：

| English-only text | English-only text |
|--------|----------|
| `{{FP_FILE_NAME}}` / `{{file_name}}` | `file_name` |
| `{{FP_FILE_CONTENT}}` / `{{file_content}}` | `file_content` |
| `{{FP_SCREEN}}` / `{{screen}}` | `screen` |
| `{{FP_ANSWER}}` / `{{answer}}` | `answer` |
| `{{FP_RUN_ID}}` / `{{run_id}}` | `run_id` |

English-only text（`seed/boards/demo/cols/in_progress/FLOW-4.md`）：

```markdown
# Polish focused column styling

Subtle focus color; readable defaults.
{{FP_SCREEN}}
```

English-only text seed English-only text（English-only text）：

P03 English-only text — English-only text seed English-only text，`init.sh` English-only text copy English-only text：

```sh
# seed/init.sh
cp -r /bench/boards/demo /bench/data/board
```

```markdown
# seed/boards/demo/cols/in_progress/FLOW-4.md
...
{{FP_SCREEN}}
```

English-only text `seed/` English-only text；staged English-only text `.oracle/staged_seed/Txx/`，stop English-only text。

Session start English-only text **TUI English-only text** `docker cp`；`session_Txx.json` English-only text `seed_injections` English-only text `seed_docker_copies`。

English-only text：

```bash
bench-oracle --project-dir "$PROJECT" gen-fingerprints T01
```

---

## 6. English-only text

English-only text [`scripts/oracle_lib.sh`](scripts/oracle_lib.sh)。English-only text exit 1，English-only text exit 0。

| English-only text | English-only text | English-only text |
|------|------|----------|
| `load_fingerprints` | English-only text | English-only text |
| `oracle_msg MSG` | English-only text `[oracle] MSG` English-only text stderr | English-only text |
| `check_shell CMD EXPECT` | English-only text shell English-only text | Docker English-only text（English-only text `ORACLE_DOCKER_CONTAINER`） |
| `check_filesystem NAME CONTENT [MODE]` | English-only text | Docker English-only text |
| `check_filesystem_name NAME` | English-only text | Docker English-only text |
| `check_filesystem_content CONTENT` | English-only text | Docker English-only text |
| `check_screen PATTERN` | English-only text pattern | English-only text tmux（agent-tui） |
| `check_screen_absent PATTERN` | English-only text**English-only text** pattern（`check_screen` English-only text） | English-only text tmux（agent-tui） |
| `check_screen_highlight [FLAGS...]` | English-only text + English-only text/English-only text | English-only text tmux（semantic snapshot） |
| `check_screen_color FG [BG]` | English-only text | English-only text tmux（semantic snapshot） |
| `check_answer [EXPECTED]` | English-only text Agent English-only text（**English-only text，English-only text**） | English-only text |

---

## 7. `check_shell`

English-only text（English-only text）English-only text shell English-only text，English-only text。

### English-only text

```bash
check_shell "CMD" "EXPECT"
```

| English-only text | English-only text | English-only text |
|------|------|------|
| `CMD` | ✅ | English-only text shell English-only text |
| `EXPECT` | ❌ | English-only text（English-only text）；English-only text exit code English-only text 0 |

### `EXPECT` English-only text

| English-only text | English-only text |
|------|------|
| `"1"` | English-only text**English-only text**English-only text `"1"` |
| `"exists"` | English-only text `exists` |
| `">=1"` | English-only text ≥ 1 |
| `"regex:^ii.*tree"` | English-only text |
| `""`（English-only text） | English-only text **exit code English-only text 0** |

> English-only text `EXPECT` English-only text「English-only text」，**English-only text** exit code。

### English-only text

```bash
# English-only text tree English-only text
check_shell "dpkg -l tree 2>/dev/null | grep -c '^ii'" "1"

# English-only text curl English-only text（grep English-only text || true English-only text set -e English-only text）
check_shell "dpkg -l curl 2>&1 | grep -c '^ii' || true" "0"

# English-only text
check_shell "test -f /bench/data/export.csv && echo exists" "exists"

# sqlite English-only text
check_shell "sqlite3 /bench/data/bench.db \"SELECT name FROM employees WHERE name='Alice'\"" "Alice"

# English-only text
check_shell "systemctl is-active redis" ""
```

---

## 8. `check_filesystem`

English-only text：English-only text/English-only text token。

### English-only text

```bash
check_filesystem "NAME_FP" "CONTENT_FP" [MODE]
check_filesystem_name "NAME_FP"
check_filesystem_content "CONTENT_FP"
```

| English-only text | English-only text | English-only text |
|------|------|------|
| `NAME_FP` | ❌ | English-only text token；English-only text |
| `CONTENT_FP` | ❌ | English-only text token；English-only text |
| `MODE` | ❌ | `name-only` / `name`：English-only text；`content-only` / `content`：English-only text |

**English-only text：** English-only text，English-only text**English-only text**English-only text（English-only text token **English-only text** English-only text token）。

### English-only text

**English-only text：** `/bench`、`/data`、`/root`、`/tmp`、`/home`、English-only text。

**English-only text：**

```bash
export ORACLE_SEARCH_ROOTS="/bench/data,/tmp"
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"
```

### English-only text

```bash
# English-only text：English-only text + English-only text（English-only text）
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"

# English-only text token
check_filesystem_name "$FP_FILE_NAME"

# English-only text token
check_filesystem_content "$FP_FILE_CONTENT"

# English-only text
check_filesystem "$FP_FILE_NAME" "" name-only
check_filesystem "" "$FP_FILE_CONTENT" content-only

# English-only text + shell English-only text（English-only text）
check_shell "grep -qF '$FP_FILE_CONTENT' /bench/data/proof.txt && echo ok" "ok"
```

---

## 9. `check_screen`

English-only text **agent-tui** English-only text tmux session English-only text，English-only text（plain / semantic English-only text，English-only text semantic）。

### English-only text

```bash
check_screen "PATTERN"
```

| English-only text | English-only text | English-only text |
|------|------|------|
| `PATTERN` | ❌ | English-only text；English-only text `$FP_SCREEN` |

English-only text。English-only text `bench-oracle run --session <label>`。

### English-only text

```bash
# English-only text：English-only text wget English-only text
check_screen "wget"

# English-only text：English-only text run English-only text
check_screen "$FP_SCREEN"
```

### English-only text：`check_screen_absent`

`check_screen` English-only text：English-only text**English-only text**English-only text。English-only text **delete** English-only text（English-only text）、English-only text、English-only text。

```bash
check_screen_absent "PATTERN"
```

| English-only text | English-only text | English-only text |
|------|------|------|
| `PATTERN` | ✅ | English-only text**English-only text**English-only text（English-only text `$FP_SCREEN` English-only text，English-only text） |

English-only text `check_screen` English-only text（English-only text、semantic/plain English-only text）。English-only text `bench-oracle run --session <label>`。

### English-only text

```bash
# English-only text
check_screen_absent "FLOW-1"

# English-only text
check_screen_absent "Error:"

# English-only text：English-only text，English-only text
check_screen "Board: demo"
check_screen_absent "deleted-card-title"
oracle_msg "T04 PASS"
```

CLI English-only text：`bench-oracle check screen-absent --pattern TOKEN`

---

## 10. `check_screen_highlight`

English-only text **agent-tui semantic snapshot**（`snapshot --format semantic`），English-only text，English-only text / English-only text。

semantic English-only text：

```
Packages <fg:yellow>wget</fg:yellow> <bg:blue>selected</bg:blue>
```

### English-only text

```bash
check_screen_highlight --text TEXT [OPTIONS...]
```

| English-only text | English-only text |
|------|------|
| `--text TEXT` | **English-only text**。English-only text |
| `--fg COLOR` | English-only text**English-only text**English-only text（English-only text `yellow`、`red`） |
| `--bg COLOR` | English-only text**English-only text**English-only text（English-only text `blue`） |
| `--colored` | English-only text**English-only text**English-only text（English-only text white/black English-only text） |
| `--highlighted` | English-only text**English-only text**English-only text |
| `--colored --highlighted` | English-only text visually distinct English-only text |

**English-only text：**

- `--fg` / `--bg`：**English-only text** semantic English-only text，English-only text white English-only text PASS
- `--colored` / `--highlighted`：English-only text，**English-only text**（English-only text `\x1b[37m` white English-only text colored）
- English-only text：`ORACLE_TERMINAL_THEME=dark`；English-only text `light`

English-only text `bench-oracle run --session <label>`。

### English-only text

```bash
# English-only text（English-only text）
check_screen_highlight --text "wget"

# English-only text（English-only text white/gray English-only text）
check_screen_highlight --text "$FP_SCREEN" --colored

# English-only text
check_screen_highlight --text "$FP_SCREEN" --highlighted

# English-only text（fg English-only text bg English-only text）
check_screen_highlight --text "$FP_SCREEN" --colored --highlighted

# English-only text
check_screen_highlight --text "wget" --fg yellow
check_screen_highlight --text "selected" --bg blue
check_screen_highlight --text "error" --fg red --bg blue
```

### English-only text screen English-only text

```bash
check_screen "wget"                                          # English-only text
check_screen_highlight --text "$FP_SCREEN" --colored         # English-only text
oracle_msg "T03 PASS"
```

---

## 11. `check_screen_color`

English-only text**English-only text span** English-only text（English-only text）。

### English-only text

```bash
check_screen_color "FG" ["BG"]
```

| English-only text | English-only text | English-only text |
|------|------|------|
| `FG` | ❌* | English-only text；English-only text `BG` English-only text |
| `BG` | ❌* | English-only text |

### English-only text

```bash
# English-only text（English-only text）
check_screen_color "red"

# English-only text
check_screen_color "red" "blue"
```

---

## 12. `check_answer`（English-only text，English-only text）

English-only text Agent **English-only text**English-only text。English-only text `--agent-answer` English-only text `ORACLE_AGENT_ANSWER`。

**English-only text `filesystem` / `screen` / `check_screen_highlight`。** `check_answer` English-only text「English-only text Agent English-only text」English-only text。

### English-only text

```bash
check_answer ["EXPECTED"]
```

| English-only text | English-only text |
|------|------|
| `EXPECTED` | English-only text；English-only text `$FP_ANSWER` |

### English-only text

```bash
check_answer "$FP_ANSWER"
```

---

## 13. English-only text vs English-only text

English-only text：**English-only text**（English-only text `observation`）+ **English-only text**（English-only text TUI English-only text）。

| observation | English-only text | English-only text |
|-------------|--------|--------------|
| `filesystem` | `check_filesystem` | `check_shell`（dpkg / sqlite / English-only text） |
| `screen` | `check_screen` + `check_screen_highlight` | `check_screen`（UI English-only text）；delete English-only text `check_screen_absent` |
| `answer` | `check_answer` | `check_shell`（English-only text） |

English-only text，**English-only text FAIL**。

---

## 14. English-only text `observation` English-only text

```
tasks.json English-only text observation English-only text？
│
├─ filesystem
│   └─ check_filesystem（English-only text check_filesystem_name / _content）
│      English-only text check_shell English-only text / DB / English-only text
│
├─ screen
│   └─ check_screen + check_screen_highlight
│      delete / English-only text check_screen_absent
│      run English-only text --session
│
└─ answer（English-only text）
    └─ check_answer
       run English-only text --agent-answer
```

---

## 15. English-only text（P01-aptui）

### T01 — filesystem：English-only text tree + English-only text

```bash
#!/usr/bin/env bash
set -euo pipefail
: "${ORACLE_FP_FILE:?}"
source "${BENCH_ORACLE_LIB:?}"
load_fingerprints

check_shell "dpkg -l tree 2>/dev/null | grep -c '^ii'" "1"
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"

oracle_msg "T01 PASS"
```

### T02 — filesystem：English-only text curl

```bash
check_shell "dpkg -l curl 2>&1 | grep -c '^ii' || true" "0"
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"
oracle_msg "T02 PASS"
```

### T03 — screen：wget English-only text + English-only text

```bash
check_screen "wget"
check_screen "$FP_SCREEN"
oracle_msg "T03 PASS"
```

**English-only text（English-only text）：**

```bash
check_screen "wget"
check_screen_highlight --text "$FP_SCREEN" --colored
oracle_msg "T03 PASS"
```

### T04 — answer（English-only text，English-only text）

```bash
check_answer "$FP_ANSWER"
oracle_msg "T04 PASS"
```

---

## 16. English-only text

### 16.1 `tasks.json`

```json
{
  "id": "T05",
  "description": "English-only text employees English-only text Alice；English-only text {{file_name}} / {{file_content}} English-only text",
  "observation": "filesystem",
  "difficulty": "medium",
  "type": "create"
}
```

### 16.2 `oracle/T05.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
: "${ORACLE_FP_FILE:?}"
source "${BENCH_ORACLE_LIB:?}"
load_fingerprints

check_shell "sqlite3 /bench/data/bench.db \"SELECT name FROM employees WHERE name='Alice'\"" "Alice"
check_filesystem "$FP_FILE_NAME" "$FP_FILE_CONTENT"

oracle_msg "T05 PASS"
```

```bash
chmod +x oracle/T05.sh
```

### 16.3 English-only text

```bash
bench-oracle --project-dir "$PROJECT" gen-fingerprints T05
bench-oracle --project-dir "$PROJECT" inject-task T05
# ... Agent English-only text ...
bench-oracle --project-dir "$PROJECT" run T05 \
  --docker-container p01-bench --human
```

---

## 17. English-only text CLI（English-only text check）

oracle English-only text `check_*` English-only text `python3 -m oracle.cli check ...`。English-only text：

```bash
bench-oracle check shell --cmd "dpkg -l tree | grep -c '^ii'" --expect "1"
bench-oracle check filesystem --name-fp TOKEN --content-fp TOKEN --roots /bench,/tmp
bench-oracle check screen --pattern TOKEN
bench-oracle check screen-absent --pattern TOKEN
bench-oracle check screen-highlight --text wget --fg yellow
bench-oracle check screen-color --fg red
```

---

## 18. English-only text

1. **English-only text `gen-fingerprints`** → `ORACLE_FP_FILE` English-only text
2. **Docker English-only text `--docker-container`** → `check_shell` English-only text macOS English-only text，`dpkg` English-only text
3. **screen English-only text `--session`** → English-only text tmux session
4. **`check_shell` English-only text expect English-only text exit code** → English-only text expect English-only text「English-only text」
5. **`set -e` + English-only text** → English-only text `|| true`（English-only text T02 grep）
6. **English-only text** → English-only text run English-only text，English-only text `$FP_*`
7. **English-only text `tasks.json` English-only text oracle** → `FileNotFoundError: oracle/Txx.sh`
8. **Docker English-only text `--rm`** → English-only text oracle English-only text
9. **`--colored` English-only text `--fg white`** English-only text：English-only text，English-only text white
10. **English-only text `check_screen_highlight`，English-only text `check_answer`**

---

## 19. English-only text

```bash
# English-only text
source "${BENCH_ORACLE_LIB:?}"
load_fingerprints

# English-only text
check_shell  "CMD" "EXPECT"

# English-only text
check_filesystem              "$FP_FILE_NAME" "$FP_FILE_CONTENT"
check_filesystem_name         "$FP_FILE_NAME"
check_filesystem_content      "$FP_FILE_CONTENT"

# English-only text
check_screen                  "pattern"
check_screen_absent           "pattern"    # English-only text：English-only text pattern
check_screen_highlight        --text "pattern" [--fg C] [--bg C] [--colored] [--highlighted]
check_screen_color            "fg" ["bg"]

# English-only text
check_answer                  "$FP_ANSWER"
```
