//! In-app help text, shown with F1 or `?`.

pub const HELP_TEXT: &str = r#"toolf — Shell Pipeline Debugger
================================

CORE KEYS
  Enter         Run the complete pipeline
  Alt+\         Run the pipeline up to the segment under the cursor (▶)
  Ctrl+O        Alternate key for partial execution (same as Alt+\)
  Ctrl+P        Alternate key for partial execution (same as Alt+\)
  Tab           Complete a command name or file path (Tab again to cycle)
  Shift+Tab     Cycle completions
  Ctrl+S        Save the current output to a file
  Ctrl+C        Quit toolf
  Ctrl+Q        Quit toolf
  F1  /  ?      Show or hide this help
  Esc           Close help / cancel the save prompt

EDITING THE COMMAND LINE
  Left/Right    Move the cursor
  Home / End    Jump to start / end of the line (also Ctrl+A / Ctrl+E)
  Backspace     Delete the character before the cursor
  Delete        Delete the character under the cursor
  Ctrl+W        Delete the word before the cursor
  Ctrl+U        Clear the whole line
  Ctrl+K        Delete from the cursor to the end
  Up / Down     Browse command history

OUTPUT AREA
  PgUp / PgDn   Scroll the output up / down
  Ctrl+L        Clear the output area

SAVING OUTPUT
  Press Ctrl+S, type a path (e.g. /bench/data/result.txt), then press
  Enter to write the file, or Esc to cancel. Parent directories are
  created automatically.

PIPELINE PREVIEW
  The preview panel shows how the current command is split into stages.
  The ▶ marker shows the stage that Alt+\ will execute up to. Move the
  cursor across a pipe boundary to change which prefix runs.

TIPS
  * Use cat <file> | grep PATTERN | sort | uniq -c | tail -N to explore logs.
  * Press Tab after a pipe to complete command names, and Tab after a
    space to complete file paths.
  * The cursor and the output area are always visible on the same screen.
"#;
