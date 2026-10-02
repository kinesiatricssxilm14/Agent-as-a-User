//! Application state: what is currently loaded, selected and being typed.

use std::time::{Duration, Instant};

use crate::config::{normalize_uri, Config, DEFAULT_URI};
use crate::db::{AclUser, ChannelInfo, Db, KeyData, KeyDetail, KeyEntry, KeyKind, StreamSummary};
use crate::sub::Subscriber;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Keys,
    Streams,
    PubSub,
    Acl,
    Servers,
    Info,
    Help,
}

impl View {
    /// Resource name as typed in the `:` selector.
    pub fn slug(&self) -> &'static str {
        match self {
            View::Keys => "keys",
            View::Streams => "streams",
            View::PubSub => "pubsub",
            View::Acl => "acl",
            View::Servers => "servers",
            View::Info => "info",
            View::Help => "help",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            View::Keys => "Keys",
            View::Streams => "Streams",
            View::PubSub => "Pub/Sub",
            View::Acl => "ACL",
            View::Servers => "Servers",
            View::Info => "Server Info",
            View::Help => "Help",
        }
    }

    /// Views reachable with the number keys / Tab, in order.
    pub const TABS: [View; 6] = [
        View::Keys,
        View::Streams,
        View::PubSub,
        View::Acl,
        View::Servers,
        View::Info,
    ];

    pub fn from_slug(s: &str) -> Option<View> {
        let s = s.trim().trim_start_matches(':').to_ascii_lowercase();
        match s.as_str() {
            "keys" | "key" | "k" => Some(View::Keys),
            "streams" | "stream" | "s" => Some(View::Streams),
            "pubsub" | "pub" | "channels" | "p" => Some(View::PubSub),
            "acl" | "users" | "a" => Some(View::Acl),
            "servers" | "server" | "conn" | "connections" => Some(View::Servers),
            "info" | "server-info" | "i" => Some(View::Info),
            "help" | "h" | "?" => Some(View::Help),
            _ => None,
        }
    }
}

/// Which pane inside a two-pane view receives navigation keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    List,
    Detail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptKind {
    /// `:` resource selector / command line.
    Command,
    /// `/` incremental name filter (applies while typing).
    Search,
    /// Server-side `SCAN MATCH` pattern.
    ScanPattern,
    NewKey,
    EditString(String),
    HashSet(String),
    HashEdit { key: String, field: String },
    ListPush { key: String, front: bool },
    ListEdit { key: String, index: isize },
    SetAdd(String),
    SetEdit { key: String, member: String },
    ZAdd(String),
    ZEdit { key: String, member: String },
    XAdd(String),
    Rename(String),
    Expire(String),
    Publish(Option<String>),
    Subscribe { pattern: bool },
    AddServer,
    AclSetUser(Option<String>),
    SelectDb,
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub kind: PromptKind,
    pub label: String,
    pub hint: String,
    pub input: String,
    /// Caret position as a character index into `input`.
    pub cursor: usize,
}

impl Prompt {
    pub fn new(kind: PromptKind, label: &str, hint: &str, initial: &str) -> Prompt {
        Prompt {
            kind,
            label: label.to_string(),
            hint: hint.to_string(),
            input: initial.to_string(),
            cursor: initial.chars().count(),
        }
    }

    pub fn insert(&mut self, c: char) {
        let idx = self.byte_at(self.cursor);
        self.input.insert(idx, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_at(self.cursor - 1);
        let end = self.byte_at(self.cursor);
        self.input.replace_range(start..end, "");
        self.cursor -= 1;
    }

    pub fn delete(&mut self) {
        let len = self.input.chars().count();
        if self.cursor >= len {
            return;
        }
        let start = self.byte_at(self.cursor);
        let end = self.byte_at(self.cursor + 1);
        self.input.replace_range(start..end, "");
    }

    /// Delete the word before the caret (Ctrl-W).
    pub fn kill_word(&mut self) {
        let chars: Vec<char> = self.input.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        let start = self.byte_at(i);
        let end = self.byte_at(self.cursor);
        self.input.replace_range(start..end, "");
        self.cursor = i;
    }

    pub fn kill_to_start(&mut self) {
        let end = self.byte_at(self.cursor);
        self.input.replace_range(0..end, "");
        self.cursor = 0;
    }

    pub fn kill_to_end(&mut self) {
        let start = self.byte_at(self.cursor);
        self.input.truncate(start);
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.input.chars().count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.input.chars().count();
    }

    fn byte_at(&self, char_idx: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    DeleteKey(String),
    DeleteItem { key: String, label: String },
    DeleteAclUser(String),
    RemoveServer(String),
}

#[derive(Clone, Debug)]
pub struct Confirm {
    pub question: String,
    pub action: ConfirmAction,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub text: String,
    pub level: Level,
}

/// Selection + scroll state for one list/detail pair.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cursor {
    pub sel: usize,
    pub scroll: usize,
}

impl Cursor {
    pub fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.sel = 0;
            self.scroll = 0;
        } else if self.sel >= len {
            self.sel = len - 1;
        }
    }

    pub fn move_by(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.sel = 0;
            return;
        }
        let next = self.sel as isize + delta;
        self.sel = next.clamp(0, len as isize - 1) as usize;
    }
}

