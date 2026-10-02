use crate::redis_client::{RedisClient, RedisError, Resp};
use serde_json::{json, Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Keys,
    Streams,
    PubSub,
    Acl,
}
impl Resource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Keys => ":keys",
            Self::Streams => ":streams",
            Self::PubSub => ":pubsub",
            Self::Acl => ":acl",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyType {
    All,
    String,
    Hash,
    List,
    Set,
    ZSet,
    Stream,
    Other,
}
impl KeyType {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::String => "string",
            Self::Hash => "hash",
            Self::List => "list",
            Self::Set => "set",
            Self::ZSet => "zset",
            Self::Stream => "stream",
            Self::Other => "other",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::String,
            Self::String => Self::Hash,
            Self::Hash => Self::List,
            Self::List => Self::Set,
            Self::Set => Self::ZSet,
            Self::ZSet => Self::Stream,
            Self::Stream => Self::Other,
            Self::Other => Self::All,
        }
    }
    pub fn from_redis(s: &str) -> Self {
        match s {
            "string" => Self::String,
            "hash" => Self::Hash,
            "list" => Self::List,
            "set" => Self::Set,
            "zset" => Self::ZSet,
            "stream" => Self::Stream,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct KeyInfo {
    pub name: String,
    pub kind: KeyType,
    pub ttl: i64,
}
#[derive(Clone, Debug)]
pub struct Server {
    pub name: String,
    pub uri: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormStage {
    Type,
    Key,
    Data,
}
#[derive(Clone, Debug)]
pub enum InputMode {
    Normal,
    Search {
        before: String,
    },
    Command,
    AddServerName,
    AddServerUri {
        name: String,
    },
    Create {
        stage: FormStage,
        kind: KeyType,
        key: String,
    },
    Edit {
        kind: KeyType,
        key: String,
    },
    DeleteConfirm {
        key: String,
    },
}

pub struct App {
    pub servers: Vec<Server>,
    pub server_selected: usize,
    pub connected: Option<usize>,
    pub client: Option<RedisClient>,
    pub resource: Resource,
    pub keys: Vec<KeyInfo>,
    pub items: Vec<String>,
    pub selected: usize,
    pub detail: Vec<String>,
    pub detail_scroll: u16,
    pub search: String,
    pub type_filter: KeyType,
    pub input: String,
    pub cursor: usize,
    pub mode: InputMode,
    pub focus: Focus,
    pub status: String,
    pub should_quit: bool,
    pub show_help: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            servers: vec![Server {
                name: "Local Redis".into(),
                uri: "redis://localhost:6379/0".into(),
            }],
            server_selected: 0,
            connected: None,
            client: None,
            resource: Resource::Keys,
            keys: vec![],
            items: vec![],
            selected: 0,
            detail: vec!["Select a server and press Enter to connect.".into()],
            detail_scroll: 0,
            search: String::new(),
            type_filter: KeyType::All,
            input: String::new(),
            cursor: 0,
            mode: InputMode::Normal,
            focus: Focus::List,
            status: "Ready".into(),
            should_quit: false,
            show_help: false,
        }
    }
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }
    pub fn selected_name(&self) -> Option<&str> {
        self.items.get(self.selected).map(String::as_str)
    }
    pub fn connect_selected(&mut self) {
        let Some(server) = self.servers.get(self.server_selected).cloned() else {
            return;
        };
        match RedisClient::connect(&server.uri) {
            Ok(c) => {
                self.client = Some(c);
                self.connected = Some(self.server_selected);
                self.resource = Resource::Keys;
                self.status = format!("Connected to {}", server.name);
                self.refresh();
            }
            Err(e) => self.error(e),
        }
    }
    pub fn disconnect(&mut self) {
        self.client = None;
        self.connected = None;
        self.items.clear();
        self.keys.clear();
        self.detail = vec!["Select a server and press Enter to connect.".into()];
        self.selected = 0;
        self.status = "Disconnected".into();
    }
    fn cmd(&mut self, args: &[&str]) -> Result<Resp, RedisError> {
        self.client
            .as_mut()
            .ok_or_else(|| RedisError("not connected".into()))?
            .command(args)
    }
    pub fn set_resource(&mut self, resource: Resource) {
        self.resource = resource;
        self.selected = 0;
        self.detail_scroll = 0;
        self.search.clear();
        self.refresh();
    }
    pub fn refresh(&mut self) {
        if !self.is_connected() {
            return;
        }
        let result = match self.resource {
            Resource::Keys => self.load_keys(),
            Resource::Streams => self.load_streams(),
            Resource::PubSub => self.load_pubsub(),
            Resource::Acl => self.load_acl(),
        };
        match result {
            Ok(()) => {
                self.clamp_selection();
                self.load_detail();
                self.status = format!("Refreshed {}", self.resource.label());
            }
            Err(e) => self.error(e),
        }
    }
    fn scan_keys(&mut self) -> Result<Vec<String>, RedisError> {
        let mut cursor = "0".to_string();
        let mut keys = Vec::new();
        loop {
            let response = self
                .cmd(&["SCAN", &cursor, "COUNT", "1000"])?
                .into_array()?;
            if response.len() != 2 {
                return Err(RedisError("unexpected SCAN response".into()));
            }
            cursor = response[0].text();
            keys.extend(
                response[1]
                    .clone()
                    .into_array()?
                    .into_iter()
                    .map(|r| r.text()),
            );
            if cursor == "0" {
                break;
            }
        }
        keys.sort();
        Ok(keys)
    }
    fn load_keys(&mut self) -> Result<(), RedisError> {
        let names = self.scan_keys()?;
        let mut infos = Vec::with_capacity(names.len());
        for name in names {
            let kind = KeyType::from_redis(&self.cmd(&["TYPE", &name])?.text());
            let ttl = self.cmd(&["PTTL", &name])?.text().parse().unwrap_or(-1);
            infos.push(KeyInfo { name, kind, ttl });
        }
        self.keys = infos;
        self.apply_filter();
        Ok(())
    }
    fn load_streams(&mut self) -> Result<(), RedisError> {
        let names = self.scan_keys()?;
        let mut streams = Vec::new();
        for name in names {
            if self.cmd(&["TYPE", &name])?.text() == "stream" {
                streams.push(name);
            }
        }
        self.items = streams
            .into_iter()
            .filter(|s| contains_ci(s, &self.search))
            .collect();
        Ok(())
    }
    fn load_pubsub(&mut self) -> Result<(), RedisError> {
        self.items = self
            .cmd(&["PUBSUB", "CHANNELS"])?
            .into_array()?
            .into_iter()
            .map(|r| r.text())
            .filter(|s| contains_ci(s, &self.search))
            .collect();
        self.items.sort();
        Ok(())
    }
    fn load_acl(&mut self) -> Result<(), RedisError> {
        self.items = self
            .cmd(&["ACL", "USERS"])?
            .into_array()?
            .into_iter()
            .map(|r| r.text())
            .filter(|s| contains_ci(s, &self.search))
            .collect();
        self.items.sort();
        Ok(())
    }
    pub fn apply_filter(&mut self) {
        self.items = self
            .keys
            .iter()
            .filter(|k| {
                (self.type_filter == KeyType::All || k.kind == self.type_filter)
                    && contains_ci(&k.name, &self.search)
            })
            .map(|k| k.name.clone())
            .collect();
        self.clamp_selection();
        self.load_detail();
    }
    fn clamp_selection(&mut self) {
        if self.items.is_empty() {
            self.selected = 0
        } else if self.selected >= self.items.len() {
            self.selected = self.items.len() - 1
        }
    }
    pub fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let max = self.items.len() - 1;
        self.selected = if delta < 0 {
            self.selected.saturating_sub((-delta) as usize)
        } else {
            (self.selected + delta as usize).min(max)
        };
        self.detail_scroll = 0;
        self.load_detail();
    }
    pub fn load_detail(&mut self) {
        let Some(name) = self.selected_name().map(str::to_owned) else {
            self.detail = vec![match self.resource {
                Resource::Keys => "No keys match the current filter.",
                Resource::Streams => "No streams on this Redis database.",
                Resource::PubSub => "No active Pub/Sub channels.",
                Resource::Acl => "No ACL users returned.",
            }
            .into()];
            return;
        };
        let result = match self.resource {
            Resource::Keys => self.key_detail(&name),
            Resource::Streams => self.stream_detail(&name),
            Resource::PubSub => self.pubsub_detail(&name),
            Resource::Acl => self.acl_detail(&name),
        };
        match result {
            Ok(lines) => self.detail = lines,
            Err(e) => self.detail = vec![format!("Error: {e}")],
        }
    }
    fn key_detail(&mut self, name: &str) -> Result<Vec<String>, RedisError> {
        let kind = KeyType::from_redis(&self.cmd(&["TYPE", name])?.text());
        let ttl = self.cmd(&["PTTL", name])?.text();
        let mut out = vec![
            format!("Key: {name}"),
            format!("Type: {}    TTL: {}", kind.label(), format_ttl(&ttl)),
            "".into(),
        ];
        match kind {
            KeyType::String => {
                out.push("Value:".into());
                out.extend(split_display(&self.cmd(&["GET", name])?.text()));
            }
            KeyType::Hash => {
                out.push("Fields:".into());
                let a = self.cmd(&["HGETALL", name])?.into_array()?;
                for pair in a.chunks(2) {
                    if pair.len() == 2 {
                        out.push(format!("  {} = {}", pair[0].text(), pair[1].text()));
                    }
                }
            }
            KeyType::List => {
                out.push("Elements:".into());
                for (i, v) in self
                    .cmd(&["LRANGE", name, "0", "-1"])?
                    .into_array()?
                    .iter()
                    .enumerate()
                {
                    out.push(format!("  [{i}] {}", v.text()));
                }
            }
            KeyType::Set => {
                out.push("Members:".into());
                let mut x = self
                    .cmd(&["SMEMBERS", name])?
                    .into_array()?
                    .into_iter()
                    .map(|r| r.text())
                    .collect::<Vec<_>>();
                x.sort();
                for v in x {
                    out.push(format!("  • {v}"));
                }
            }
            KeyType::ZSet => {
                out.push("Members (ascending score):".into());
                let a = self
                    .cmd(&["ZRANGE", name, "0", "-1", "WITHSCORES"])?
                    .into_array()?;
                for p in a.chunks(2) {
                    if p.len() == 2 {
                        out.push(format!("  {}    score={}", p[0].text(), p[1].text()));
                    }
                }
            }
            KeyType::Stream => return self.stream_detail_with_header(name, out),
            _ => out.push("This Redis type is not editable in toolj.".into()),
        }
        Ok(out)
    }
    fn stream_detail(&mut self, name: &str) -> Result<Vec<String>, RedisError> {
        self.stream_detail_with_header(name, vec![format!("Stream: {name}"), "".into()])
    }
    fn stream_detail_with_header(
        &mut self,
        name: &str,
        mut out: Vec<String>,
    ) -> Result<Vec<String>, RedisError> {
        out.push("Messages:".into());
        let messages = self.cmd(&["XRANGE", name, "-", "+"])?.into_array()?;
        for msg in messages {
            let p = msg.into_array()?;
            if p.len() < 2 {
                continue;
            }
            out.push(format!("  ID {}", p[0].text()));
            let fields = p[1].clone().into_array()?;
            for pair in fields.chunks(2) {
                if pair.len() == 2 {
                    out.push(format!("    {} = {}", pair[0].text(), pair[1].text()));
                }
            }
        }
        Ok(out)
    }
    fn pubsub_detail(&mut self, name: &str) -> Result<Vec<String>, RedisError> {
        let n = self.cmd(&["PUBSUB", "NUMSUB", name])?.into_array()?;
        Ok(vec![
            format!("Channel: {name}"),
            "".into(),
            format!(
                "Subscribers: {}",
                n.get(1).map(Resp::text).unwrap_or_default()
            ),
            "".into(),
            "PUBSUB CHANNELS shows channels with at least one active subscriber.".into(),
        ])
    }
    fn acl_detail(&mut self, name: &str) -> Result<Vec<String>, RedisError> {
        let response = self.cmd(&["ACL", "GETUSER", name])?;
        let mut out = vec![format!("ACL user: {name}"), "".into()];
        let a = response.into_array()?;
        for p in a.chunks(2) {
            if p.len() == 2 {
                out.push(format!("{}:", p[0].text()));
                match &p[1] {
                    Resp::Array(Some(v)) => {
                        for x in v {
                            out.push(format!("  {}", x.text()))
                        }
                    }
                    x => out.push(format!("  {}", x.text())),
                }
            }
        }
        Ok(out)
    }
    pub fn cycle_type(&mut self) {
        self.type_filter = self.type_filter.next();
        self.selected = 0;
        self.apply_filter();
        self.status = format!("Type filter: {}", self.type_filter.label());
    }
    pub fn begin_input(&mut self, mode: InputMode, initial: String) {
        self.mode = mode;
        self.input = initial;
        self.cursor = self.input.len();
    }
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        if matches!(self.mode, InputMode::Search { .. }) {
            self.search = self.input.clone();
            self.apply_search();
        }
    }
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.input[..self.cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.input.replace_range(prev..self.cursor, "");
        self.cursor = prev;
        if matches!(self.mode, InputMode::Search { .. }) {
            self.search = self.input.clone();
            self.apply_search();
        }
    }
    pub fn delete_char(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        let next = self.input[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.input.len());
        self.input.replace_range(self.cursor..next, "");
        if matches!(self.mode, InputMode::Search { .. }) {
            self.search = self.input.clone();
            self.apply_search();
        }
    }
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.input[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0)
        }
    }
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.input.len())
        }
    }
    fn apply_search(&mut self) {
        self.selected = 0;
        match self.resource {
            Resource::Keys => self.apply_filter(),
            _ => self.refresh(),
        }
    }
    pub fn cancel_input(&mut self) {
        if let InputMode::Search { before } = self.mode.clone() {
            self.search = before;
            self.apply_search();
        }
        self.mode = InputMode::Normal;
        self.input.clear();
        self.status = "Cancelled".into();
    }
    pub fn submit_input(&mut self) {
        let text = self.input.clone();
        let mode = self.mode.clone();
        match mode {
            InputMode::Search { .. } => {
                self.mode = InputMode::Normal;
                self.status = format!(
                    "Filter: {}",
                    if self.search.is_empty() {
                        "(none)"
                    } else {
                        &self.search
                    }
                );
            }
            InputMode::Command => {
                self.mode = InputMode::Normal;
                let c = text.trim().trim_start_matches(':');
                match c {
                    "keys" => self.set_resource(Resource::Keys),
                    "streams" => self.set_resource(Resource::Streams),
                    "pubsub" => self.set_resource(Resource::PubSub),
                    "acl" => self.set_resource(Resource::Acl),
                    _ => self.status = format!("Unknown resource :{c}"),
                }
            }
            InputMode::AddServerName => {
                if text.trim().is_empty() {
                    self.status = "Server name cannot be empty".into()
                } else {
                    self.begin_input(
                        InputMode::AddServerUri {
                            name: text.trim().into(),
                        },
                        "redis://localhost:6379/0".into(),
                    );
                }
            }
            InputMode::AddServerUri { name } => {
                match crate::redis_client::RedisUri::parse(text.trim()) {
                    Ok(_) => {
                        self.servers.push(Server {
                            name,
                            uri: text.trim().into(),
                        });
                        self.server_selected = self.servers.len() - 1;
                        self.mode = InputMode::Normal;
                        self.input.clear();
                        self.status = "Server added; press Enter to connect".into()
                    }
                    Err(e) => self.error(e),
                }
            }
            InputMode::Create {
                stage: FormStage::Type,
                kind,
                key,
            } => {
                let _ = (kind, key);
            }
            InputMode::Create {
                stage: FormStage::Key,
                kind,
                ..
            } => {
                if text.is_empty() {
                    self.status = "Key name cannot be empty".into()
                } else {
                    self.begin_input(
                        InputMode::Create {
                            stage: FormStage::Data,
                            kind,
                            key: text,
                        },
                        default_data(kind),
                    );
                }
            }
            InputMode::Create {
                stage: FormStage::Data,
                kind,
                key,
            } => match self.write_value(&key, kind, &text, false) {
                Ok(()) => {
                    self.mode = InputMode::Normal;
                    self.input.clear();
                    self.status = format!("Created {key}");
                    self.refresh()
                }
                Err(e) => self.error(e),
            },
            InputMode::Edit { kind, key } => match self.write_value(&key, kind, &text, true) {
                Ok(()) => {
                    self.mode = InputMode::Normal;
                    self.input.clear();
                    self.status = format!("Updated {key}");
                    self.refresh()
                }
                Err(e) => self.error(e),
            },
            _ => {}
        }
    }
    pub fn create_cycle(&mut self, reverse: bool) {
        if let InputMode::Create {
            stage: FormStage::Type,
            kind,
            ..
        } = self.mode.clone()
        {
            let kinds = [
                KeyType::String,
                KeyType::Hash,
                KeyType::List,
                KeyType::Set,
                KeyType::ZSet,
                KeyType::Stream,
            ];
            let i = kinds.iter().position(|x| *x == kind).unwrap_or(0);
            let n = if reverse {
                (i + kinds.len() - 1) % kinds.len()
            } else {
                (i + 1) % kinds.len()
            };
            self.mode = InputMode::Create {
                stage: FormStage::Type,
                kind: kinds[n],
                key: String::new(),
            };
        }
    }
    pub fn create_accept_type(&mut self) {
        if let InputMode::Create {
            stage: FormStage::Type,
            kind,
            ..
        } = self.mode.clone()
        {
            self.begin_input(
                InputMode::Create {
                    stage: FormStage::Key,
                    kind,
                    key: String::new(),
                },
                String::new(),
            );
        }
    }
    pub fn begin_edit(&mut self) {
        let Some(key) = self.selected_name().map(str::to_owned) else {
            return;
        };
        let kind = if self.resource == Resource::Streams {
            KeyType::Stream
        } else {
            self.keys
                .iter()
                .find(|k| k.name == key)
                .map(|k| k.kind)
                .unwrap_or(KeyType::Other)
        };
        if !matches!(
            kind,
            KeyType::String
                | KeyType::Hash
                | KeyType::List
                | KeyType::Set
                | KeyType::ZSet
                | KeyType::Stream
        ) {
            self.status = "This type is not editable".into();
            return;
        }
        match self.editable_value(&key, kind) {
            Ok(v) => self.begin_input(InputMode::Edit { kind, key }, v),
            Err(e) => self.error(e),
        }
    }
    fn editable_value(&mut self, key: &str, kind: KeyType) -> Result<String, RedisError> {
        Ok(match kind {
            KeyType::String => self.cmd(&["GET", key])?.text(),
            KeyType::Hash => {
                let a = self.cmd(&["HGETALL", key])?.into_array()?;
                let mut m = Map::new();
                for p in a.chunks(2) {
                    if p.len() == 2 {
                        m.insert(p[0].text(), Value::String(p[1].text()));
                    }
                }
                Value::Object(m).to_string()
            }
            KeyType::List => Value::Array(
                self.cmd(&["LRANGE", key, "0", "-1"])?
                    .into_array()?
                    .into_iter()
                    .map(|r| Value::String(r.text()))
                    .collect(),
            )
            .to_string(),
            KeyType::Set => Value::Array(
                self.cmd(&["SMEMBERS", key])?
                    .into_array()?
                    .into_iter()
                    .map(|r| Value::String(r.text()))
                    .collect(),
            )
            .to_string(),
            KeyType::ZSet => {
                let a = self
                    .cmd(&["ZRANGE", key, "0", "-1", "WITHSCORES"])?
                    .into_array()?;
                let mut m = Map::new();
                for p in a.chunks(2) {
                    if p.len() == 2 {
                        m.insert(
                            p[0].text(),
                            json!(p[1].text().parse::<f64>().unwrap_or(0.0)),
                        );
                    }
                }
                Value::Object(m).to_string()
            }
            KeyType::Stream => "{}".into(),
            _ => String::new(),
        })
    }
    fn write_value(
        &mut self,
        key: &str,
        kind: KeyType,
        data: &str,
        replace: bool,
    ) -> Result<(), RedisError> {
        if !replace && self.cmd(&["EXISTS", key])?.text() != "0" {
            return Err(RedisError(format!(
                "key {key:?} already exists; select it and use edit"
            )));
        }
        let old_ttl = if replace {
            self.cmd(&["PTTL", key])?
                .text()
                .parse::<i64>()
                .unwrap_or(-1)
        } else {
            -1
        };
        let mut commands: Vec<Vec<String>> = Vec::new();
        match kind {
            KeyType::String => {
                commands.push(vec!["SET".into(), key.into(), data.into()]);
            }
            KeyType::Hash => {
                let obj = parse_object(data)?;
                if obj.is_empty() {
                    return Err(RedisError("hash JSON object cannot be empty".into()));
                }
                commands.push(vec!["DEL".into(), key.into()]);
                let mut c = vec!["HSET".into(), key.into()];
                for (k, v) in obj {
                    c.push(k);
                    c.push(json_scalar(v)?);
                }
                commands.push(c);
            }
            KeyType::List => {
                let arr = parse_array(data)?;
                if arr.is_empty() {
                    return Err(RedisError("list JSON array cannot be empty".into()));
                }
                commands.push(vec!["DEL".into(), key.into()]);
                let mut c = vec!["RPUSH".into(), key.into()];
                for v in arr {
                    c.push(json_scalar(v)?)
                }
                commands.push(c);
            }
            KeyType::Set => {
                let arr = parse_array(data)?;
                if arr.is_empty() {
                    return Err(RedisError("set JSON array cannot be empty".into()));
                }
                commands.push(vec!["DEL".into(), key.into()]);
                let mut c = vec!["SADD".into(), key.into()];
                for v in arr {
                    c.push(json_scalar(v)?)
                }
                commands.push(c);
            }
            KeyType::ZSet => {
                let obj = parse_object(data)?;
                if obj.is_empty() {
                    return Err(RedisError("zset JSON object cannot be empty".into()));
                }
                commands.push(vec!["DEL".into(), key.into()]);
                let mut c = vec!["ZADD".into(), key.into()];
                for (member, score) in obj {
                    let n = score
                        .as_f64()
                        .ok_or_else(|| RedisError("zset scores must be JSON numbers".into()))?;
                    c.push(n.to_string());
                    c.push(member);
                }
                commands.push(c);
            }
            KeyType::Stream => {
                let obj = parse_object(data)?;
                if obj.is_empty() {
                    return Err(RedisError("stream message cannot be empty".into()));
                }
                let mut c = vec!["XADD".into(), key.into(), "*".into()];
                for (k, v) in obj {
                    c.push(k);
                    c.push(json_scalar(v)?)
                }
                commands.push(c);
            }
            _ => return Err(RedisError("unsupported key type".into())),
        }
        for c in commands {
            let refs = c.iter().map(String::as_str).collect::<Vec<_>>();
            self.cmd(&refs)?;
        }
        if replace && old_ttl > 0 && kind != KeyType::Stream {
            self.cmd(&["PEXPIRE", key, &old_ttl.to_string()])?;
        }
        Ok(())
    }
    pub fn delete_selected(&mut self) {
        if let Some(key) = self.selected_name().map(str::to_owned) {
            self.mode = InputMode::DeleteConfirm { key };
        }
    }
    pub fn confirm_delete(&mut self, yes: bool) {
        if let InputMode::DeleteConfirm { key } = self.mode.clone() {
            self.mode = InputMode::Normal;
            if yes {
                match self.cmd(&["DEL", &key]) {
                    Ok(_) => {
                        self.status = format!("Deleted {key}");
                        self.refresh()
                    }
                    Err(e) => self.error(e),
                }
            } else {
                self.status = "Delete cancelled".into()
            }
        }
    }
    fn error(&mut self, e: RedisError) {
        self.status = format!("Error: {e}");
    }
    pub fn total_keys(&self) -> usize {
        self.keys.len()
    }
}
fn contains_ci(s: &str, q: &str) -> bool {
    s.to_lowercase().contains(&q.to_lowercase())
}
fn format_ttl(s: &str) -> String {
    match s.parse::<i64>().unwrap_or(-2) {
        -1 => "persistent".into(),
        -2 => "missing".into(),
        n => format!("{n} ms"),
    }
}
fn split_display(s: &str) -> Vec<String> {
    if s.is_empty() {
        vec!["  (empty string)".into()]
    } else {
        s.lines().map(|x| format!("  {x}")).collect()
    }
}
fn default_data(k: KeyType) -> String {
    match k {
        KeyType::String => String::new(),
        KeyType::Hash | KeyType::ZSet | KeyType::Stream => "{}".into(),
        KeyType::List | KeyType::Set => "[]".into(),
        _ => String::new(),
    }
}
fn parse_object(s: &str) -> Result<Map<String, Value>, RedisError> {
    serde_json::from_str::<Value>(s)
        .map_err(|e| RedisError(format!("invalid JSON: {e}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| RedisError("expected a JSON object".into()))
}
fn parse_array(s: &str) -> Result<Vec<Value>, RedisError> {
    serde_json::from_str::<Value>(s)
        .map_err(|e| RedisError(format!("invalid JSON: {e}")))?
        .as_array()
        .cloned()
        .ok_or_else(|| RedisError("expected a JSON array".into()))
}
fn json_scalar(v: Value) -> Result<String, RedisError> {
    match v {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok("null".into()),
        _ => Err(RedisError(
            "values must be JSON strings, numbers, booleans, or null".into(),
        )),
    }
}
