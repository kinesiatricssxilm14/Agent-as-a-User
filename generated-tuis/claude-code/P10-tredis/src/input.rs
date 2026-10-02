//! Key event dispatch. Prompts and confirmations swallow input first; otherwise
//! keys are routed to the global bindings and then to the per-view bindings.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::actions::{ask, jump_to_key, open_prompt, run_confirm, submit_prompt};
use crate::app::{
    App, ConfirmAction, Cursor, Focus, PromptKind, View,
};
use crate::db::{KeyData, KeyKind};
use crate::util::fmt_score;

/// How many rows a PageUp/PageDown jumps. The renderer stores the real pane
/// height each frame; this is the fallback for the first frame.
pub const PAGE: isize = 10;

pub fn handle_key(app: &mut App, key: KeyEvent, page: isize) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Ctrl-C always exits, except while a prompt is open (there it cancels).
    if ctrl && matches!(key.code, KeyCode::Char('c')) {
        if app.prompt.is_some() {
            app.prompt = None;
            app.info_msg("Cancelled");
        } else if app.confirm.is_some() {
            app.confirm = None;
            app.info_msg("Cancelled");
        } else {
            app.should_quit = true;
        }
        return;
    }

    if app.confirm.is_some() {
        handle_confirm(app, key);
        return;
    }
    if app.prompt.is_some() {
        handle_prompt(app, key, ctrl);
        return;
    }
    handle_normal(app, key, ctrl, page);
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            if let Some(c) = app.confirm.take() {
                run_confirm(app, c.action);
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.confirm = None;
            app.info_msg("Cancelled");
        }
        _ => {}
    }
}

fn handle_prompt(app: &mut App, key: KeyEvent, ctrl: bool) {
    let is_command = matches!(
        app.prompt.as_ref().map(|p| p.kind.clone()),
        Some(PromptKind::Command)
    );
    let is_search = matches!(
        app.prompt.as_ref().map(|p| p.kind.clone()),
        Some(PromptKind::Search)
    );

    match key.code {
        KeyCode::Esc => {
            app.prompt = None;
            app.history_pos = None;
            app.info_msg("Cancelled");
            return;
        }
        KeyCode::Enter => {
            if let Some(p) = app.prompt.take() {
                app.history_pos = None;
                submit_prompt(app, p);
            }
            return;
        }
        KeyCode::Up if is_command => {
            recall_history(app, -1);
            return;
        }
        KeyCode::Down if is_command => {
            recall_history(app, 1);
            return;
        }
        _ => {}
    }

    let Some(p) = app.prompt.as_mut() else { return };
    match key.code {
        KeyCode::Backspace => p.backspace(),
        KeyCode::Delete => p.delete(),
        KeyCode::Left => p.left(),
        KeyCode::Right => p.right(),
        KeyCode::Home => p.home(),
        KeyCode::End => p.end(),
        KeyCode::Char('w') if ctrl => p.kill_word(),
        KeyCode::Char('u') if ctrl => p.kill_to_start(),
        KeyCode::Char('k') if ctrl => p.kill_to_end(),
        KeyCode::Char('a') if ctrl => p.home(),
        KeyCode::Char('e') if ctrl => p.end(),
        KeyCode::Char(c) => p.insert(c),
        _ => {}
    }

    // `/` filters as you type so results update live.
    if is_search {
        let text = app.prompt.as_ref().map(|p| p.input.clone()).unwrap_or_default();
        app.filter = text;
        app.on_filter_changed();
    }
}

fn recall_history(app: &mut App, delta: isize) {
    if app.history.is_empty() {
        return;
    }
    let len = app.history.len();
    let next = match app.history_pos {
        None if delta < 0 => Some(len - 1),
        None => None,
        Some(i) => {
            let n = i as isize + delta;
            if n < 0 {
                Some(0)
            } else if n >= len as isize {
                None
            } else {
                Some(n as usize)
            }
        }
    };
    app.history_pos = next;
    let text = next.map(|i| app.history[i].clone()).unwrap_or_default();
    if let Some(p) = app.prompt.as_mut() {
        p.input = text;
        p.end();
    }
}

