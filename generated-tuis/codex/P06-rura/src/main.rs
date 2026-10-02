use std::{
    cmp::min,
    collections::BTreeSet,
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthStr;

const PREVIEW_DELAY: Duration = Duration::from_millis(350);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Parser, Debug)]
#[command(
    name = "toolf",
    version,
    about = "Interactively build and debug shell pipelines"
)]
struct Args {
    /// Log file connected to the pipeline's standard input
    #[arg(short, long, default_value = "/bench/server.log")]
    file: PathBuf,

    /// Initial pipeline command
    #[arg(short, long, default_value = "tail -n 50")]
    command: String,
}

#[derive(Clone, Debug)]
struct Editor {
    text: String,
    cursor: usize, // character index
}

impl Editor {
    fn new(text: String) -> Self {
        let cursor = text.chars().count();
        Self { text, cursor }
    }

    fn byte_at(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    fn insert(&mut self, ch: char) {
        let byte = self.byte_at(self.cursor);
        self.text.insert(byte, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            let end = self.byte_at(self.cursor);
            let start = self.byte_at(self.cursor - 1);
            self.text.replace_range(start..end, "");
            self.cursor -= 1;
        }
    }

    fn delete(&mut self) {
        let start = self.byte_at(self.cursor);
        let end = self.byte_at(self.cursor + 1);
        if start < end {
            self.text.replace_range(start..end, "");
        }
    }

    fn move_word_left(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        while self.cursor > 0 && chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        while self.cursor > 0
            && !chars[self.cursor - 1].is_whitespace()
            && chars[self.cursor - 1] != '|'
        {
            self.cursor -= 1;
        }
    }

    fn move_word_right(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        while self.cursor < chars.len()
            && !chars[self.cursor].is_whitespace()
            && chars[self.cursor] != '|'
        {
            self.cursor += 1;
        }
        while self.cursor < chars.len() && chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
    }

    fn replace_chars(&mut self, start: usize, end: usize, value: &str) {
        let byte_start = self.byte_at(start);
        let byte_end = self.byte_at(end);
        self.text.replace_range(byte_start..byte_end, value);
        self.cursor = start + value.chars().count();
    }

    fn prefix(&self) -> String {
        let mut prefix = self.text[..self.byte_at(self.cursor)].trim().to_owned();
        while prefix.ends_with('|') {
            prefix.pop();
            prefix = prefix.trim_end().to_owned();
        }
        prefix
    }
}

#[derive(Clone, Debug)]
struct Completion {
    start: usize,
    end: usize,
    candidates: Vec<String>,
    index: usize,
}

#[derive(Debug)]
struct RunResult {
    generation: u64,
    command: String,
    scope: &'static str,
    output: String,
    success: bool,
    exit_code: Option<i32>,
    elapsed: Duration,
    timed_out: bool,
}

#[derive(Debug, PartialEq)]
enum InputMode {
    Edit,
    Save,
}

struct App {
    file: PathBuf,
    editor: Editor,
    mode: InputMode,
    output: String,
    output_scroll: u16,
    status: String,
    result_summary: String,
    save_path: Editor,
    help: bool,
    completion: Option<Completion>,
    dirty_since: Option<Instant>,
    generation: u64,
    latest_requested: u64,
    running: bool,
    quit: bool,
    tx: Sender<RunResult>,
    rx: Receiver<RunResult>,
}

impl App {
    fn new(args: Args) -> Self {
        let (tx, rx) = mpsc::channel();
        let save_default = default_save_path();
        Self {
            file: args.file,
            editor: Editor::new(args.command),
            mode: InputMode::Edit,
            output: "Waiting for first preview…".to_string(),
            output_scroll: 0,
            status: "Ready — the log file is connected to pipeline stdin".to_string(),
            result_summary: "not run".to_string(),
            save_path: Editor::new(save_default),
            help: false,
            completion: None,
            dirty_since: Some(Instant::now() - PREVIEW_DELAY),
            generation: 0,
            latest_requested: 0,
            running: false,
            quit: false,
            tx,
            rx,
        }
    }

    fn changed(&mut self) {
        self.completion = None;
        self.dirty_since = Some(Instant::now());
        self.status = "Edited — preview pending…".to_string();
    }

