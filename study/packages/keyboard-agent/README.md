# Keyboard Agent

LLM **keyboard agent** for [TUI-Bench](../evaluation/ORACLE_GUIDE.md): wraps `bench.sh` + internal PTY driver. The model only sees **press / type / wait / submit** — never `agent-tui` or other tools.

## Architecture

**Observe-Act loop** (ReAct-style action protocol, custom implementation — not LangChain / AutoGPT):

```
bench.sh start-all
  └─ for each task (fresh LLM context):
       observe screen (text to LLM, SVG saved to disk)
         → LLM returns one JSON action
         → execute press/type/wait
         → record tokens, timing, SVG trajectory
       on submit | max_turns | container exit → bench verify → record → next
bench.sh stop
```

| Layer | Role |
|-------|------|
| `bench.sh` | Docker + tmux lifecycle, task JSON, oracle verify |
| `KeyboardDriver` | Internal PTY control (uses agent-tui library, **not exposed**) |
| `OpenAIAgent` | LLM dialogue + token accounting |
| `RunRecorder` | All artifacts under `runs/<run_id>/` |

## Install

```bash
cd keyboard-agent
pip install -e .
pip install -e ../agent-tui
pip install -e ".[llm]"
```

## Run

```bash
export BENCH_PROJECT=/path/to/anonymous-artifact
export BENCH=/path/to/anonymous-artifact
export OPENAI_API_KEY=sk-...

keyboard-agent init-config config.yaml
keyboard-agent run --config config.yaml
```

## Config highlights

```yaml
max_turns: 200
observation_mode: semantic   # plain | semantic | png | svg
output_dir: runs
```

### `observation_mode` — what the LLM receives

| Value | LLM input | Model requirement |
|-------|-----------|-------------------|
| `plain` | Plain terminal text | Text model |
| `semantic` | Text with `[bold]` / color role tags | Text model (**default**) |
| `png` | PNG image (base64 multimodal) | Vision model (e.g. `gpt-4o`) |
| `svg` | SVG image (base64 multimodal) | Vision model |

Trajectory logs **always** save `turns/turn_NNN.svg` (and `turn_NNN.png` when mode is `png`), regardless of `observation_mode`.

```bash
keyboard-agent run --observation-mode plain
keyboard-agent run --observation-mode png --model gpt-4o
```

## Agent protocol

One JSON object per turn:

```json
{"action": "press", "key": "down"}
{"action": "type", "text": "tree\n"}
{"action": "wait", "ms": 3000}
{"action": "submit"}
```

## Stop conditions → verify

| Reason | When |
|--------|------|
| `agent_submit` | Model returned `submit` |
| `max_turns` | Exceeded `max_turns` (default 200) |
| `container_exit` | Bench Docker container no longer running |

All paths run `$BENCH verify` and record the result.

## Run artifacts (`runs/<run_id>/`)

Run directory names: `<model|human>-<Pxx-slug>-<mode>-<YYYYMMDDTHHMMSSZ>`  
Examples: `human-P01-aptui-semantic-20260622T034203Z`, `deepseek-v4-flash-P02-kairo-plain-20260622T120000Z`

```
meta.json
summary.json
T01/
  initial_screen.json    # turn 0 text + svg path
  transcript.jsonl       # one JSON line per turn (full detail)
  result.json            # complete task record
  turns/
    turn_000.svg
    turn_001.svg
    ...
T01_summary.json
```

Each turn records:

- `started_at`, `ended_at`, `duration_ms`, `llm_latency_ms`
- `token_usage`: `prompt_tokens`, `completion_tokens`, `total_tokens`, **`reasoning_tokens`** (thinking), `cached_prompt_tokens`
- `action`, `screen_text`, `svg_file`, `raw_agent_response`
- `verify` result in `result.json`

## Custom agent

Implement `KeyboardAgent` in `src/keyboard_agent/agents/base.py`, register in `create_agent()`.

## Framework note