pub struct App {
    pub db: Option<Db>,
    pub cfg: Config,
    pub sub: Subscriber,

    pub view: View,
    /// View to return to when leaving the help page.
    pub prev_view: View,
    pub focus: Focus,
    pub should_quit: bool,

    // ----- keys view
    pub keys: Vec<KeyEntry>,
    pub keys_truncated: bool,
    pub total_keys: usize,
    pub scan_pattern: String,
    pub filter: String,
    pub type_filter: Option<KeyKind>,
    pub key_cur: Cursor,
    pub detail: Option<KeyDetail>,
    pub detail_cur: Cursor,
    /// Key name whose detail is currently loaded.
    loaded_key: Option<String>,

    // ----- streams view
    pub streams: Vec<StreamSummary>,
    pub stream_cur: Cursor,
    pub stream_detail_cur: Cursor,
    pub stream_groups: Vec<Vec<(String, String)>>,

    // ----- pubsub view
    pub channels: Vec<ChannelInfo>,
    pub channel_cur: Cursor,
    pub msg_cur: Cursor,
    pub numpat: i64,

    // ----- acl view
    pub acl: Vec<AclUser>,
    pub acl_cur: Cursor,
    pub acl_detail_cur: Cursor,
    pub whoami: String,

    // ----- servers view
    pub server_cur: Cursor,

    // ----- info view
    pub info: Vec<(String, String)>,
    pub info_cur: Cursor,
    pub keyspace: Vec<(i64, usize)>,

    // ----- help view
    pub help_cur: Cursor,

    pub prompt: Option<Prompt>,
    pub confirm: Option<Confirm>,
    pub status: Status,
    status_at: Instant,
    /// Command history for the `:` prompt, newest last.
    pub history: Vec<String>,
    pub history_pos: Option<usize>,
    pub last_refresh: Instant,
}

impl App {
    pub fn new(cfg: Config, db: Option<Db>, err: Option<String>) -> App {
        let uri = db
            .as_ref()
            .map(|d| d.uri.clone())
            .unwrap_or_else(|| DEFAULT_URI.to_string());
        let status = match &err {
            Some(e) => Status {
                text: format!("{e} — press 5 for Servers, then a to add / Enter to connect"),
                level: Level::Error,
            },
            None => Status {
                text: "Connected. Press ? for help, : for the resource selector.".into(),
                level: Level::Ok,
            },
        };
        let view = if db.is_some() { View::Keys } else { View::Servers };
        let mut app = App {
            db,
            cfg,
            sub: Subscriber::new(&uri),
            view,
            prev_view: View::Keys,
            focus: Focus::List,
            should_quit: false,
            keys: Vec::new(),
            keys_truncated: false,
            total_keys: 0,
            scan_pattern: "*".into(),
            filter: String::new(),
            type_filter: None,
            key_cur: Cursor::default(),
            detail: None,
            detail_cur: Cursor::default(),
            loaded_key: None,
            streams: Vec::new(),
            stream_cur: Cursor::default(),
            stream_detail_cur: Cursor::default(),
            stream_groups: Vec::new(),
            channels: Vec::new(),
            channel_cur: Cursor::default(),
            msg_cur: Cursor::default(),
            numpat: 0,
            acl: Vec::new(),
            acl_cur: Cursor::default(),
            acl_detail_cur: Cursor::default(),
            whoami: String::new(),
            server_cur: Cursor::default(),
            info: Vec::new(),
            info_cur: Cursor::default(),
            keyspace: Vec::new(),
            help_cur: Cursor::default(),
            prompt: None,
            confirm: None,
            status,
            status_at: Instant::now(),
            history: Vec::new(),
            history_pos: None,
            last_refresh: Instant::now(),
        };
        app.refresh_current();
        app
    }