    fn run(&mut self, command: String, scope: &'static str) {
        self.generation += 1;
        let generation = self.generation;
        self.latest_requested = generation;
        self.running = true;
        self.dirty_since = None;
        self.status = format!("Running {scope}…");
        self.result_summary = format!("{scope} • running");
        let tx = self.tx.clone();
        let file = self.file.clone();
        thread::spawn(move || {
            let result = execute_pipeline(generation, command, scope, &file);
            let _ = tx.send(result);
        });
    }

    fn run_full(&mut self, scope: &'static str) {
        self.run(self.editor.text.trim().to_string(), scope);
    }

    fn run_prefix(&mut self) {
        let prefix = self.editor.prefix();
        if prefix.is_empty() {
            self.status = "Nothing before the cursor to execute".to_string();
        } else {
            self.run(prefix, "prefix");
        }
    }

    fn receive_results(&mut self) {
        while let Ok(result) = self.rx.try_recv() {
            if result.generation != self.latest_requested {
                continue;
            }
            self.running = false;
            self.output = result.output;
            self.output_scroll = 0;
            let code = result
                .exit_code
                .map(|v| v.to_string())
                .unwrap_or_else(|| "signal".to_string());
            self.result_summary = format!(
                "{} • {} • exit {} • {} ms",
                result.scope,
                if result.success { "success" } else { "failed" },
                code,
                result.elapsed.as_millis()
            );
            self.status = if result.timed_out {
                "Command stopped after the 5 second safety timeout".to_string()
            } else {
                format!(
                    "Finished {}: {}",
                    result.scope,
                    compact_command(&result.command)
                )
            };
        }
    }

    fn maybe_preview(&mut self) {
        if self.mode == InputMode::Edit
            && !self.help
            && self
                .dirty_since
                .is_some_and(|at| at.elapsed() >= PREVIEW_DELAY)
        {
            self.run_full("preview");
        }
    }

    fn save(&mut self) {
        let path = expand_tilde(self.save_path.text.trim());
        if path.as_os_str().is_empty() {
            self.status = "Save path cannot be empty".to_string();
            return;
        }
        match fs::write(&path, self.output.as_bytes()) {
            Ok(_) => {
                self.status = format!("Saved {} bytes to {}", self.output.len(), path.display());
                self.mode = InputMode::Edit;
            }
            Err(err) => self.status = format!("Save failed: {err}"),
        }
    }

    fn complete(&mut self, backwards: bool) {
        if let Some(mut completion) = self.completion.take() {
            if completion.candidates.is_empty() {
                return;
            }
            completion.index = if backwards {
                (completion.index + completion.candidates.len() - 1) % completion.candidates.len()
            } else {
                (completion.index + 1) % completion.candidates.len()
            };
            let value = completion.candidates[completion.index].clone();
            self.editor
                .replace_chars(completion.start, completion.end, &value);
            completion.end = self.editor.cursor;
            self.status = format!(
                "Completion {}/{}: {}",
                completion.index + 1,
                completion.candidates.len(),
                value
            );
            self.completion = Some(completion);
            self.dirty_since = Some(Instant::now());
            return;
        }

        let (start, end, token, command_position) = completion_context(&self.editor);
        let candidates = completion_candidates(&token, command_position);
        if candidates.is_empty() {
            self.status = format!("No completion found for “{token}”");
            return;
        }
        let index = if backwards { candidates.len() - 1 } else { 0 };
        let value = candidates[index].clone();
        self.editor.replace_chars(start, end, &value);
        self.status = format!("Completion 1/{}: {}", candidates.len(), value);
        self.completion = Some(Completion {
            start,
            end: self.editor.cursor,
            candidates,
            index,
        });
        self.dirty_since = Some(Instant::now());
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, App::new(args));

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app: App) -> io::Result<()> {
    loop {
        app.receive_results();
        app.maybe_preview();
        terminal.draw(|frame| draw(frame, &app))?;

        if app.quit {
            return Ok(());
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(&mut app, key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.quit = true;
        return;
    }
    if key.code == KeyCode::F(1) {
        app.help = !app.help;
        app.completion = None;
        return;
    }
    if app.help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
            app.help = false;
        }
        return;
    }

    if app.mode == InputMode::Save {
        handle_save_key(app, key);
        return;
    }

    if (key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('\\'))
        || key.code == KeyCode::F(6)
    {
        app.completion = None;
        app.run_prefix();
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        app.completion = None;
        app.mode = InputMode::Save;
        app.status = "Enter destination; Enter saves, Esc cancels".to_string();
        return;
    }
    if (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r'))
        || (key.code == KeyCode::Enter && key.modifiers.is_empty())
    {
        app.completion = None;
        app.run_full("full");
        return;
    }

    match key.code {
        KeyCode::Tab => app.complete(false),
        KeyCode::BackTab => app.complete(true),
        KeyCode::Esc => {
            app.completion = None;
            app.status = "Completion closed".to_string();
        }
        KeyCode::PageUp => app.output_scroll = app.output_scroll.saturating_sub(10),
        KeyCode::PageDown => app.output_scroll = app.output_scroll.saturating_add(10),
        KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
            app.output_scroll = app.output_scroll.saturating_sub(1)
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
            app.output_scroll = app.output_scroll.saturating_add(1)
        }
        KeyCode::Left => {
            app.completion = None;
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.editor.move_word_left();
            } else {
                app.editor.cursor = app.editor.cursor.saturating_sub(1);
            }
        }
        KeyCode::Right => {
            app.completion = None;
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.editor.move_word_right();
            } else {
                app.editor.cursor = min(app.editor.cursor + 1, app.editor.text.chars().count());
            }
        }
        KeyCode::Home => {
            app.completion = None;
            app.editor.cursor = 0;
        }
        KeyCode::End => {
            app.completion = None;
            app.editor.cursor = app.editor.text.chars().count();
        }
        KeyCode::Backspace => {
            app.editor.backspace();
            app.changed();
        }
        KeyCode::Delete => {
            app.editor.delete();
            app.changed();
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.completion = None;
            app.editor.cursor = 0;
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.completion = None;
            app.editor.cursor = app.editor.text.chars().count();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.editor.text.clear();
            app.editor.cursor = 0;
            app.changed();
        }
        KeyCode::Char(ch)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            app.editor.insert(ch);
            app.changed();
        }
        _ => {}
    }
}