This is a **minimal Observe-Act loop** inspired by [ReAct](https://arxiv.org/abs/2210.03629) (interleave observation → action) but:

- No Thought/Action/Observation text format — actions are JSON only
- No LangChain / LlamaIndex dependency
- One environment step per LLM call
- Fresh agent instance per task (no cross-task memory)

To add explicit chain-of-thought, extend `OpenAIAgent._chat()` or the system prompt — the harness stays the same.

## CLI

```bash
keyboard-agent run --config config.yaml
keyboard-agent run --project ... --max-turns 200 --current-only
keyboard-agent init-config config.yaml
keyboard-agent keys
```

## Human recording

Interactive human evaluation with the same bench lifecycle and run artifacts as the LLM agent.

**Turn rules**

| Input | Recorded as | Turn boundary |
|-------|-------------|---------------|
| Letters, digits, **all** US keyboard symbols (`!@#$%^&*()`, `?`, …) | `type` | Same burst if gap ≤ 1s; new turn after 1s idle |
| Other press keys (arrows, enter, ctrl+*, space, …) | `press` | One key = one turn |

Every keypress is logged to `keystrokes.jsonl` with timestamp, `key`, and `raw_bytes`
(hex). Input is captured from the terminal in raw mode and forwarded unchanged to tmux
(no tmux key-binding whitelist), so arbitrary combos (e.g. `ctrl+_` / `ctrl+/`, `?`) reach
the TUI and appear in the log. Mouse is disabled.

**Bash / Terminal UI (recommended)**

```bash
export BENCH_PROJECT=/path/to/anonymous-artifact
export BENCH=/path/to/anonymous-artifact

keyboard-agent human run -c config.human.example.yaml --task T01
```

The runner attaches to the task tmux session inside Bash, disables mouse input, and
records the same keystroke / turn / SVG artifacts as agent runs.

Submit with the default sequence: press **Ctrl-G** three times. The submit
sequence is consumed by the harness and is not forwarded to the TUI.

If the TUI exits on its own (e.g. pressing `q` in aptui), the harness detects the
dead pane, detaches, runs oracle verify, and advances to the next task.

Mouse is disabled via tmux click blockers and a one-time terminal sequence; use keyboard only.

**Browser UI (optional)**

```bash
keyboard-agent human run -c config.human.example.yaml --web --task T01
# → opens http://127.0.0.1:8765/
```

Click the terminal area, type with keyboard, then press **Ctrl-G** three times or click
**Submit task**.

```bash
keyboard-agent human run -p ... --web --port 8765 --no-browser
keyboard-agent human run -p ... --idle-ms 1000 --submit-key "ctrl+g ctrl+g ctrl+g"
```

Artifacts (same layout as agent runs, plus per-task keystroke log):

```
runs/<run_id>/
  meta.json                 # framework: human-record
  T01/
    initial_screen.json
    keystrokes.jsonl          # every keypress: seq, pressed_at, interval_ms, key
    transcript.jsonl          # one line per operation + submit
    result.json
    turns/turn_NNN.svg
```

Batch all `final_data` projects:

```bash
./run_all_final_data_human.sh
```

Batch the final Chinese benchmark for one participant:

```bash
keyboard-agent human run-cn --operator P001
```

Run ids and log folders include the operator id, e.g.
`runs/P001-P01-aptui-semantic-<timestamp>/`.

Logs are written **incrementally** (keystrokes, transcript, SVG per turn). Ctrl+C
keeps everything recorded so far; see `Txx/live.json` for in-progress status.

**Standalone kit for participants:** see [`../human-study-kit/`](../human-study-kit/README.md)
(self-contained `packages/` + `install.sh` + `run.sh`; zip the whole folder to ship).

After changing harness code, refresh the kit with:
`human-study-kit/sync-packages.sh`

This defaults to `/path/to/anonymous-artifact runs projects
from `P01-*` through `P24-*`, and uses the Bash/terminal interface. Submit each task
by pressing **Ctrl-G** three times. To resume midway:

```bash
keyboard-agent human run-cn --operator P001 --from-project P08-rura
```
