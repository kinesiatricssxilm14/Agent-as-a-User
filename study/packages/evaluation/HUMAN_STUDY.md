# TUI-Bench English-only text

> **English-only text：** English-only text *Methods* / *Human Baseline* English-only text。  
> **English-only text：** `keyboard-agent human run`（`framework: human-record`）  
> **English-only text：** [ORACLE_GUIDE.md](./ORACLE_GUIDE.md)（bench English-only text）、[ORACLE_SCRIPT.md](./ORACLE_SCRIPT.md)（English-only text）

---

## 1. English-only text

TUI-Bench English-only text 24 English-only text TUI English-only text、English-only text 90 English-only text LLM Agent English-only text。English-only text**English-only text（human baseline）**，English-only text Agent English-only text**English-only text bench English-only text、Oracle English-only text schema** English-only text。

English-only text：

- **English-only text：** English-only text vs. Agent English-only text Oracle English-only text PASS/FAIL English-only text；
- **English-only text：** English-only text（SVG）、English-only text、English-only text（turn）English-only text；
- **English-only text：** English-only text、English-only text、typing burst English-only text；
- **English-only text：** English-only text Agent English-only text `type` / `press` / `submit` English-only text vocabulary。

English-only text**English-only text**English-only text；ground truth English-only text Oracle English-only text（`bench.sh verify`）。

---

## 2. English-only text（Benchmark Corpus）

### 2.1 English-only text

English-only text `final_data/` English-only text TUI-Bench English-only text，English-only text：

- `bench.spec.json`：English-only text；
- `tasks.json`：English-only text（`description`）、English-only text；
- `oracle/Txx.sh`：English-only text `Txx` English-only text；
- Docker English-only text（`Dockerfile` + `host-setup.sh`）。

English-only text **24 English-only text**（English-only text `P01-aptui`、`P02-kairo`、`P17-tredis` English-only text），English-only text（English-only text 3–5 English-only text），English-only text **90 English-only text**。

### 2.2 English-only text

English-only text `bench.sh start Txx` English-only text Docker English-only text tmux session，English-only text TUI English-only text。English-only text `bench.sh task` English-only text JSON English-only text `description` English-only text；Web UI English-only text。

### 2.3 English-only text

English-only text Agent English-only text，English-only text：

| English-only text | English-only text |
|------|------|
| **English-only text** | tmux English-only text；Web UI English-only text |
| **English-only text submit** | English-only text **Ctrl-G** English-only text（English-only text TUI，English-only text Agent English-only text `submit` English-only text） |
| **English-only text Oracle** | English-only text `bench.sh verify --human`，English-only text Agent English-only text Oracle English-only text |

---

## 3. English-only text（English-only text）

> English-only text；English-only text。

| English-only text | English-only text |
|------|--------------|
| English-only text | *N* English-only text × English-only text |
| English-only text | English-only text：CS English-only text、TUI English-only text |
| English-only text | English-only text Linux/English-only text；English-only text |
| English-only text | English-only text Oracle English-only text；English-only text |
| English-only text | English-only text Vim/tmux/TUI English-only text |
| English-only text | English-only text（English-only text） |

**English-only text：** English-only text `meta.json` English-only text `participants.json` English-only text `operator_id`（English-only text `human.operator_id`），English-only text session，English-only text。

---

## 4. English-only text

### 4.1 English-only text

```
bench start(-all)  →  English-only text task description  →  English-only text TUI  →  Ctrl-G ×3 English-only text  →  bench verify  →  bench next / stop
         ↑                                                                                    ↑
    English-only text Agent English-only text                                                                    Oracle English-only text PASS/FAIL
```

English-only text `HumanSuiteRunner` English-only text，English-only text keystroke / turn English-only text。

### 4.2 English-only text（instrumented）

1. **English-only text：** `bench.sh start Txx`（English-only text `start-all` English-only text）；English-only text `--rebuild` English-only text。
2. **English-only text：** English-only text `initial_screen.json` English-only text `turns/turn_000.svg`。
3. **English-only text：** English-only text **English-only text attach（English-only text）** English-only text Web UI English-only text；English-only text `keystrokes.jsonl`；English-only text turn English-only text `transcript.jsonl` English-only text SVG。
4. **English-only text：** English-only text submit English-only text（English-only text Ctrl-G）English-only text Web UI「Submit task」English-only text；English-only text synthetic `submit` turn。
5. **English-only text：** `bench.sh verify --human --quiet`；English-only text `result.json` English-only text `passed` English-only text `verify` English-only text。
6. **English-only text：** `bench.sh next`（English-only text）English-only text `stop` English-only text。

### 4.3 English-only text