fn handle_normal(app: &mut App, key: KeyEvent, ctrl: bool, page: isize) {
    let page = if page > 0 { page } else { PAGE };

    // ---- global bindings
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char('?') | KeyCode::F(1) => {
            if app.view == View::Help {
                let back = app.prev_view;
                app.goto(back);
            } else {
                app.goto(View::Help);
            }
            return;
        }
        KeyCode::Char(':') => {
            open_prompt(
                app,
                PromptKind::Command,
                ":",
                "resource or command — try keys, streams, pubsub, acl, connect, cmd",
                "",
            );
            return;
        }
        KeyCode::Char('/') => {
            let cur = app.filter.clone();
            open_prompt(
                app,
                PromptKind::Search,
                "/",
                "filter by name — substring or glob (user:*)",
                &cur,
            );
            return;
        }
        KeyCode::Char('r') | KeyCode::F(5) => {
            app.refresh_current();
            app.info_msg(format!("Reloaded :{}", app.view.slug()));
            return;
        }
        KeyCode::Tab => {
            app.next_tab(1);
            return;
        }
        KeyCode::BackTab => {
            app.next_tab(-1);
            return;
        }
        KeyCode::Char(c @ '1'..='6') => {
            let idx = c as usize - '1' as usize;
            app.goto(View::TABS[idx]);
            return;
        }
        KeyCode::Esc => {
            if app.view == View::Help {
                let back = app.prev_view;
                app.goto(back);
            } else if !app.filter.is_empty() {
                app.filter.clear();
                app.on_filter_changed();
                app.info_msg("Filter cleared");
            } else if app.focus == Focus::Detail {
                app.focus = Focus::List;
            } else if app.type_filter.is_some() {
                app.type_filter = None;
                app.on_filter_changed();
                app.info_msg("Type filter cleared");
            }
            return;
        }
        _ => {}
    }

    // ---- navigation shared by every view
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.move_cursor(1);
            return;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_cursor(-1);
            return;
        }
        KeyCode::PageDown => {
            app.move_cursor(page);
            return;
        }
        KeyCode::PageUp => {
            app.move_cursor(-page);
            return;
        }
        KeyCode::Char('d') if ctrl => {
            app.move_cursor(page / 2);
            return;
        }
        KeyCode::Char('u') if ctrl => {
            app.move_cursor(-page / 2);
            return;
        }
        KeyCode::Home | KeyCode::Char('g') => {
            app.cursor_to(0);
            return;
        }
        KeyCode::End | KeyCode::Char('G') => {
            let len = app.active_cursor_len();
            app.cursor_to(len.saturating_sub(1));
            return;
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.focus = Focus::List;
            return;
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if has_detail_pane(app) {
                focus_detail(app);
            }
            return;
        }
        _ => {}
    }

    // ---- per-view bindings
    match app.view {
        View::Keys => keys_view(app, key),
        View::Streams => streams_view(app, key),
        View::PubSub => pubsub_view(app, key),
        View::Acl => acl_view(app, key),
        View::Servers => servers_view(app, key),
        View::Info | View::Help => {}
    }
}

fn has_detail_pane(app: &App) -> bool {
    matches!(
        app.view,
        View::Keys | View::Streams | View::PubSub | View::Acl
    )
}

fn focus_detail(app: &mut App) {
    if app.active_detail_len() == 0 {
        return;
    }
    app.focus = Focus::Detail;
}

impl App {
    /// Row count of the detail pane for the current view.
    fn active_detail_len(&self) -> usize {
        match self.view {
            View::Keys | View::Streams => self.detail_len(),
            View::PubSub => self.sub.message_count(),
            View::Acl => self.selected_acl().map(|u| u.attrs.len()).unwrap_or(0),
            _ => 0,
        }
    }
}