fn handle_save_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = InputMode::Edit;
            app.status = "Save cancelled".to_string();
        }
        KeyCode::Enter => app.save(),
        KeyCode::Left => app.save_path.cursor = app.save_path.cursor.saturating_sub(1),
        KeyCode::Right => {
            app.save_path.cursor = min(app.save_path.cursor + 1, app.save_path.text.chars().count())
        }
        KeyCode::Home => app.save_path.cursor = 0,
        KeyCode::End => app.save_path.cursor = app.save_path.text.chars().count(),
        KeyCode::Backspace => app.save_path.backspace(),
        KeyCode::Delete => app.save_path.delete(),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.save_path.text.clear();
            app.save_path.cursor = 0;
        }
        KeyCode::Char(ch)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            app.save_path.insert(ch)
        }
        _ => {}
    }
}

fn execute_pipeline(
    generation: u64,
    command: String,
    scope: &'static str,
    file: &Path,
) -> RunResult {
    let started = Instant::now();
    if command.trim().is_empty() {
        return RunResult {
            generation,
            command,
            scope,
            output: "Pipeline is empty. Type a command such as: grep ERROR | tail -n 20"
                .to_string(),
            success: false,
            exit_code: Some(2),
            elapsed: started.elapsed(),
            timed_out: false,
        };
    }

    let input = match File::open(file) {
        Ok(file) => file,
        Err(err) => {
            return RunResult {
                generation,
                command,
                scope,
                output: format!("Cannot open input file {}: {err}", file.display()),
                success: false,
                exit_code: Some(1),
                elapsed: started.elapsed(),
                timed_out: false,
            }
        }
    };

    let (mut stdout_file, stdout_path) = match capture_file(generation, "stdout") {
        Ok(value) => value,
        Err(err) => return launch_error(generation, command, scope, started, err),
    };
    let (mut stderr_file, stderr_path) = match capture_file(generation, "stderr") {
        Ok(value) => value,
        Err(err) => {
            let _ = fs::remove_file(stdout_path);
            return launch_error(generation, command, scope, started, err);
        }
    };
    let child_stdout = match stdout_file.try_clone() {
        Ok(file) => file,
        Err(err) => return launch_error(generation, command, scope, started, err),
    };
    let child_stderr = match stderr_file.try_clone() {
        Ok(file) => file,
        Err(err) => return launch_error(generation, command, scope, started, err),
    };

    let mut process = Command::new("bash");
    process
        .arg("-o")
        .arg("pipefail")
        .arg("-c")
        .arg(&command)
        .env("TOOLF_FILE", file)
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr));
    // Put the shell and every process in its pipeline into a separate process
    // group so the timeout can stop the complete pipeline, not only bash.
    unsafe {
        process.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let spawn = process.spawn();

    let mut child = match spawn {
        Ok(child) => child,
        Err(err) => {
            let _ = fs::remove_file(stdout_path);
            let _ = fs::remove_file(stderr_path);
            return launch_error(generation, command, scope, started, err);
        }
    };

    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= COMMAND_TIMEOUT => {
                timed_out = true;
                // A negative pid addresses the process group created above.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break None;
            }
        }
    };

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let _ = stdout_file.seek(SeekFrom::Start(0));
    let _ = stderr_file.seek(SeekFrom::Start(0));
    let _ = stdout_file.read_to_end(&mut stdout);
    let _ = stderr_file.read_to_end(&mut stderr);
    drop(stdout_file);
    drop(stderr_file);
    let _ = fs::remove_file(stdout_path);
    let _ = fs::remove_file(stderr_path);

    let mut output = String::from_utf8_lossy(&stdout).into_owned();
    if !stderr.is_empty() {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str("── stderr ──\n");
        output.push_str(&String::from_utf8_lossy(&stderr));
    }
    if timed_out {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str("── toolf: command timed out after 5 seconds ──");
    }
    if output.is_empty() {
        output = "(command produced no output)".to_string();
    }
    if output.len() > MAX_OUTPUT_BYTES {
        let mut cut = MAX_OUTPUT_BYTES;
        while !output.is_char_boundary(cut) {
            cut -= 1;
        }
        output.truncate(cut);
        output.push_str("\n── toolf: output truncated at 2 MiB ──");
    }

    let exit_code = status.as_ref().and_then(|s| s.code());
    let success = status.is_some_and(|s| s.success()) && !timed_out;
    RunResult {
        generation,
        command,
        scope,
        output,
        success,
        exit_code,
        elapsed: started.elapsed(),
        timed_out,
    }
}