| English-only text | English-only text | English-only text |
|------|------|------|
| **Terminal attach（English-only text）** | `keyboard-agent human run -c config.yaml` | English-only text PTY attach English-only text tmux；English-only text；English-only text Bash English-only text |
| **Web UI（English-only text）** | `keyboard-agent human run -c config.yaml --web` | English-only text（250 ms English-only text SVG）；English-only text |

English-only text `HumanTurnRecorder` English-only text，turn English-only text artifact schema English-only text。

### 4.4 English-only text

English-only text `keyboard-agent/run_all_final_data_human.sh` English-only text `final_data/P*-*` English-only text；English-only text `start-all → task → English-only text → verify → next` English-only text。

---

## 5. Turn English-only text（English-only text）

English-only text：**English-only text turn**。English-only text turn English-only text、English-only text Agent turn English-only text，English-only text timing English-only text。

### 5.1 English-only text

English-only text（English-only text Agent English-only text vocabulary English-only text）：

| English-only text | English-only text | English-only text |
|------|------|----------|
| **Type key** | English-only text：English-only text、English-only text，English-only text Shift English-only text `/ . , - = [ ] \\ ; ' \`` | `{"action":"type","text":"..."}` |
| **Press key** | English-only text、Enter、Space、Tab、Backspace、Ctrl+*、Alt+*、F1–F12 English-only text `VALID_KEYS` English-only text type English-only text | `{"action":"press","key":"..."}` |
| **Submit sequence** | English-only text `ctrl+g ctrl+g ctrl+g`（English-only text） | `{"action":"submit"}`（English-only text TUI） |
| **Unmapped** | English-only text | English-only text `keystrokes.jsonl`，English-only text operation turn |

English-only text：`keyboard-agent/src/keyboard_agent/human/key_classifier.py`。

### 5.2 Turn English-only text

| English-only text | Turn English-only text |
|----------|-----------|
| **Type keys** | English-only text type English-only text turn（typing burst）；English-only text **> τ**（English-only text τ = 1000 ms），English-only text turn English-only text turn；English-only text，English-only text type English-only text idle English-only text，English-only text burst |
| **Press keys** | **English-only text turn**；English-only text type buffer English-only text，English-only text flush type turn English-only text press turn |
| **Submit** | English-only text session；flush English-only text type buffer；runner English-only text submit turn |

English-only text **`human.type_idle_ms`**（English-only text 1000 ms）English-only text type burst English-only text τ。English-only text CLI（`--idle-ms`）English-only text；English-only text（English-only text pilot English-only text 1 s English-only text typing pause English-only text）。

### 5.3 English-only text Agent turn English-only text

| English-only text | English-only text turn | Agent turn |
|------|-----------|------------|
| **English-only text** | English-only text + idle English-only text | English-only text LLM English-only text JSON action |
| **English-only text** | English-only text typing burst English-only text special key | English-only text（English-only text LLM English-only text `type`） |
| **English-only text** | **English-only text**English-only text（post-action observation） | **English-only text** observe，English-only text observe English-only text turn |
| **LLM English-only text** | `llm_latency_ms`、`token_usage`、`raw_agent_response` English-only text | English-only text |

**English-only text：** English-only text `transcript.jsonl` English-only text Agent English-only text `TurnRecord` schema，English-only text turn English-only text**English-only text motor chunking**，English-only text LLM English-only text；English-only text turn English-only text，English-only text **keystroke English-only text** English-only text **English-only text** PASS/FAIL / English-only text。

---

## 6. English-only text

### 6.1 Run English-only text

English-only text suite run English-only text ID：

```
<runner_label>-<Pxx-slug>-<observation_mode>-<YYYYMMDDTHHMMSSZ>
```

English-only text：`human-P02-kairo-semantic-20260622T034203Z`

- `runner_label`：English-only text `human`（English-only text `operator_id`）；
- `observation_mode`：`plain` / `semantic` / `svg` / `png`（English-only text，English-only text Oracle）；
- English-only text `-2`、`-3` English-only text。

### 6.2 English-only text

```
runs/<run_id>/
  meta.json                 # framework: "human-record", config English-only text
  summary.json              # English-only text suite English-only text
  T01/
    initial_screen.json     # turn 0 English-only text
    keystrokes.jsonl        # 【English-only text】English-only text
    transcript.jsonl        # English-only text turn English-only text（English-only text Agent English-only text schema）
    result.json             # English-only text + verify + English-only text turns
    T01_summary.json
    turns/
      turn_000.svg          # English-only text
      turn_001.svg          # English-only text 1 English-only text operation English-only text
      ...
```

### 6.3 `keystrokes.jsonl`（English-only text，English-only text）

