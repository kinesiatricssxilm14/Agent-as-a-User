//! Application state and key-driven logic.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::redis_client::{
    ChannelInfo, KeyInfo, KeyKind, Payload, RedisClient, Request, StreamEntry, ValueData,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Keys,
    Streams,
    PubSub,
    Acl,
}

impl Resource {
    pub fn index(self) -> usize {
        match self {
            Resource::Keys => 0,
            Resource::Streams => 1,
            Resource::PubSub => 2,
            Resource::Acl => 3,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Resource::Keys,
            1 => Resource::Streams,
            2 => Resource::PubSub,
            _ => Resource::Acl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeFilter {
    All,
    String,
    Hash,
    List,
    Set,
    ZSet,
    Stream,
}

impl TypeFilter {
    pub fn label(self) -> &'static str {
        match self {
            TypeFilter::All => "all",
            TypeFilter::String => "string",
            TypeFilter::Hash => "hash",
            TypeFilter::List => "list",
            TypeFilter::Set => "set",
            TypeFilter::ZSet => "zset",
            TypeFilter::Stream => "stream",
        }
    }

    pub fn to_kind(self) -> Option<KeyKind> {
        match self {
            TypeFilter::All => None,
            TypeFilter::String => Some(KeyKind::String),
            TypeFilter::Hash => Some(KeyKind::Hash),
            TypeFilter::List => Some(KeyKind::List),
            TypeFilter::Set => Some(KeyKind::Set),
            TypeFilter::ZSet => Some(KeyKind::ZSet),
            TypeFilter::Stream => Some(KeyKind::Stream),
        }
    }

    pub fn from_digit(c: char) -> Option<Self> {
        match c {
            '1' => Some(TypeFilter::All),
            '2' => Some(TypeFilter::String),
            '3' => Some(TypeFilter::Hash),
            '4' => Some(TypeFilter::List),
            '5' => Some(TypeFilter::Set),
            '6' => Some(TypeFilter::ZSet),
            '7' => Some(TypeFilter::Stream),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Server {
    pub name: String,
    pub uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Command,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub enum InputCtx {
    None,
    CreatePickType,
    CreateName { kind: KeyKind },
    CreateValue { key: String, kind: KeyKind },
    EditValue { key: String },
    HashAddField { key: String },
    HashAddValue { key: String, field: String },
    ListAppend { key: String },
    SetAdd { key: String },
    ZAdd { key: String },
    StreamAdd { key: String },
    DeleteItem { key: String, kind: KeyKind },
    ServerName,
    ServerUri { name: String },
    Confirm { action: ConfirmAction },
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteKey { key: String },
}

pub struct App {
    pub worker: RedisClient,
    pub servers: Vec<Server>,
    pub server_idx: usize,
    pub connected: bool,
    pub version: String,
    pub db_size: u64,

    pub resource: Resource,
    pub focus: Focus,

    pub keys: Vec<KeyInfo>,
    pub key_cursor: usize,
    pub type_filter: TypeFilter,
    pub search: String,
    pub search_backup: String,
    pub selected_key: Option<String>,
    pub detail_data: Option<ValueData>,
    pub detail_scroll: usize,

    pub streams: Vec<KeyInfo>,
    pub stream_cursor: usize,
    pub stream_entries: Vec<StreamEntry>,

    pub channels: Vec<ChannelInfo>,
    pub channel_cursor: usize,

    pub acl_users: Vec<String>,
    pub acl_cursor: usize,
    pub acl_detail: Vec<String>,

    pub mode: Mode,
    pub input: String,
    pub input_prompt: String,
    pub ctx: InputCtx,

    pub message: Option<(String, MessageKind)>,
    pub show_help: bool,
    pub should_quit: bool,
}

const SCAN_LIMIT: usize = 20_000;

impl App {
    pub fn new() -> Self {
        let worker = RedisClient::spawn();
        let servers = vec![Server {
            name: "default".to_string(),
            uri: "redis://localhost:6379/0".to_string(),
        }];
        let mut app = App {
            worker,
            servers,
            server_idx: 0,
            connected: false,
            version: String::new(),
            db_size: 0,
            resource: Resource::Keys,
            focus: Focus::List,
            keys: Vec::new(),
            key_cursor: 0,
            type_filter: TypeFilter::All,
            search: String::new(),
            search_backup: String::new(),
            selected_key: None,
            detail_data: None,
            detail_scroll: 0,
            streams: Vec::new(),
            stream_cursor: 0,
            stream_entries: Vec::new(),
            channels: Vec::new(),
            channel_cursor: 0,
            acl_users: Vec::new(),
            acl_cursor: 0,
            acl_detail: Vec::new(),
            mode: Mode::Normal,
            input: String::new(),
            input_prompt: String::new(),
            ctx: InputCtx::None,
            message: None,
            show_help: false,
            should_quit: false,
        };
        app.connect();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.on_normal(key),
            Mode::Search => self.on_search(key),
            Mode::Command => self.on_command(key),
            Mode::Input => self.on_input(key),
        }
    }

    // ---------------------------------------------------------------- normal

    fn on_normal(&mut self, key: KeyEvent) {
        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => self.show_help = false,
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('?') | KeyCode::F(1) => self.show_help = true,
            KeyCode::Esc => {}
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.input.clear();
                self.input_prompt = ":".to_string();
            }
            KeyCode::Char('/') => {
                self.search_backup = self.search.clone();
                self.input = self.search.clone();
                self.mode = Mode::Search;
                self.input_prompt = "search: ".to_string();
            }
            KeyCode::Tab => self.cycle_resource(1),
            KeyCode::BackTab => self.cycle_resource(-1),
            KeyCode::Left | KeyCode::Char('h') => self.focus = Focus::List,
            KeyCode::Right | KeyCode::Char('l') => self.focus = Focus::Detail,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.focus == Focus::List {
                    self.move_cursor(-1);
                } else {
                    self.scroll_detail(-1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.focus == Focus::List {
                    self.move_cursor(1);
                } else {
                    self.scroll_detail(1);
                }
            }
            KeyCode::PageUp => self.page_scroll(-1),
            KeyCode::PageDown => self.page_scroll(1),
            KeyCode::Home | KeyCode::Char('g') => self.goto(false),
            KeyCode::End | KeyCode::Char('G') => self.goto(true),
            KeyCode::Enter => {
                self.focus = if self.focus == Focus::List {
                    Focus::Detail
                } else {
                    Focus::List
                };
                if self.focus == Focus::Detail {
                    self.load_selection();
                }
            }
            KeyCode::Char('e') => self.start_edit(),
            KeyCode::Char('a') => self.start_create(),
            KeyCode::Char('d') => self.start_delete(),
            KeyCode::Char('r') => self.load_resource(),
            KeyCode::Char('s') => self.start_add_server(),
            KeyCode::Char('c') => self.toggle_connect(),
            KeyCode::Char('[') => self.cycle_server(-1),
            KeyCode::Char(']') => self.cycle_server(1),
            KeyCode::Char(c) => {
                if self.resource == Resource::Keys {
                    if let Some(tf) = TypeFilter::from_digit(c) {
                        self.type_filter = tf;
                        self.key_cursor = 0;
                        self.load_keys();
                    }
                }
            }
            _ => {}
        }
    }

    // ----------------------------------------------------------------- search

    fn on_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search = self.search_backup.clone();
                self.mode = Mode::Normal;
                self.input.clear();
                self.key_cursor = 0;
                self.load_keys();
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.apply_search();
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if c == 'c' || c == 'u' {
                        self.input.clear();
                        self.apply_search();
                    }
                    return;
                }
                self.input.push(c);
                self.apply_search();
            }
            _ => {}
        }
    }