fn capture_file(generation: u64, stream: &str) -> io::Result<(File, PathBuf)> {
    let path = env::temp_dir().join(format!(
        "toolf-{}-{generation}-{stream}.tmp",
        std::process::id()
    ));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    Ok((file, path))
}

fn launch_error(
    generation: u64,
    command: String,
    scope: &'static str,
    started: Instant,
    err: io::Error,
) -> RunResult {
    RunResult {
        generation,
        command,
        scope,
        output: format!("Failed to launch bash: {err}"),
        success: false,
        exit_code: None,
        elapsed: started.elapsed(),
        timed_out: false,
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let completion_height = if app.completion.is_some() { 5 } else { 0 };
    let help_height = if app.help {
        min(12, area.height / 2)
    } else {
        0
    };
    let save_height = if app.mode == InputMode::Save { 3 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(completion_height),
            Constraint::Min(5),
            Constraint::Length(help_height),
            Constraint::Length(save_height),
            Constraint::Length(2),
        ])
        .split(area);

    draw_editor(frame, chunks[0], app);
    if completion_height > 0 {
        draw_completions(frame, chunks[1], app);
    }
    draw_output(frame, chunks[2], app);
    if help_height > 0 {
        draw_help(frame, chunks[3]);
    }
    if save_height > 0 {
        draw_save(frame, chunks[4], app);
    }
    draw_footer(frame, chunks[5], app);
}