    // ------------------------------------------------------------- messaging

    pub fn set_status(&mut self, level: Level, text: impl Into<String>) {
        self.status = Status {
            text: text.into(),
            level,
        };
        self.status_at = Instant::now();
    }

    pub fn ok(&mut self, text: impl Into<String>) {
        self.set_status(Level::Ok, text);
    }

    pub fn info_msg(&mut self, text: impl Into<String>) {
        self.set_status(Level::Info, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.set_status(Level::Warn, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.set_status(Level::Error, text);
    }

    /// Age of the current status message; the footer fades stale messages.
    pub fn status_age(&self) -> Duration {
        self.status_at.elapsed()
    }

    /// Report a `Result` as a status line, keeping the message on success.
    pub fn report(&mut self, r: anyhow::Result<String>) {
        match r {
            Ok(msg) => self.ok(msg),
            Err(e) => self.error(format_err(&e)),
        }
    }

    // ----------------------------------------------------------- connection

    pub fn connected(&self) -> bool {
        self.db.is_some()
    }

    pub fn connect(&mut self, name: &str, uri: &str) {
        let uri = normalize_uri(uri);
        match Db::connect(name, &uri) {
            Ok(db) => {
                self.sub.retarget(&uri);
                self.db = Some(db);
                self.loaded_key = None;
                self.detail = None;
                self.keys.clear();
                self.streams.clear();
                self.channels.clear();
                self.acl.clear();
                self.info.clear();
                self.key_cur = Cursor::default();
                self.ok(format!("Connected to {name} ({uri})"));
                if self.view == View::Servers {
                    self.view = View::Keys;
                }
                self.refresh_current();
            }
            Err(e) => self.error(format!("{name}: {}", format_err(&e))),
        }
    }

    // -------------------------------------------------------------- refresh

    /// Reload the data backing the current view from Redis.
    pub fn refresh_current(&mut self) {
        self.last_refresh = Instant::now();
        match self.view {
            View::Keys => self.refresh_keys(),
            View::Streams => self.refresh_streams(),
            View::PubSub => self.refresh_channels(),
            View::Acl => self.refresh_acl(),
            View::Info => self.refresh_info(),
            View::Servers | View::Help => {}
        }
    }

    pub fn refresh_keys(&mut self) {
        let selected = self.selected_key_name();
        let pattern = self.scan_pattern.clone();
        let Some(db) = self.db.as_mut() else { return };
        match db.scan_keys(&pattern, None) {
            Ok((keys, truncated)) => {
                self.keys = keys;
                self.keys_truncated = truncated;
                self.total_keys = db.dbsize().unwrap_or(self.keys.len());
            }
            Err(e) => {
                self.error(format!("SCAN failed: {}", format_err(&e)));
                return;
            }
        }
        // Keep the cursor on the same key across refreshes when possible.
        if let Some(name) = selected {
            let visible = self.visible_keys();
            if let Some(pos) = visible.iter().position(|i| self.keys[*i].name == name) {
                self.key_cur.sel = pos;
            }
        }
        let len = self.visible_keys().len();
        self.key_cur.clamp(len);
        self.loaded_key = None;
        self.load_detail();
    }

    pub fn refresh_streams(&mut self) {
        let pattern = self.scan_pattern.clone();
        let Some(db) = self.db.as_mut() else { return };
        match db.streams(&pattern) {
            Ok(s) => {
                self.streams = s;
                let len = self.visible_streams().len();
                self.stream_cur.clamp(len);
                self.load_stream_detail();
            }
            Err(e) => self.error(format!("XINFO failed: {}", format_err(&e))),
        }
    }

    pub fn refresh_channels(&mut self) {
        let local = self.sub.list();
        let pattern = if self.scan_pattern.trim().is_empty() {
            "*".to_string()
        } else {
            self.scan_pattern.clone()
        };
        let Some(db) = self.db.as_mut() else { return };
        match db.pubsub_channels(&pattern, &local) {
            Ok(c) => {
                self.channels = c;
                self.numpat = db.pubsub_numpat();
                let len = self.visible_channels().len();
                self.channel_cur.clamp(len);
            }
            Err(e) => self.error(format!("PUBSUB failed: {}", format_err(&e))),
        }
    }

    pub fn refresh_acl(&mut self) {
        let Some(db) = self.db.as_mut() else { return };
        self.whoami = db.acl_whoami();
        match db.acl_users() {
            Ok(users) => {
                self.acl = users;
                let len = self.visible_acl().len();
                self.acl_cur.clamp(len);
            }
            Err(e) => self.error(format!("ACL USERS failed: {}", format_err(&e))),
        }
    }

    pub fn refresh_info(&mut self) {
        let Some(db) = self.db.as_mut() else { return };
        match db.info("everything") {
            Ok(map) => {
                self.info = map.into_iter().collect();
                self.keyspace = db.keyspace();
                self.total_keys = db.dbsize().unwrap_or(0);
            }
            Err(e) => self.error(format!("INFO failed: {}", format_err(&e))),
        }
    }

    // ---------------------------------------------------------- keys helpers

    /// Indices into `self.keys` that pass the type filter and name filter.
    pub fn visible_keys(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.keys
            .iter()
            .enumerate()
            .filter(|(_, k)| match self.type_filter {
                Some(t) => k.kind == t,
                None => true,
            })
            .filter(|(_, k)| matches_filter(&k.name, &needle))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn type_counts(&self) -> Vec<(KeyKind, usize)> {
        KeyKind::ALL
            .iter()
            .map(|t| (*t, self.keys.iter().filter(|k| k.kind == *t).count()))
            .collect()
    }

    pub fn selected_key(&self) -> Option<&KeyEntry> {
        let visible = self.visible_keys();
        visible.get(self.key_cur.sel).map(|i| &self.keys[*i])
    }

    pub fn selected_key_name(&self) -> Option<String> {
        self.selected_key().map(|k| k.name.clone())
    }

    /// Load the detail pane for the highlighted key if it is not loaded yet.
    pub fn load_detail(&mut self) {
        let Some(name) = self.selected_key_name() else {
            self.detail = None;
            self.loaded_key = None;
            return;
        };
        if self.loaded_key.as_deref() == Some(name.as_str()) {
            return;
        }
        self.reload_detail_for(&name);
    }

    /// Re-read the detail pane for whichever list currently drives it. The
    /// Streams view points the pane at the selected stream, not at the Keys
    /// selection, so the two must not be conflated.
    pub fn reload_detail(&mut self) {
        match self.view {
            View::Streams => {
                self.loaded_key = None;
                self.load_stream_detail();
            }
            _ => {
                if let Some(name) = self.selected_key_name() {
                    self.reload_detail_for(&name);
                }
            }
        }
    }

    fn reload_detail_for(&mut self, name: &str) {
        let Some(db) = self.db.as_mut() else { return };
        match db.key_detail(name) {
            Ok(d) => {
                let count = d.data.item_count();
                self.detail = Some(d);
                self.loaded_key = Some(name.to_string());
                self.detail_cur.clamp(count);
            }
            Err(e) => {
                self.detail = None;
                self.loaded_key = None;
                self.error(format!("{name}: {}", format_err(&e)));
            }
        }
    }

    /// Number of navigable rows in the key detail pane.
    pub fn detail_len(&self) -> usize {
        self.detail.as_ref().map(|d| d.data.item_count()).unwrap_or(0)
    }

    /// Human label for the highlighted item inside the detail pane.
    pub fn selected_item_label(&self) -> Option<String> {
        let d = self.detail.as_ref()?;
        let i = self.detail_cur.sel;
        match &d.data {
            KeyData::Str(_) => Some("value".to_string()),
            KeyData::Hash(v) => v.get(i).map(|(f, _)| f.clone()),
            KeyData::List(v) => v.get(i).map(|_| format!("index {i}")),
            KeyData::Set(v) => v.get(i).cloned(),
            KeyData::ZSet(v) => v.get(i).map(|(m, _)| m.clone()),
            KeyData::Stream(v) => v.get(i).map(|e| e.id.clone()),
            _ => None,
        }
    }

    // ------------------------------------------------------- streams helpers

    pub fn visible_streams(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.streams
            .iter()
            .enumerate()
            .filter(|(_, s)| matches_filter(&s.name, &needle))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_stream(&self) -> Option<&StreamSummary> {
        let v = self.visible_streams();
        v.get(self.stream_cur.sel).map(|i| &self.streams[*i])
    }

    /// Stream detail reuses the key detail pane (a stream *is* a key).
    pub fn load_stream_detail(&mut self) {
        let Some(name) = self.selected_stream().map(|s| s.name.clone()) else {
            self.detail = None;
            self.stream_groups.clear();
            return;
        };
        self.reload_detail_for(&name);
        if let Some(db) = self.db.as_mut() {
            self.stream_groups = db.stream_groups(&name);
        }
        let len = self.detail_len();
        self.stream_detail_cur.clamp(len);
    }

    // -------------------------------------------------------- pubsub helpers

    pub fn visible_channels(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.channels
            .iter()
            .enumerate()
            .filter(|(_, c)| matches_filter(&c.name, &needle))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_channel(&self) -> Option<&ChannelInfo> {
        let v = self.visible_channels();
        v.get(self.channel_cur.sel).map(|i| &self.channels[*i])
    }

    // ----------------------------------------------------------- acl helpers

    /// `INFO` fields plus per-database key counts, honouring the name filter.
    pub fn info_rows(&self) -> Vec<(String, String)> {
        let needle = self.filter.to_lowercase();
        let mut rows: Vec<(String, String)> = self
            .keyspace
            .iter()
            .map(|(idx, keys)| (format!("db{idx}"), format!("{keys} keys")))
            .collect();
        rows.extend(self.info.iter().cloned());
        rows.into_iter()
            .filter(|(k, v)| {
                matches_filter(k, &needle) || (!needle.is_empty() && matches_filter(v, &needle))
            })
            .collect()
    }

    pub fn visible_acl(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.acl
            .iter()
            .enumerate()
            .filter(|(_, u)| matches_filter(&u.name, &needle))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_acl(&self) -> Option<&AclUser> {
        let v = self.visible_acl();
        v.get(self.acl_cur.sel).map(|i| &self.acl[*i])
    }

    // --------------------------------------------------------- view plumbing

    pub fn goto(&mut self, view: View) {
        if view == self.view {
            return;
        }
        if view == View::Help {
            self.prev_view = self.view;
        }
        self.view = view;
        self.focus = Focus::List;
        self.refresh_current();
    }

    pub fn next_tab(&mut self, delta: isize) {
        let tabs = View::TABS;
        let cur = tabs.iter().position(|v| *v == self.view).unwrap_or(0) as isize;
        let n = tabs.len() as isize;
        let next = ((cur + delta) % n + n) % n;
        self.goto(tabs[next as usize]);
    }

    /// The cursor driving list navigation in the focused pane.
    pub fn active_cursor_len(&self) -> usize {
        match (self.view, self.focus) {
            (View::Keys, Focus::List) => self.visible_keys().len(),
            (View::Keys, Focus::Detail) => self.detail_len(),
            (View::Streams, Focus::List) => self.visible_streams().len(),
            (View::Streams, Focus::Detail) => self.detail_len(),
            (View::PubSub, Focus::List) => self.visible_channels().len(),
            (View::PubSub, Focus::Detail) => self.sub.message_count(),
            (View::Acl, Focus::List) => self.visible_acl().len(),
            (View::Acl, Focus::Detail) => {
                self.selected_acl().map(|u| u.attrs.len()).unwrap_or(0)
            }
            (View::Servers, _) => self.cfg.servers.len(),
            (View::Info, _) => self.info_rows().len(),
            (View::Help, _) => crate::help::help_lines().len(),
        }
    }

    pub fn cursor_mut(&mut self) -> &mut Cursor {
        match (self.view, self.focus) {
            (View::Keys, Focus::List) => &mut self.key_cur,
            (View::Keys, Focus::Detail) => &mut self.detail_cur,
            (View::Streams, Focus::List) => &mut self.stream_cur,
            (View::Streams, Focus::Detail) => &mut self.stream_detail_cur,
            (View::PubSub, Focus::List) => &mut self.channel_cur,
            (View::PubSub, Focus::Detail) => &mut self.msg_cur,
            (View::Acl, Focus::List) => &mut self.acl_cur,
            (View::Acl, Focus::Detail) => &mut self.acl_detail_cur,
            (View::Servers, _) => &mut self.server_cur,
            (View::Info, _) => &mut self.info_cur,
            (View::Help, _) => &mut self.help_cur,
        }
    }

    /// Move the focused list cursor and pull in any data it now needs.
    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.active_cursor_len();
        self.cursor_mut().move_by(delta, len);
        self.after_move();
    }

    pub fn cursor_to(&mut self, pos: usize) {
        let len = self.active_cursor_len();
        let c = self.cursor_mut();
        c.sel = pos;
        c.clamp(len);
        self.after_move();
    }

    fn after_move(&mut self) {
        match (self.view, self.focus) {
            (View::Keys, Focus::List) => self.load_detail(),
            (View::Streams, Focus::List) => self.load_stream_detail(),
            _ => {}
        }
    }

    /// Reset filter/selection when the visible set changes underneath us.
    pub fn on_filter_changed(&mut self) {
        match self.view {
            View::Keys => {
                self.key_cur = Cursor::default();
                self.loaded_key = None;
                self.load_detail();
            }
            View::Streams => {
                self.stream_cur = Cursor::default();
                self.load_stream_detail();
            }
            View::PubSub => self.channel_cur = Cursor::default(),
            View::Acl => self.acl_cur = Cursor::default(),
            _ => {}
        }
    }

    pub fn cycle_type_filter(&mut self, delta: isize) {
        // The filter ring is: all, then each concrete type.
        let types = KeyKind::ALL;
        let cur = match self.type_filter {
            None => 0isize,
            Some(t) => types.iter().position(|x| *x == t).unwrap() as isize + 1,
        };
        let n = types.len() as isize + 1;
        let next = ((cur + delta) % n + n) % n;
        self.type_filter = if next == 0 {
            None
        } else {
            Some(types[(next - 1) as usize])
        };
        self.on_filter_changed();
        let label = self
            .type_filter
            .map(|t| t.label().to_string())
            .unwrap_or_else(|| "all types".to_string());
        self.info_msg(format!("Type filter: {label}"));
    }
}

/// Case-insensitive substring match, or glob match when the needle has
/// wildcards. An empty needle matches everything.
pub fn matches_filter(name: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if needle_lower.contains('*') || needle_lower.contains('?') {
        glob_match(&needle_lower.to_lowercase(), &name.to_lowercase())
    } else {
        name.to_lowercase().contains(needle_lower)
    }
}

/// Minimal `*`/`?` glob matcher used for client-side filtering.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Flatten an `anyhow` chain into one status-line-friendly sentence.
pub fn format_err(e: &anyhow::Error) -> String {
    let mut parts: Vec<String> = Vec::new();
    for cause in e.chain() {
        let s = cause.to_string();
        if !parts.contains(&s) {
            parts.push(s);
        }
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("user:*", "user:1"));
        assert!(glob_match("*:1", "user:1"));
        assert!(glob_match("u?er:1", "user:1"));
        assert!(!glob_match("user:*", "order:1"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn filter_modes() {
        assert!(matches_filter("User:1", "user"));
        assert!(matches_filter("User:1", ""));
        assert!(matches_filter("User:1", "user:*"));
        assert!(!matches_filter("User:1", "order"));
    }

    #[test]
    fn prompt_edits() {
        let mut p = Prompt::new(PromptKind::Command, ":", "", "abc");
        assert_eq!(p.cursor, 3);
        p.left();
        p.insert('X');
        assert_eq!(p.input, "abXc");
        p.backspace();
        assert_eq!(p.input, "abc");
        p.home();
        p.delete();
        assert_eq!(p.input, "bc");
        p.end();
        p.kill_word();
        assert_eq!(p.input, "");
    }

    #[test]
    fn view_slugs() {
        for v in View::TABS {
            assert_eq!(View::from_slug(v.slug()), Some(v));
        }
    }

    #[test]
    fn cursor_clamp() {
        let mut c = Cursor { sel: 9, scroll: 4 };
        c.clamp(3);
        assert_eq!(c.sel, 2);
        c.clamp(0);
        assert_eq!(c.sel, 0);
        assert_eq!(c.scroll, 0);
    }
}