English-only text JSON English-only text，English-only text：

```json
{
  "seq": 42,
  "pressed_at": "2026-06-22T03:42:15.123456+00:00",
  "interval_ms": 187.3,
  "key": "enter",
  "raw_bytes": "0d"
}
```

| English-only text | English-only text |
|------|------|
| `seq` | English-only text |
| `pressed_at` | UTC ISO8601 English-only text |
| `interval_ms` | English-only text（English-only text `null`） |
| `key` | English-only text（English-only text Agent `press`/`type` vocabulary English-only text） |
| `raw_bytes` | English-only text hex（Web English-only text） |

### 6.4 `transcript.jsonl`（turn English-only text，English-only text Agent English-only text schema）

English-only text `TurnRecord`，English-only text `action` English-only text：

```json
{
  "turn": 3,
  "started_at": "...",
  "ended_at": "...",
  "duration_ms": 842.1,
  "llm_latency_ms": null,
  "action": {
    "action": "type",
    "text": "apt install",
    "human_keystrokes": [ /* KeystrokeRecord[] */ ],
    "gap_before_ms": 1204.5
  },
  "action_ok": true,
  "screen_text": "...",
  "observation_mode": "semantic",
  "svg_file": "turns/turn_003.svg",
  "raw_agent_response": "",
  "container_alive": true
}
```

Press turn English-only text：`{"action":"press","key":"enter", ...}`。  
Submit turn：`{"action":"submit"}`，English-only text。

### 6.5 `result.json`（English-only text）

| English-only text | English-only text |
|------|--------------|
| `passed` | Oracle `verify` English-only text（ground truth） |
| `agent_id` | `{operator_id}:{task_id}`，English-only text `human:T01` |
| `stop_reason` | English-only text `agent_submit`（English-only text submit English-only text） |
| `total_token_usage` | English-only text |
| `turns` | English-only text turn English-only text（English-only text submit turn） |
| `verify` | Oracle English-only text details |

### 6.6 English-only text

- English-only text：SVG（`turns/turn_NNN.svg`），English-only text `agent-tui` English-only text tmux pane English-only text；
- English-only text：English-only text operation English-only text（background thread）；
- English-only text：English-only text、English-only text Agent English-only text、English-only text vision English-only text（Agent English-only text）。

---

## 7. English-only text Agent English-only text

English-only text LLM Agent English-only text **parallel benchmark design**：

| English-only text | English-only text | Agent |
|------|------|-------|
| Bench English-only text | `start → operate → verify → next` | English-only text |
| Oracle / ground truth | English-only text `oracle/Txx.sh` | English-only text |
| English-only text vocabulary | `type` / `press` / `submit` | English-only text（JSON English-only text） |
| English-only text schema | `TurnRecord` + SVG English-only text | English-only text |
| English-only text | English-only text / Web UI | `agent-tui` CLI |
| English-only text | `keystrokes.jsonl` | LLM transcript、token usage |
| `meta.json.framework` | `human-record` | `observe-act` |
| English-only text | English-only text `max_turns`（English-only text 200），session English-only text | English-only text |

**English-only text：**

- English-only text Agent English-only text**English-only text Docker English-only text、English-only text tmux session、English-only text、English-only text Oracle**；
- English-only text Agent English-only text JSON English-only text，English-only text；English-only text Agent English-only text；
- Agent English-only text `observation_mode: semantic | png | svg` English-only text；English-only text `semantic`（English-only text），English-only text TUI，English-only text modality ablation，English-only text observation format English-only text。

---

## 8. English-only text（English-only text）

### 8.1 English-only text（primary）

| English-only text | English-only text |
|------|------|
| **Pass rate** | Oracle `passed == true` English-only text |
| **Time on task** | `result.json` English-only text `started_at` → `ended_at` |
| **Turn count** | English-only text operation English-only text（English-only text submit） |

### 8.2 English-only text（secondary，English-only text）

| English-only text | English-only text |
|------|------|
| **Keystroke count** | `keystrokes.jsonl` English-only text |
| **Inter-key interval (IKI)** | `interval_ms` English-only text |
| **Typing burst length** | English-only text type turn English-only text `human_keystrokes` English-only text |
| **Navigation ratio** | press turn English-only text / English-only text turn English-only text |

### 8.3 English-only text Agent English-only text（exploratory）

- Pass rate delta（human − agent）English-only text / English-only text；
- English-only text Oracle `verify.details` English-only text；
- English-only text keystroke English-only text vs. Agent turn English-only text / token English-only text（English-only text）。

---

## 9. English-only text

### 9.1 English-only text