    fn apply_search(&mut self) {
        self.search = self.input.clone();
        self.key_cursor = 0;
        self.load_keys();
    }

    // ---------------------------------------------------------------- command

    fn on_command(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                let cmd = self.input.trim().to_string();
                self.input.clear();
                self.mode = Mode::Normal;
                self.run_command(&cmd);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if c == 'c' || c == 'u' {
                        self.input.clear();
                    }
                    return;
                }
                self.input.push(c);
            }
            _ => {}
        }
    }

    fn run_command(&mut self, cmd: &str) {
        match cmd {
            "keys" => self.switch_resource(Resource::Keys),
            "streams" => self.switch_resource(Resource::Streams),
            "pubsub" => self.switch_resource(Resource::PubSub),
            "acl" => self.switch_resource(Resource::Acl),
            "help" | "?" => self.show_help = true,
            "connect" => self.connect(),
            "disconnect" => self.disconnect(),
            "add-server" => self.start_add_server(),
            "refresh" | "r" => self.load_resource(),
            "quit" | "q" | "exit" => self.should_quit = true,
            "" => {}
            _ => self.set_message(format!("unknown command: {cmd}"), MessageKind::Error),
        }
    }

    // ------------------------------------------------------------------ input

    fn on_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_input(),
            KeyCode::Enter => self.submit_input(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'c' => self.cancel_input(),
                        'u' => self.input.clear(),
                        _ => {}
                    }
                    return;
                }
                if let InputCtx::CreatePickType = self.ctx {
                    if let Some(kind) = kind_from_create_digit(c) {
                        self.begin_create_name(kind);
                    }
                    return;
                }
                self.input.push(c);
            }
            _ => {}
        }
    }

    fn cancel_input(&mut self) {
        self.mode = Mode::Normal;
        self.ctx = InputCtx::None;
        self.input.clear();
        self.input_prompt.clear();
    }

    fn submit_input(&mut self) {
        let ctx = std::mem::replace(&mut self.ctx, InputCtx::None);
        let buffer = std::mem::take(&mut self.input);
        self.mode = Mode::Normal;
        self.input_prompt.clear();

        match ctx {
            InputCtx::None | InputCtx::CreatePickType => {}

            InputCtx::CreateName { kind } => {
                let key = buffer.trim().to_string();
                if key.is_empty() {
                    self.set_message("key name is required".to_string(), MessageKind::Error);
                    return;
                }
                self.prompt_create_value(key, kind);
            }
            InputCtx::CreateValue { key, kind } => self.do_create(key, kind, &buffer),

            InputCtx::EditValue { key } => {
                match self
                    .worker
                    .request(Request::Set { key: key.clone(), value: buffer })
                {
                    Ok(_) => {
                        self.set_message(format!("updated '{key}'"), MessageKind::Info);
                        self.load_resource();
                    }
                    Err(e) => self.set_message(e, MessageKind::Error),
                }
            }

            InputCtx::HashAddField { key } => {
                let field = buffer.trim().to_string();
                if field.is_empty() {
                    self.set_message("field name is required".to_string(), MessageKind::Error);
                    return;
                }
                self.mode = Mode::Input;
                self.ctx = InputCtx::HashAddValue { key, field };
                self.input.clear();
                self.input_prompt = "field value: ".to_string();
            }
            InputCtx::HashAddValue { key, field } => {
                match self.worker.request(Request::HSet {
                    key: key.clone(),
                    field,
                    value: buffer.trim().to_string(),
                }) {
                    Ok(_) => {
                        self.set_message(format!("field set on '{key}'"), MessageKind::Info);
                        self.load_resource();
                    }
                    Err(e) => self.set_message(e, MessageKind::Error),
                }
            }
            InputCtx::ListAppend { key } => {
                match self.worker.request(Request::RPush {
                    key: key.clone(),
                    value: buffer.trim().to_string(),
                }) {
                    Ok(_) => {
                        self.set_message(format!("appended to '{key}'"), MessageKind::Info);
                        self.load_resource();
                    }
                    Err(e) => self.set_message(e, MessageKind::Error),
                }
            }
            InputCtx::SetAdd { key } => {
                match self.worker.request(Request::SAdd {
                    key: key.clone(),
                    member: buffer.trim().to_string(),
                }) {
                    Ok(_) => {
                        self.set_message(format!("member added to '{key}'"), MessageKind::Info);
                        self.load_resource();
                    }
                    Err(e) => self.set_message(e, MessageKind::Error),
                }
            }
            InputCtx::ZAdd { key } => {
                match split_member_score(&buffer).into_iter().next() {
                    Some((member, score)) => {
                        match self.worker.request(Request::ZAdd {
                            key: key.clone(),
                            member,
                            score,
                        }) {
                            Ok(_) => {
                                self.set_message(format!("member added to '{key}'"), MessageKind::Info);
                                self.load_resource();
                            }
                            Err(e) => self.set_message(e, MessageKind::Error),
                        }
                    }
                    None => self.set_message(
                        "invalid input; expected member=score".to_string(),
                        MessageKind::Error,
                    ),
                }
            }
            InputCtx::StreamAdd { key } => {
                let fields = split_pairs(&buffer);
                if fields.is_empty() {
                    self.set_message("no valid field=value pairs".to_string(), MessageKind::Error);
                    return;
                }
                match self.worker.request(Request::XAdd { key: key.clone(), fields }) {
                    Ok(_) => {
                        self.set_message(format!("message added to '{key}'"), MessageKind::Info);
                        self.load_resource();
                    }
                    Err(e) => self.set_message(e, MessageKind::Error),
                }
            }
            InputCtx::DeleteItem { key, kind } => {
                let v = buffer.trim().to_string();
                if v.is_empty() {
                    self.set_message("value is required".to_string(), MessageKind::Error);
                    return;
                }
                let result = match kind {
                    KeyKind::Hash => self.worker.request(Request::HDel { key: key.clone(), field: v }),
                    KeyKind::List => self.worker.request(Request::LRem { key: key.clone(), value: v }),
                    KeyKind::Set => self.worker.request(Request::SRem { key: key.clone(), member: v }),
                    KeyKind::ZSet => self.worker.request(Request::ZRem { key: key.clone(), member: v }),
                    KeyKind::Stream => self.worker.request(Request::XDel { key: key.clone(), id: v }),
                    _ => {
                        self.set_message("cannot delete item of this type".to_string(), MessageKind::Error);
                        return;
                    }
                };
                match result {
                    Ok(_) => {
                        self.set_message(format!("item removed from '{key}'"), MessageKind::Info);
                        self.load_resource();
                    }
                    Err(e) => self.set_message(e, MessageKind::Error),
                }
            }

            InputCtx::ServerName => {
                let name = buffer.trim().to_string();
                if name.is_empty() {
                    self.set_message("server name is required".to_string(), MessageKind::Error);
                    return;
                }
                self.mode = Mode::Input;
                self.ctx = InputCtx::ServerUri { name };
                self.input = "redis://localhost:6379/0".to_string();
                self.input_prompt = "Redis URI: ".to_string();
            }
            InputCtx::ServerUri { name } => {
                let uri = buffer.trim().to_string();
                if uri.is_empty() {
                    self.set_message("URI is required".to_string(), MessageKind::Error);
                    return;
                }
                self.servers.push(Server { name, uri });
                self.server_idx = self.servers.len() - 1;
                self.connect();
            }

            InputCtx::Confirm { action } => {
                let b = buffer.trim().to_ascii_lowercase();
                if b == "y" || b == "yes" {
                    self.perform_confirm(action);
                }
            }
        }
    }

    // ------------------------------------------------------------- connection

    fn connect(&mut self) {
        if self.servers.is_empty() {
            self.set_message("no servers configured".to_string(), MessageKind::Error);
            return;
        }
        let uri = self.servers[self.server_idx].uri.clone();
        let name = self.servers[self.server_idx].name.clone();
        match self.worker.request(Request::Connect { uri }) {
            Ok(Payload::Connected { version }) => {
                self.connected = true;
                self.version = version;
                self.set_message(format!("connected to '{name}'"), MessageKind::Info);
                self.load_resource();
            }
            Ok(_) => {}
            Err(e) => {
                self.connected = false;
                self.set_message(e, MessageKind::Error);
            }
        }
    }

    fn disconnect(&mut self) {
        let _ = self.worker.request(Request::Disconnect);
        self.connected = false;
        self.clear_data();
        self.set_message("disconnected".to_string(), MessageKind::Info);
    }

    fn toggle_connect(&mut self) {
        if self.connected {
            self.disconnect();
        } else {
            self.connect();
        }
    }

    fn cycle_server(&mut self, delta: i32) {
        if self.servers.is_empty() {
            return;
        }
        let n = self.servers.len() as i64;
        let new = ((self.server_idx as i64 + delta as i64) % n + n) % n;
        self.server_idx = new as usize;
        if self.connected {
            self.connect();
        } else {
            self.set_message(
                format!("selected server '{}' (press c to connect)", self.servers[self.server_idx].name),
                MessageKind::Info,
            );
        }
    }

    fn start_add_server(&mut self) {
        self.mode = Mode::Input;
        self.ctx = InputCtx::ServerName;
        self.input.clear();
        self.input_prompt = "new server name: ".to_string();
    }

    fn clear_data(&mut self) {
        self.keys.clear();
        self.streams.clear();
        self.channels.clear();
        self.acl_users.clear();
        self.detail_data = None;
        self.stream_entries.clear();
        self.acl_detail.clear();
        self.selected_key = None;
        self.db_size = 0;
        self.key_cursor = 0;
        self.stream_cursor = 0;
        self.channel_cursor = 0;
        self.acl_cursor = 0;
        self.detail_scroll = 0;
    }

    // -------------------------------------------------------------- resources

    fn cycle_resource(&mut self, delta: i32) {
        let n = 4i32;
        let idx = ((self.resource.index() as i32 + delta) % n + n) % n;
        self.switch_resource(Resource::from_index(idx as usize));
    }

    fn switch_resource(&mut self, r: Resource) {
        if self.resource == r {
            self.load_resource();
            return;
        }
        self.resource = r;
        self.focus = Focus::List;
        self.detail_scroll = 0;
        self.load_resource();
    }

    fn load_resource(&mut self) {
        match self.resource {
            Resource::Keys => self.load_keys(),
            Resource::Streams => self.load_streams(),
            Resource::PubSub => self.load_channels(),
            Resource::Acl => self.load_users(),
        }
    }

    fn load_keys(&mut self) {
        if !self.connected {
            return;
        }
        match self.worker.request(Request::ScanKeys { limit: SCAN_LIMIT }) {
            Ok(Payload::Keys(mut list)) => {
                if let Some(kind) = self.type_filter.to_kind() {
                    list.retain(|k| k.kind == kind);
                }
                if !self.search.is_empty() {
                    let needle = self.search.clone();
                    list.retain(|k| k.name.contains(&needle));
                }
                list.sort_by(|a, b| a.name.cmp(&b.name));
                self.keys = list;
                self.key_cursor = self.key_cursor.min(self.keys.len().saturating_sub(1));
                if self.keys.is_empty() {
                    self.selected_key = None;
                    self.detail_data = None;
                } else {
                    self.load_selection();
                }
            }
            Ok(_) => {}
            Err(e) => self.set_message(e, MessageKind::Error),
        }
        if let Ok(Payload::DbSize(n)) = self.worker.request(Request::DbSize) {
            self.db_size = n;
        }
    }

    fn load_streams(&mut self) {
        if !self.connected {
            return;
        }
        match self.worker.request(Request::ScanKeys { limit: SCAN_LIMIT }) {
            Ok(Payload::Keys(list)) => {
                let mut s: Vec<KeyInfo> = list.into_iter().filter(|k| k.kind == KeyKind::Stream).collect();
                s.sort_by(|a, b| a.name.cmp(&b.name));
                self.streams = s;
                self.stream_cursor = self.stream_cursor.min(self.streams.len().saturating_sub(1));
                if self.streams.is_empty() {
                    self.stream_entries.clear();
                } else {
                    self.load_selection();
                }
            }
            Ok(_) => {}
            Err(e) => self.set_message(e, MessageKind::Error),
        }
    }

    fn load_channels(&mut self) {
        if !self.connected {
            return;
        }
        match self.worker.request(Request::PubSubChannels) {
            Ok(Payload::Channels(list)) => {
                self.channels = list;
                self.channel_cursor = self.channel_cursor.min(self.channels.len().saturating_sub(1));
            }
            Ok(_) => {}
            Err(e) => self.set_message(e, MessageKind::Error),
        }
    }

    fn load_users(&mut self) {
        if !self.connected {
            return;
        }
        match self.worker.request(Request::AclUsers) {
            Ok(Payload::AclUsers(list)) => {
                self.acl_users = list;
                self.acl_cursor = self.acl_cursor.min(self.acl_users.len().saturating_sub(1));
                self.load_selection();
            }
            Ok(_) => {}
            Err(e) => self.set_message(e, MessageKind::Error),
        }
    }

    fn load_selection(&mut self) {
        match self.resource {
            Resource::Keys => {
                if let Some(info) = self.keys.get(self.key_cursor) {
                    let name = info.name.clone();
                    self.selected_key = Some(name.clone());
                    match self.worker.request(Request::GetValue { key: name }) {
                        Ok(Payload::Value(v)) => {
                            self.detail_data = Some(v);
                            self.detail_scroll = 0;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            self.detail_data = Some(ValueData::None);
                            self.set_message(e, MessageKind::Error);
                        }
                    }
                } else {
                    self.selected_key = None;
                    self.detail_data = None;
                }
            }
            Resource::Streams => {
                if let Some(info) = self.streams.get(self.stream_cursor) {
                    let name = info.name.clone();
                    match self.worker.request(Request::GetValue { key: name }) {
                        Ok(Payload::Value(ValueData::Stream(entries))) => {
                            self.stream_entries = entries;
                            self.detail_scroll = 0;
                        }
                        Ok(_) => self.stream_entries.clear(),
                        Err(e) => {
                            self.stream_entries.clear();
                            self.set_message(e, MessageKind::Error);
                        }
                    }
                }
            }
            Resource::PubSub => {}
            Resource::Acl => {
                if let Some(user) = self.acl_users.get(self.acl_cursor) {
                    let name = user.clone();
                    match self.worker.request(Request::AclGetUser { name }) {
                        Ok(Payload::AclUser(lines)) => {
                            self.acl_detail = lines;
                            self.detail_scroll = 0;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            self.acl_detail = vec![e.clone()];
                            self.set_message(e, MessageKind::Error);
                        }
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- editing

    fn start_edit(&mut self) {
        match self.resource {
            Resource::Keys => self.edit_key(),
            Resource::Streams => {
                if let Some(s) = self.streams.get(self.stream_cursor) {
                    self.prompt_stream_add(s.name.clone());
                }
            }
            Resource::PubSub => self.set_message("cannot edit a pubsub channel".to_string(), MessageKind::Error),
            Resource::Acl => self.set_message("cannot edit an ACL user here".to_string(), MessageKind::Error),
        }
    }

    fn edit_key(&mut self) {
        let Some(info) = self.keys.get(self.key_cursor) else {
            self.set_message("no key selected".to_string(), MessageKind::Error);
            return;
        };
        let name = info.name.clone();
        match info.kind {
            KeyKind::String => {
                let current = match &self.detail_data {
                    Some(ValueData::String(s)) => s.clone(),
                    _ => String::new(),
                };
                self.mode = Mode::Input;
                self.ctx = InputCtx::EditValue { key: name.clone() };
                self.input = current;
                self.input_prompt = format!("new value for '{name}': ");
            }
            KeyKind::Hash => self.prompt_hash_add_field(name),
            KeyKind::List => self.prompt_list_append(name),
            KeyKind::Set => self.prompt_set_add(name),
            KeyKind::ZSet => self.prompt_zadd(name),
            KeyKind::Stream => self.prompt_stream_add(name),
            KeyKind::None | KeyKind::Unknown => {
                self.set_message("unknown key type".to_string(), MessageKind::Error);
            }
        }
    }

    fn start_create(&mut self) {
        if self.focus == Focus::Detail {
            self.start_add_item();
            return;
        }
        self.mode = Mode::Input;
        self.ctx = InputCtx::CreatePickType;
        self.input.clear();
        self.input_prompt = "new key type: 2 string, 3 hash, 4 list, 5 set, 6 zset, 7 stream".to_string();
    }

    fn start_add_item(&mut self) {
        match self.resource {
            Resource::Keys => {
                let Some(info) = self.keys.get(self.key_cursor) else {
                    self.set_message("no key selected".to_string(), MessageKind::Error);
                    return;
                };
                let name = info.name.clone();
                match info.kind {
                    KeyKind::String => {
                        let current = match &self.detail_data {
                            Some(ValueData::String(s)) => s.clone(),
                            _ => String::new(),
                        };
                        self.mode = Mode::Input;
                        self.ctx = InputCtx::EditValue { key: name.clone() };
                        self.input = current;
                        self.input_prompt = format!("value for '{name}': ");
                    }
                    KeyKind::Hash => self.prompt_hash_add_field(name),
                    KeyKind::List => self.prompt_list_append(name),
                    KeyKind::Set => self.prompt_set_add(name),
                    KeyKind::ZSet => self.prompt_zadd(name),
                    KeyKind::Stream => self.prompt_stream_add(name),
                    KeyKind::None | KeyKind::Unknown => {
                        self.set_message("unknown key type".to_string(), MessageKind::Error);
                    }
                }
            }
            Resource::Streams => {
                if let Some(s) = self.streams.get(self.stream_cursor) {
                    self.prompt_stream_add(s.name.clone());
                }
            }
            Resource::PubSub => self.set_message("cannot add to pubsub".to_string(), MessageKind::Error),
            Resource::Acl => self.set_message("cannot add an ACL user here".to_string(), MessageKind::Error),
        }
    }

    fn start_delete(&mut self) {
        match self.resource {
            Resource::Keys => {
                let Some(info) = self.keys.get(self.key_cursor) else {
                    self.set_message("no key selected".to_string(), MessageKind::Error);
                    return;
                };
                let name = info.name.clone();
                if self.focus == Focus::Detail && info.kind != KeyKind::String {
                    self.prompt_delete_item(name, info.kind);
                } else {
                    self.confirm_delete_key(name);
                }
            }
            Resource::Streams => {
                if let Some(s) = self.streams.get(self.stream_cursor) {
                    self.confirm_delete_key(s.name.clone());
                }
            }
            Resource::PubSub => self.set_message("cannot delete a pubsub channel".to_string(), MessageKind::Error),
            Resource::Acl => self.set_message("cannot delete an ACL user here".to_string(), MessageKind::Error),
        }
    }

    fn prompt_delete_item(&mut self, key: String, kind: KeyKind) {
        let prompt = match kind {
            KeyKind::Hash => "field to delete: ",
            KeyKind::List => "element to remove: ",
            KeyKind::Set => "member to remove: ",
            KeyKind::ZSet => "member to remove: ",
            KeyKind::Stream => "entry ID to delete: ",
            _ => {
                self.confirm_delete_key(key);
                return;
            }
        };
        self.mode = Mode::Input;
        self.ctx = InputCtx::DeleteItem { key, kind };
        self.input.clear();
        self.input_prompt = prompt.to_string();
    }

    fn confirm_delete_key(&mut self, key: String) {
        self.mode = Mode::Input;
        self.ctx = InputCtx::Confirm {
            action: ConfirmAction::DeleteKey { key: key.clone() },
        };
        self.input.clear();
        self.input_prompt = format!("delete key '{key}'? (y/N) ");
    }

    fn perform_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::DeleteKey { key } => {
                match self.worker.request(Request::Del { key: key.clone() }) {
                    Ok(_) => {
                        self.set_message(format!("deleted '{key}'"), MessageKind::Info);
                        self.load_resource();
                    }
                    Err(e) => self.set_message(e, MessageKind::Error),
                }
            }
        }
    }

    // --------------------------------------------------------------- prompts

    fn prompt_hash_add_field(&mut self, key: String) {
        self.mode = Mode::Input;
        self.ctx = InputCtx::HashAddField { key };
        self.input.clear();
        self.input_prompt = "field name: ".to_string();
    }

    fn prompt_list_append(&mut self, key: String) {
        self.mode = Mode::Input;
        self.ctx = InputCtx::ListAppend { key };
        self.input.clear();
        self.input_prompt = "element to append: ".to_string();
    }

    fn prompt_set_add(&mut self, key: String) {
        self.mode = Mode::Input;
        self.ctx = InputCtx::SetAdd { key };
        self.input.clear();
        self.input_prompt = "member to add: ".to_string();
    }

    fn prompt_zadd(&mut self, key: String) {
        self.mode = Mode::Input;
        self.ctx = InputCtx::ZAdd { key };
        self.input.clear();
        self.input_prompt = "member=score to add (e.g. alice=3.5): ".to_string();
    }

    fn prompt_stream_add(&mut self, key: String) {
        self.mode = Mode::Input;
        self.ctx = InputCtx::StreamAdd { key };
        self.input.clear();
        self.input_prompt = "field=value pairs (comma-separated): ".to_string();
    }

    fn begin_create_name(&mut self, kind: KeyKind) {
        self.ctx = InputCtx::CreateName { kind };
        self.input.clear();
        self.input_prompt = format!("new {} key name: ", kind.label());
    }

    fn prompt_create_value(&mut self, key: String, kind: KeyKind) {
        let prompt = match kind {
            KeyKind::String => format!("value for '{key}': "),
            KeyKind::Hash => format!("fields for '{key}' (field=value, comma-separated): "),
            KeyKind::List => format!("elements for '{key}' (comma-separated): "),
            KeyKind::Set => format!("members for '{key}' (comma-separated): "),
            KeyKind::ZSet => format!("members for '{key}' (member=score, comma-separated): "),
            KeyKind::Stream => format!("fields for '{key}' (field=value, comma-separated): "),
            _ => format!("value for '{key}': "),
        };
        self.mode = Mode::Input;
        self.ctx = InputCtx::CreateValue { key, kind };
        self.input.clear();
        self.input_prompt = prompt;
    }

    fn do_create(&mut self, key: String, kind: KeyKind, raw: &str) {
        let result = match kind {
            KeyKind::String => self.worker.request(Request::Set {
                key: key.clone(),
                value: raw.to_string(),
            }),
            KeyKind::Hash => {
                let fields = split_pairs(raw);
                self.worker.request(Request::CreateHash { key: key.clone(), fields })
            }
            KeyKind::List => {
                let values = split_csv(raw);
                self.worker.request(Request::CreateList { key: key.clone(), values })
            }
            KeyKind::Set => {
                let members = split_csv(raw);
                self.worker.request(Request::CreateSet { key: key.clone(), members })
            }
            KeyKind::ZSet => {
                let members = split_member_score(raw);
                self.worker.request(Request::CreateZSet { key: key.clone(), members })
            }
            KeyKind::Stream => {
                let fields = split_pairs(raw);
                self.worker.request(Request::CreateStream { key: key.clone(), fields })
            }
            KeyKind::None | KeyKind::Unknown => return,
        };

        match result {
            Ok(_) => {
                self.set_message(format!("created {} key '{key}'", kind.label()), MessageKind::Info);
                self.search.clear();
                self.load_keys();
                if let Some(pos) = self.keys.iter().position(|k| k.name == key) {
                    self.key_cursor = pos;
                }
                self.load_selection();
            }
            Err(e) => self.set_message(e, MessageKind::Error),
        }
    }

    // -------------------------------------------------------------- movement

    fn list_len(&self) -> usize {
        match self.resource {
            Resource::Keys => self.keys.len(),
            Resource::Streams => self.streams.len(),
            Resource::PubSub => self.channels.len(),
            Resource::Acl => self.acl_users.len(),
        }
    }

    fn cursor_index(&self) -> usize {
        match self.resource {
            Resource::Keys => self.key_cursor,
            Resource::Streams => self.stream_cursor,
            Resource::PubSub => self.channel_cursor,
            Resource::Acl => self.acl_cursor,
        }
    }

    fn set_cursor_index(&mut self, i: usize) {
        match self.resource {
            Resource::Keys => self.key_cursor = i,
            Resource::Streams => self.stream_cursor = i,
            Resource::PubSub => self.channel_cursor = i,
            Resource::Acl => self.acl_cursor = i,
        }
    }

    fn move_cursor(&mut self, delta: i32) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let cur = self.cursor_index() as i64 + delta as i64;
        let new = cur.clamp(0, len as i64 - 1) as usize;
        self.set_cursor_index(new);
        self.load_selection();
    }

    fn page_scroll(&mut self, delta: i32) {
        if self.focus == Focus::Detail {
            self.scroll_detail(delta * 20);
        } else {
            self.move_cursor(delta * 20);
        }
    }

    fn goto(&mut self, to_end: bool) {
        if self.focus == Focus::Detail {
            self.detail_scroll = if to_end { usize::MAX } else { 0 };
        } else {
            let len = self.list_len();
            if len == 0 {
                return;
            }
            let idx = if to_end { len - 1 } else { 0 };
            self.set_cursor_index(idx);
            self.load_selection();
        }
    }

    fn scroll_detail(&mut self, delta: i32) {
        self.detail_scroll = self.detail_scroll.saturating_add_signed(delta as isize);
    }

    fn set_message(&mut self, msg: String, kind: MessageKind) {
        self.message = Some((msg, kind));
    }
}

fn kind_from_create_digit(c: char) -> Option<KeyKind> {
    match c {
        '2' => Some(KeyKind::String),
        '3' => Some(KeyKind::Hash),
        '4' => Some(KeyKind::List),
        '5' => Some(KeyKind::Set),
        '6' => Some(KeyKind::ZSet),
        '7' => Some(KeyKind::Stream),
        _ => None,
    }
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn split_pairs(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter_map(|p| {
            let p = p.trim();
            let (k, v) = p.split_once('=')?;
            let k = k.trim();
            let v = v.trim();
            if k.is_empty() {
                None
            } else {
                Some((k.to_string(), v.to_string()))
            }
        })
        .collect()
}

fn split_member_score(s: &str) -> Vec<(String, f64)> {
    s.split(',')
        .filter_map(|p| {
            let p = p.trim();
            let (m, sc) = p.split_once('=')?;
            let m = m.trim();
            let sc = sc.trim();
            if m.is_empty() {
                return None;
            }
            let score = sc.parse::<f64>().ok()?;
            Some((m.to_string(), score))
        })
        .collect()
}