fn keys_view(app: &mut App, key: KeyEvent) {
    let selected = app.selected_key_name();
    match key.code {
        KeyCode::Enter => {
            app.reload_detail();
            focus_detail(app);
        }
        KeyCode::Char('t') => app.cycle_type_filter(1),
        KeyCode::Char('T') => app.cycle_type_filter(-1),
        KeyCode::Char('s') => {
            let cur = app.scan_pattern.clone();
            open_prompt(
                app,
                PromptKind::ScanPattern,
                "SCAN MATCH",
                "server-side key pattern, e.g. user:* (empty = *)",
                &cur,
            );
        }
        KeyCode::Char('n') => open_prompt(
            app,
            PromptKind::NewKey,
            "new key",
            "<type> <key> <value...> — string|hash|list|set|zset|stream",
            "string ",
        ),
        KeyCode::Char('e') => edit_selected(app),
        KeyCode::Char('a') => add_to_selected(app),
        KeyCode::Char('d') => {
            if let Some(k) = selected {
                ask(
                    app,
                    format!("Delete key `{k}`?"),
                    ConfirmAction::DeleteKey(k),
                );
            } else {
                app.warn("No key selected");
            }
        }
        KeyCode::Char('D') => delete_item_prompt(app),
        KeyCode::Char('m') => {
            if let Some(k) = selected {
                open_prompt(
                    app,
                    PromptKind::Rename(k.clone()),
                    "rename to",
                    "new key name (RENAME)",
                    &k,
                );
            } else {
                app.warn("No key selected");
            }
        }
        KeyCode::Char('x') => {
            if let Some(k) = selected {
                open_prompt(
                    app,
                    PromptKind::Expire(k),
                    "TTL",
                    "seconds, or 5m / 2h / 1d; -1 removes the expiry",
                    "",
                );
            } else {
                app.warn("No key selected");
            }
        }
        KeyCode::Char('y') => {
            if let Some(k) = selected {
                open_prompt(
                    app,
                    PromptKind::Command,
                    ":",
                    "resource or command",
                    &format!("get {k}"),
                );
            }
        }
        _ => {}
    }
}

/// Open the right editing prompt for whatever is highlighted.
fn edit_selected(app: &mut App) {
    let Some(key) = app.selected_key_name() else {
        app.warn("No key selected");
        return;
    };
    let Some(detail) = app.detail.clone() else {
        app.warn("Key not loaded — press Enter first");
        return;
    };
    let idx = app.detail_cur.sel;
    match &detail.data {
        KeyData::Str(v) => open_prompt(
            app,
            PromptKind::EditString(key),
            "SET value",
            "new string value (SET)",
            v,
        ),
        KeyData::Hash(pairs) => {
            if app.focus == Focus::Detail {
                if let Some((f, v)) = pairs.get(idx) {
                    open_prompt(
                        app,
                        PromptKind::HashEdit {
                            key,
                            field: f.clone(),
                        },
                        &format!("HSET {f}"),
                        "new value for this field",
                        v,
                    );
                    return;
                }
            }
            open_prompt(
                app,
                PromptKind::HashSet(key),
                "HSET",
                "field=value pairs, e.g. name=alice age=30",
                "",
            );
        }
        KeyData::List(items) => {
            if app.focus == Focus::Detail {
                if let Some(v) = items.get(idx) {
                    open_prompt(
                        app,
                        PromptKind::ListEdit {
                            key,
                            index: idx as isize,
                        },
                        &format!("LSET [{idx}]"),
                        "new element value",
                        v,
                    );
                    return;
                }
            }
            open_prompt(
                app,
                PromptKind::ListPush {
                    key,
                    front: false,
                },
                "RPUSH",
                "one or more elements (quote to include spaces)",
                "",
            );
        }
        KeyData::Set(members) => {
            if app.focus == Focus::Detail {
                if let Some(m) = members.get(idx) {
                    open_prompt(
                        app,
                        PromptKind::SetEdit {
                            key,
                            member: m.clone(),
                        },
                        "replace member",
                        "new member value (SREM + SADD)",
                        m,
                    );
                    return;
                }
            }
            open_prompt(app, PromptKind::SetAdd(key), "SADD", "one or more members", "");
        }
        KeyData::ZSet(members) => {
            if app.focus == Focus::Detail {
                if let Some((m, s)) = members.get(idx) {
                    open_prompt(
                        app,
                        PromptKind::ZEdit {
                            key,
                            member: m.clone(),
                        },
                        &format!("ZADD score for {m}"),
                        "new numeric score",
                        &fmt_score(*s),
                    );
                    return;
                }
            }
            open_prompt(
                app,
                PromptKind::ZAdd(key),
                "ZADD",
                "member=score pairs, e.g. alice=10 bob=20",
                "",
            );
        }
        KeyData::Stream(_) => open_prompt(
            app,
            PromptKind::XAdd(key),
            "XADD",
            "[id] field=value ... (id `*` auto-generates)",
            "* ",
        ),
        KeyData::Missing => app.warn("Key no longer exists"),
        KeyData::Other(_) => app.warn("This key type cannot be edited here"),
    }
}