English-only text `keyboard-agent/config.human.example.yaml`：

```yaml
project_dir: /path/to/final_data/P01-aptui
output_dir: runs
observation_mode: semantic
human:
  type_idle_ms: 1000
  submit_key: ctrl+g ctrl+g ctrl+g
  operator_id: human
  web_host: 127.0.0.1
  web_port: 8765
  open_browser: false
```

### 9.2 English-only text

```bash
export BENCH_PROJECT=/path/to/final_data/P01-aptui
export BENCH=/path/to/evaluation/scripts/bench.sh

# English-only text，English-only text attach
keyboard-agent human run -c config.human.example.yaml --task T01

# English-only text，Web UI（English-only text）
keyboard-agent human run -c config.human.example.yaml --web --task T01

# English-only text
keyboard-agent human run -c config.yaml --idle-ms 1000 --submit-key "ctrl+g ctrl+g ctrl+g" --operator P001
```

### 9.3 English-only text

- `keyboard-agent` + `agent-tui`（editable install）；
- Docker + tmux（bench English-only text）；
- Web English-only text：English-only text HTTP server，English-only text `http://127.0.0.1:8765/`；
- SVG English-only text：`agent-tui`；PNG English-only text `librsvg`（`brew install librsvg`）。

### 9.4 English-only text（English-only text implementation validation）

| English-only text | English-only text |
|----------|----------|
| `tests/test_human_key_parser.py` | English-only text；type vs. press English-only text；turn English-only text（English-only text idle English-only text） |
| `tests/test_run_id.py` | run ID English-only text |

---

## 10. English-only text

| English-only text | English-only text |
|------|------|
| **Turn English-only text** | τ = 1 s English-only text type burst English-only text，English-only text ground truth；English-only text τ English-only text turn English-only text |
| **Post-action English-only text** | English-only text turn English-only text SVG English-only text**English-only text**English-only text，Agent English-only text**English-only text**English-only text；English-only text off-by-one |
| **Web vs. terminal** | English-only text；English-only text |
| **English-only text** | English-only text、Docker English-only text TUI，English-only text gap |
| **English-only text** | English-only text，English-only text bench English-only text pass rate |
| **English-only text eye-tracking / think-aloud** | English-only text，English-only text |
| **Oracle English-only text ground truth** | Oracle English-only text，human pass rate English-only text |

---

## 11. English-only text（English-only text）

### Methods — Human baseline collection

We collected human performance baselines on TUI-Bench using an instrumented recording harness (`keyboard-agent human run`) that preserves the same Docker/tmux environments, task descriptions, and oracle-based success criteria as our LLM agent evaluation. Participants completed each task by interacting with the target TUI via keyboard only; mouse input was disabled. Tasks were submitted with a dedicated key sequence (three consecutive Ctrl-G presses by default) that was not forwarded to the application, mirroring the agent's `submit` action. Keystrokes were logged at millisecond resolution (`keystrokes.jsonl`). Operations were segmented into turns using a hybrid rule: printable characters were grouped into typing bursts separated by idle gaps exceeding τ = 1 s, while navigation and control keys each formed an individual turn. After submission, task success was determined by the same oracle scripts used for agents (`bench.sh verify`). Trajectories were stored in a shared JSONL schema with per-turn SVG screen captures, enabling direct comparison of pass rates, timing, and action sequences between humans and agents.

### Methods — Turn segmentation (formal)

Let $k_1, k_2, \ldots$ denote the sequence of keyed inputs with timestamps. Keys are classified as *type* (alphanumeric and selected punctuation) or *press* (all other valid keys in the agent action space). A *type turn* is a maximal contiguous subsequence of type keys such that inter-key intervals are at most τ ms (τ = 1000 by default), closed either by a press key, an idle timeout, or submit. A *press turn* contains exactly one press key. Each turn is associated with a post-action terminal snapshot.

---

## 12. English-only text

| English-only text | English-only text |
|------|------|
| `keyboard-agent/src/keyboard_agent/human/runner.py` | `HumanSuiteRunner`：bench English-only text |
| `keyboard-agent/src/keyboard_agent/human/turn_recorder.py` | Turn English-only text |
| `keyboard-agent/src/keyboard_agent/human/session.py` | English-only text attach |
| `keyboard-agent/src/keyboard_agent/human/web_server.py` | Web UI |
| `keyboard-agent/src/keyboard_agent/recorder.py` | Artifact English-only text、run ID |
| `keyboard-agent/config.human.example.yaml` | English-only text |
| `evaluation/scripts/bench.sh` | Bench CLI（start / verify / next） |

---

*English-only text：2026-06-22 · English-only text keyboard-agent human-record English-only text*
