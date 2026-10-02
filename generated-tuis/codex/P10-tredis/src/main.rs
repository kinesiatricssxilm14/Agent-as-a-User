mod app;
mod redis_client;
mod ui;
use app::{App, Focus, FormStage, InputMode, KeyType, Resource};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, panic, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, event::EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            event::DisableBracketedPaste
        );
        old_hook(info);
    }));
    let result = run(&mut terminal);
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableBracketedPaste
    )?;
    terminal.show_cursor()?;
    result.map_err(Into::into)
}
fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();
    loop {
        terminal.draw(|f| ui::draw(f, &app))?;
        if app.should_quit {
            return Ok(());
        }
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(k) if k.kind == event::KeyEventKind::Press => handle_key(&mut app, k),
                Event::Paste(s) => {
                    if !matches!(
                        app.mode,
                        InputMode::Normal
                            | InputMode::DeleteConfirm { .. }
                            | InputMode::Create {
                                stage: FormStage::Type,
                                ..
                            }
                    ) {
                        for c in s.chars() {
                            app.insert_char(c)
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
fn handle_key(app: &mut App, key: KeyEvent) {
    if app.show_help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc => app.show_help = false,
            _ => {}
        }
        return;
    }
    match app.mode.clone() {
        InputMode::Normal => normal_key(app, key),
        InputMode::DeleteConfirm { .. } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => app.confirm_delete(true),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.confirm_delete(false),
            _ => {}
        },
        InputMode::Create {
            stage: FormStage::Type,
            ..
        } => match key.code {
            KeyCode::Left | KeyCode::Char('h') => app.create_cycle(true),
            KeyCode::Right | KeyCode::Char('l') => app.create_cycle(false),
            KeyCode::Enter => app.create_accept_type(),
            KeyCode::Esc => app.cancel_input(),
            _ => {}
        },
        _ => input_key(app, key),
    }
}
fn input_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('a') => app.cursor = 0,
            KeyCode::Char('e') => app.cursor = app.input.len(),
            KeyCode::Char('u') => {
                app.input.clear();
                app.cursor = 0
            }
            KeyCode::Char('c') => app.cancel_input(),
            _ => {}
        }
        return;
    }
    match key.code {
        KeyCode::Esc => app.cancel_input(),
        KeyCode::Enter => app.submit_input(),
        KeyCode::Backspace => app.backspace(),
        KeyCode::Delete => app.delete_char(),
        KeyCode::Left => app.cursor_left(),
        KeyCode::Right => app.cursor_right(),
        KeyCode::Home => app.cursor = 0,
        KeyCode::End => app.cursor = app.input.len(),
        KeyCode::Char(c) => app.insert_char(c),
        _ => {}
    }
}
fn normal_key(app: &mut App, key: KeyEvent) {
    if !app.is_connected() {
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Up | KeyCode::Char('k') => {
                app.server_selected = app.server_selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.server_selected + 1 < app.servers.len() {
                    app.server_selected += 1
                }
            }
            KeyCode::Enter => app.connect_selected(),
            KeyCode::Char('n') => app.begin_input(InputMode::AddServerName, String::new()),
            _ => {}
        }
        return;
    }
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('1') => app.set_resource(Resource::Keys),
        KeyCode::Char('2') => app.set_resource(Resource::Streams),
        KeyCode::Char('3') => app.set_resource(Resource::PubSub),
        KeyCode::Char('4') => app.set_resource(Resource::Acl),
        KeyCode::Char(':') => app.begin_input(InputMode::Command, String::new()),
        KeyCode::Char('/') => app.begin_input(
            InputMode::Search {
                before: app.search.clone(),
            },
            app.search.clone(),
        ),
        KeyCode::Char('r') => app.refresh(),
        KeyCode::Char('c') => app.disconnect(),
        KeyCode::Left | KeyCode::Char('h') => app.focus = Focus::List,
        KeyCode::Right | KeyCode::Char('l') => app.focus = Focus::Detail,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.focus == Focus::List {
                app.move_selection(-1)
            } else {
                app.detail_scroll = app.detail_scroll.saturating_sub(1)
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.focus == Focus::List {
                app.move_selection(1)
            } else {
                app.detail_scroll = app.detail_scroll.saturating_add(1)
            }
        }
        KeyCode::PageUp => {
            if app.focus == Focus::List {
                app.move_selection(-10)
            } else {
                app.detail_scroll = app.detail_scroll.saturating_sub(10)
            }
        }
        KeyCode::PageDown => {
            if app.focus == Focus::List {
                app.move_selection(10)
            } else {
                app.detail_scroll = app.detail_scroll.saturating_add(10)
            }
        }
        KeyCode::Char('t') if app.resource == Resource::Keys => app.cycle_type(),
        KeyCode::Char('n') if matches!(app.resource, Resource::Keys | Resource::Streams) => {
            if app.resource == Resource::Streams {
                app.begin_input(
                    InputMode::Create {
                        stage: FormStage::Key,
                        kind: KeyType::Stream,
                        key: String::new(),
                    },
                    String::new(),
                );
            } else {
                app.mode = InputMode::Create {
                    stage: FormStage::Type,
                    kind: KeyType::String,
                    key: String::new(),
                };
            }
        }
        KeyCode::Char('e') if matches!(app.resource, Resource::Keys | Resource::Streams) => {
            app.begin_edit()
        }
        KeyCode::Char('d') if matches!(app.resource, Resource::Keys | Resource::Streams) => {
            app.delete_selected()
        }
        _ => {}
    }
}