fn draw_editor(frame: &mut Frame, area: Rect, app: &App) {
    let inner_width = area.width.saturating_sub(2) as usize;
    let cursor_width =
        UnicodeWidthStr::width(&app.editor.text[..app.editor.byte_at(app.editor.cursor)]);
    let scroll = cursor_width.saturating_sub(inner_width.saturating_sub(1)) as u16;
    let title = format!(" Pipeline — stdin: {} ", app.file.display());
    let paragraph = Paragraph::new(highlight_shell(&app.editor.text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title)
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .scroll((0, scroll));
    frame.render_widget(paragraph, area);
    if app.mode == InputMode::Edit && !app.help {
        let x = area.x + 1 + cursor_width.saturating_sub(scroll as usize) as u16;
        frame.set_cursor_position((min(x, area.right().saturating_sub(2)), area.y + 1));
    }
}

fn draw_completions(frame: &mut Frame, area: Rect, app: &App) {
    let Some(completion) = &app.completion else {
        return;
    };
    let items: Vec<ListItem> = completion
        .candidates
        .iter()
        .enumerate()
        .take(20)
        .map(|(i, item)| {
            let style = if i == completion.index {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(format!(" {} ", item)).style(style)
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Completions — Tab next • Shift-Tab previous • Esc close "),
        )
        .direction(ratatui::widgets::ListDirection::TopToBottom);
    frame.render_widget(list, area);
}

fn draw_output(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(
        " Output — {}{} ",
        app.result_summary,
        if app.running { " ⟳" } else { "" }
    );
    let output = Paragraph::new(app.output.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(Style::default().fg(if app.running {
                    Color::Yellow
                } else {
                    Color::Green
                })),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.output_scroll, 0));
    frame.render_widget(output, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help = vec![
        Line::from(vec![
            Span::styled("Enter / Ctrl-R", key_style()),
            Span::raw(" run full     "),
            Span::styled("Alt-\\ / F6", key_style()),
            Span::raw(" run prefix before cursor"),
        ]),
        Line::from(vec![
            Span::styled("Tab / Shift-Tab", key_style()),
            Span::raw(" complete commands/paths     "),
            Span::styled("Ctrl-S", key_style()),
            Span::raw(" save output"),
        ]),
        Line::from(vec![
            Span::styled("←/→ Home/End", key_style()),
            Span::raw(" move cursor     "),
            Span::styled("Ctrl-←/→", key_style()),
            Span::raw(" move by word     "),
            Span::styled("Ctrl-U", key_style()),
            Span::raw(" clear"),
        ]),
        Line::from(vec![
            Span::styled("PgUp/PgDn", key_style()),
            Span::raw(" scroll output     "),
            Span::styled("Alt-↑/↓", key_style()),
            Span::raw(" fine scroll     "),
            Span::styled("Ctrl-C", key_style()),
            Span::raw(" quit"),
        ]),
        Line::from("The selected log file is stdin. $TOOLF_FILE also contains its path."),
        Line::from("Edits preview after 350 ms. Shell: bash + pipefail. Timeout: 5 seconds."),
        Line::from(Span::styled(
            "F1, Esc, or Enter closes help",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(
        Paragraph::new(help).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title(" Help "),
        ),
        area,
    );
}

fn draw_save(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Clear, area);
    let paragraph = Paragraph::new(app.save_path.text.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Save output path — Enter save • Esc cancel "),
    );
    frame.render_widget(paragraph, area);
    let width =
        UnicodeWidthStr::width(&app.save_path.text[..app.save_path.byte_at(app.save_path.cursor)])
            as u16;
    frame.set_cursor_position((
        min(area.x + 1 + width, area.right().saturating_sub(2)),
        area.y + 1,
    ));
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let line1 = Line::from(vec![
        Span::styled(" Enter", key_style()),
        Span::raw(" Run  "),
        Span::styled("Alt-\\", key_style()),
        Span::raw(" Prefix  "),
        Span::styled("Tab", key_style()),
        Span::raw(" Complete  "),
        Span::styled("Ctrl-S", key_style()),
        Span::raw(" Save  "),
        Span::styled("F1", key_style()),
        Span::raw(" Help  "),
        Span::styled("Ctrl-C", key_style()),
        Span::raw(" Quit"),
    ]);
    let line2 = Line::from(vec![
        Span::styled(" Status: ", Style::default().fg(Color::DarkGray)),
        Span::raw(&app.status),
    ]);
    frame.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn key_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn highlight_shell(input: &str) -> Line<'static> {
    let chars: Vec<char> = input.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0;
    let mut command_position = true;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            let start = i;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            spans.push(Span::raw(chars[start..i].iter().collect::<String>()));
        } else if chars[i] == '|' {
            spans.push(Span::styled(
                "|",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
            i += 1;
            command_position = true;
        } else if matches!(chars[i], '\'' | '"') {
            let quote = chars[i];
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == quote && (i == 0 || chars[i - 1] != '\\') {
                    i += 1;
                    break;
                }
                i += 1;
            }
            spans.push(Span::styled(
                chars[start..i].iter().collect::<String>(),
                Style::default().fg(Color::Yellow),
            ));
            command_position = false;
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '|' {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let style = if command_position {
                command_position = false;
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if word.starts_with('-') {
                Style::default().fg(Color::Green)
            } else if word.starts_with('$') {
                Style::default().fg(Color::LightMagenta)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(word, style));
        }
    }
    Line::from(spans)
}

fn completion_context(editor: &Editor) -> (usize, usize, String, bool) {
    let chars: Vec<char> = editor.text.chars().collect();
    let mut start = editor.cursor;
    while start > 0
        && !chars[start - 1].is_whitespace()
        && !matches!(chars[start - 1], '|' | ';' | '&')
    {
        start -= 1;
    }
    let mut end = editor.cursor;
    while end < chars.len() && !chars[end].is_whitespace() && !matches!(chars[end], '|' | ';' | '&')
    {
        end += 1;
    }
    let token: String = chars[start..editor.cursor].iter().collect();
    let before: String = chars[..start].iter().collect();
    let command_position = before.trim().is_empty()
        || before
            .trim_end()
            .chars()
            .last()
            .is_some_and(|c| matches!(c, '|' | ';' | '&'));
    (start, end, token, command_position)
}

fn completion_candidates(token: &str, command_position: bool) -> Vec<String> {
    if command_position && !token.contains('/') && !token.starts_with('.') {
        command_candidates(token)
    } else {
        path_candidates(token)
    }
}

fn command_candidates(prefix: &str) -> Vec<String> {
    let mut found = BTreeSet::new();
    for dir in env::var_os("PATH")
        .unwrap_or_default()
        .to_string_lossy()
        .split(':')
    {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(prefix) && entry.path().is_file() {
                    found.insert(format!("{name} "));
                }
            }
        }
    }
    found.into_iter().take(100).collect()
}

fn path_candidates(token: &str) -> Vec<String> {
    let expanded = expand_tilde(token);
    let (dir, prefix) = if token.ends_with('/') {
        (expanded.clone(), String::new())
    } else {
        (
            expanded
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
            expanded
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        )
    };
    let display_parent = token
        .rfind('/')
        .map(|i| &token[..=i])
        .unwrap_or_default()
        .to_string();
    let mut found = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) {
                let mut value = format!("{display_parent}{name}");
                if entry.path().is_dir() {
                    value.push('/');
                } else {
                    value.push(' ');
                }
                found.push(value);
            }
        }
    }
    found.sort();
    found.truncate(100);
    found
}

fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(value))
    } else if let Some(rest) = value.strip_prefix("~/") {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(rest)
    } else {
        PathBuf::from(value)
    }
}

fn default_save_path() -> String {
    if Path::new("/bench/data").is_dir() {
        "/bench/data/result.txt".to_string()
    } else {
        "result.txt".to_string()
    }
}

fn compact_command(command: &str) -> String {
    const LIMIT: usize = 70;
    if command.chars().count() <= LIMIT {
        command.to_string()
    } else {
        format!("{}…", command.chars().take(LIMIT).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_handles_unicode() {
        let mut editor = Editor::new("abEnglish-only text".to_string());
        editor.cursor = 2;
        editor.insert('X');
        assert_eq!(editor.text, "abXEnglish-only text");
        editor.backspace();
        assert_eq!(editor.text, "abEnglish-only text");
    }

    #[test]
    fn prefix_removes_dangling_pipe() {
        let mut editor = Editor::new("grep ERROR | tail -n 2".to_string());
        editor.cursor = "grep ERROR |".chars().count();
        assert_eq!(editor.prefix(), "grep ERROR");
    }

    #[test]
    fn completion_detects_command_position() {
        let editor = Editor::new("grep x | ta".to_string());
        let (_, _, token, command) = completion_context(&editor);
        assert_eq!(token, "ta");
        assert!(command);
    }

    #[test]
    fn pipeline_reads_real_stdin_file() {
        let path = env::temp_dir().join(format!("toolf-test-{}", std::process::id()));
        fs::write(&path, "ok\nerror\nok\n").unwrap();
        let result = execute_pipeline(1, "grep ok | wc -l".into(), "test", &path);
        fs::remove_file(path).unwrap();
        assert!(result.success);
        assert_eq!(result.output.trim(), "2");
    }

    #[test]
    fn timeout_stops_whole_pipeline_group() {
        let path = env::temp_dir().join(format!("toolf-timeout-test-{}", std::process::id()));
        fs::write(&path, "").unwrap();
        let started = Instant::now();
        let result = execute_pipeline(2, "sleep 30 | sleep 30".into(), "timeout-test", &path);
        fs::remove_file(path).unwrap();
        assert!(result.timed_out);
        assert!(started.elapsed() < Duration::from_secs(8));
    }
}