/// `a` always appends rather than replacing.
fn add_to_selected(app: &mut App) {
    let Some(key) = app.selected_key_name() else {
        app.warn("No key selected");
        return;
    };
    let kind = app
        .detail
        .as_ref()
        .map(|d| d.kind)
        .or_else(|| app.selected_key().map(|k| k.kind));
    match kind {
        Some(KeyKind::Hash) => open_prompt(
            app,
            PromptKind::HashSet(key),
            "HSET",
            "field=value pairs, e.g. name=alice age=30",
            "",
        ),
        Some(KeyKind::List) => open_prompt(
            app,
            PromptKind::ListPush { key, front: false },
            "RPUSH",
            "one or more elements (quote to include spaces)",
            "",
        ),
        Some(KeyKind::Set) => {
            open_prompt(app, PromptKind::SetAdd(key), "SADD", "one or more members", "")
        }
        Some(KeyKind::ZSet) => open_prompt(
            app,
            PromptKind::ZAdd(key),
            "ZADD",
            "member=score pairs, e.g. alice=10 bob=20",
            "",
        ),
        Some(KeyKind::Stream) => open_prompt(
            app,
            PromptKind::XAdd(key),
            "XADD",
            "[id] field=value ... (id `*` auto-generates)",
            "* ",
        ),
        Some(KeyKind::String) => {
            let cur = app
                .detail
                .as_ref()
                .and_then(|d| match &d.data {
                    KeyData::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            open_prompt(
                app,
                PromptKind::EditString(key),
                "SET value",
                "new string value (SET replaces the whole value)",
                &cur,
            )
        }
        _ => app.warn("Select a key first"),
    }
}

fn delete_item_prompt(app: &mut App) {
    let Some(key) = current_key_name(app) else {
        app.warn("No key selected");
        return;
    };
    let Some(label) = app.selected_item_label() else {
        app.warn("Nothing to delete in this key");
        return;
    };
    let what = match app.detail.as_ref().map(|d| d.kind) {
        Some(KeyKind::Hash) => format!("field `{label}`"),
        Some(KeyKind::List) => label.clone(),
        Some(KeyKind::Set) | Some(KeyKind::ZSet) => format!("member `{label}`"),
        Some(KeyKind::Stream) => format!("entry `{label}`"),
        _ => format!("`{label}`"),
    };
    ask(
        app,
        format!("Delete {what} from `{key}`?"),
        ConfirmAction::DeleteItem { key, label },
    );
}

/// The key the detail pane is showing, in either the Keys or Streams view.
fn current_key_name(app: &App) -> Option<String> {
    match app.view {
        View::Streams => app.selected_stream().map(|s| s.name.clone()),
        _ => app.selected_key_name(),
    }
}

fn streams_view(app: &mut App, key: KeyEvent) {
    let name = app.selected_stream().map(|s| s.name.clone());
    match key.code {
        KeyCode::Enter => {
            app.load_stream_detail();
            focus_detail(app);
        }
        KeyCode::Char('a') | KeyCode::Char('e') => {
            if let Some(n) = name {
                open_prompt(
                    app,
                    PromptKind::XAdd(n),
                    "XADD",
                    "[id] field=value ... (id `*` auto-generates)",
                    "* ",
                );
            } else {
                app.warn("No stream selected — use n in :keys to create one");
            }
        }
        KeyCode::Char('n') => open_prompt(
            app,
            PromptKind::NewKey,
            "new key",
            "<type> <key> <value...> — e.g. stream events * msg=hi",
            "stream ",
        ),
        KeyCode::Char('D') => delete_item_prompt(app),
        KeyCode::Char('d') => {
            if let Some(n) = name {
                ask(
                    app,
                    format!("Delete stream `{n}` and all its entries?"),
                    ConfirmAction::DeleteKey(n),
                );
            } else {
                app.warn("No stream selected");
            }
        }
        KeyCode::Char('y') => {
            if let Some(n) = name {
                jump_to_key(app, &n);
            }
        }
        _ => {}
    }
}

fn pubsub_view(app: &mut App, key: KeyEvent) {
    let channel = app.selected_channel().map(|c| (c.name.clone(), c.pattern));
    match key.code {
        KeyCode::Enter | KeyCode::Char(' ') => match channel {
            Some((name, pattern)) => match app.sub.toggle(&name, pattern) {
                Ok(true) => {
                    app.ok(format!(
                        "{} {name}",
                        if pattern { "PSUBSCRIBE" } else { "SUBSCRIBE" }
                    ));
                    app.refresh_channels();
                }
                Ok(false) => {
                    app.ok(format!("Unsubscribed from {name}"));
                    app.refresh_channels();
                }
                Err(e) => app.error(crate::app::format_err(&e)),
            },
            None => app.warn("No channel selected — press P to subscribe to a pattern"),
        },
        KeyCode::Char('s') => open_prompt(
            app,
            PromptKind::Subscribe { pattern: false },
            "SUBSCRIBE",
            "channel name to subscribe / unsubscribe",
            "",
        ),
        KeyCode::Char('P') => open_prompt(
            app,
            PromptKind::Subscribe { pattern: true },
            "PSUBSCRIBE",
            "channel glob pattern, e.g. news.*",
            "",
        ),
        KeyCode::Char('p') => {
            let target = channel.as_ref().map(|(n, _)| n.clone());
            let label = match &target {
                Some(n) => format!("PUBLISH {n}"),
                None => "PUBLISH".to_string(),
            };
            let hint = match &target {
                Some(_) => "message payload",
                None => "<channel> <message>",
            };
            open_prompt(app, PromptKind::Publish(target), &label, hint, "");
        }
        KeyCode::Char('c') => {
            app.sub.clear_messages();
            app.msg_cur = Cursor::default();
            app.ok("Cleared received messages");
        }
        KeyCode::Char('u') => {
            app.sub.stop_all();
            app.ok("Unsubscribed from all channels");
            app.refresh_channels();
        }
        _ => {}
    }
}

fn acl_view(app: &mut App, key: KeyEvent) {
    let user = app.selected_acl().map(|u| (u.name.clone(), u.rule.clone()));
    match key.code {
        KeyCode::Enter => focus_detail(app),
        KeyCode::Char('a') | KeyCode::Char('n') => open_prompt(
            app,
            PromptKind::AclSetUser(None),
            "ACL SETUSER",
            "<username> [rules...] e.g. alice on >secret ~* +@read",
            "",
        ),
        KeyCode::Char('e') => match user {
            Some((name, rule)) => {
                // `ACL LIST` gives `user <name> <rules...>`; drop the prefix.
                let rules = rule
                    .split_whitespace()
                    .skip(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                open_prompt(
                    app,
                    PromptKind::AclSetUser(Some(name.clone())),
                    "ACL SETUSER",
                    "rules are merged into the existing user",
                    &format!("{name} {rules}"),
                );
            }
            None => app.warn("No ACL user selected"),
        },
        KeyCode::Char('d') => match user {
            Some((name, _)) => {
                if name == "default" {
                    app.warn("The `default` user cannot be deleted");
                } else {
                    ask(
                        app,
                        format!("Delete ACL user `{name}`?"),
                        ConfirmAction::DeleteAclUser(name),
                    );
                }
            }
            None => app.warn("No ACL user selected"),
        },
        _ => {}
    }
}

fn servers_view(app: &mut App, key: KeyEvent) {
    let sel = app.cfg.servers.get(app.server_cur.sel).cloned();
    match key.code {
        KeyCode::Enter => match sel {
            Some(s) => app.connect(&s.name, &s.uri),
            None => app.warn("No server selected — press a to add one"),
        },
        KeyCode::Char('a') | KeyCode::Char('n') => open_prompt(
            app,
            PromptKind::AddServer,
            "add server",
            "<name> <redis-uri> e.g. New_user redis://localhost:6379/0",
            "",
        ),
        KeyCode::Char('e') => match sel {
            Some(s) => open_prompt(
                app,
                PromptKind::AddServer,
                "edit server",
                "<name> <redis-uri>",
                &format!("{} {}", s.name, s.uri),
            ),
            None => app.warn("No server selected"),
        },
        KeyCode::Char('d') => match sel {
            Some(s) => ask(
                app,
                format!("Remove server `{}` from the list?", s.name),
                ConfirmAction::RemoveServer(s.name),
            ),
            None => app.warn("No server selected"),
        },
        KeyCode::Char('w') => match app.cfg.save() {
            Ok(()) => app.ok(format!("Saved {}", crate::config::Config::path().display())),
            Err(e) => app.error(crate::app::format_err(&e)),
        },
        KeyCode::Char('s') => open_prompt(
            app,
            PromptKind::SelectDb,
            "SELECT db",
            "logical database index, e.g. 0",
            "",
        ),
        _ => {}
    }
}
