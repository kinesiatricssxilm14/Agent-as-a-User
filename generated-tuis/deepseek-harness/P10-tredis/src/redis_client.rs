//! Background Redis worker.
//!
//! All Redis commands are executed on a dedicated thread so that the TUI event
//! loop stays responsive. Requests are sent over an mpsc channel and each one
//! carries its own reply channel.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use redis::{Commands, Connection};

pub type WorkerResult = Result<Payload, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    String,
    Hash,
    List,
    Set,
    ZSet,
    Stream,
    None,
    Unknown,
}

impl KeyKind {
    pub fn from_type(s: &str) -> Self {
        match s {
            "string" => KeyKind::String,
            "hash" => KeyKind::Hash,
            "list" => KeyKind::List,
            "set" => KeyKind::Set,
            "zset" => KeyKind::ZSet,
            "stream" => KeyKind::Stream,
            "none" => KeyKind::None,
            _ => KeyKind::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            KeyKind::String => "string",
            KeyKind::Hash => "hash",
            KeyKind::List => "list",
            KeyKind::Set => "set",
            KeyKind::ZSet => "zset",
            KeyKind::Stream => "stream",
            KeyKind::None => "none",
            KeyKind::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub name: String,
    pub kind: KeyKind,
}

#[derive(Debug, Clone)]
pub struct StreamEntry {
    pub id: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ChannelInfo {
    pub name: String,
    pub subscribers: i64,
}

#[derive(Debug, Clone)]
pub enum ValueData {
    String(String),
    Hash(Vec<(String, String)>),
    List(Vec<String>),
    Set(Vec<String>),
    ZSet(Vec<(String, f64)>),
    Stream(Vec<StreamEntry>),
    None,
}

#[derive(Debug)]
pub enum Request {
    Connect { uri: String },
    Disconnect,
    DbSize,
    ScanKeys { limit: usize },
    GetValue { key: String },
    Set { key: String, value: String },
    Del { key: String },
    HSet { key: String, field: String, value: String },
    HDel { key: String, field: String },
    RPush { key: String, value: String },
    LRem { key: String, value: String },
    SAdd { key: String, member: String },
    SRem { key: String, member: String },
    ZAdd { key: String, member: String, score: f64 },
    ZRem { key: String, member: String },
    XAdd { key: String, fields: Vec<(String, String)> },
    XDel { key: String, id: String },
    CreateHash { key: String, fields: Vec<(String, String)> },
    CreateList { key: String, values: Vec<String> },
    CreateSet { key: String, members: Vec<String> },
    CreateZSet { key: String, members: Vec<(String, f64)> },
    CreateStream { key: String, fields: Vec<(String, String)> },
    PubSubChannels,
    AclUsers,
    AclGetUser { name: String },
}

#[derive(Debug)]
pub enum Payload {
    Connected { version: String },
    Disconnected,
    DbSize(u64),
    Keys(Vec<KeyInfo>),
    Value(ValueData),
    Channels(Vec<ChannelInfo>),
    AclUsers(Vec<String>),
    AclUser(Vec<String>),
    Ack,
}

/// A lightweight handle to the background worker thread.
pub struct RedisClient {
    tx: Sender<(Request, Sender<WorkerResult>)>,
}

impl RedisClient {
    pub fn spawn() -> Self {
        let (tx, rx) = channel::<(Request, Sender<WorkerResult>)>();
        std::thread::spawn(move || worker_loop(rx));
        RedisClient { tx }
    }

    /// Send a request and block for its reply (bounded so a stuck command
    /// cannot hang the UI forever).
    pub fn request(&self, req: Request) -> WorkerResult {
        let (rtx, rrx) = channel::<WorkerResult>();
        self.tx
            .send((req, rtx))
            .map_err(|e| format!("worker channel closed: {e}"))?;
        rrx.recv_timeout(Duration::from_secs(30))
            .map_err(|e| format!("request timed out: {e}"))?
    }
}

fn worker_loop(rx: Receiver<(Request, Sender<WorkerResult>)>) {
    let mut conn: Option<Connection> = None;
    while let Ok((req, reply)) = rx.recv() {
        let result = handle(&mut conn, req);
        let _ = reply.send(result);
    }
}

fn handle(conn: &mut Option<Connection>, req: Request) -> WorkerResult {
    match req {
        Request::Connect { uri } => {
            let client = redis::Client::open(uri.as_str()).map_err(|e| format!("invalid URI: {e}"))?;
            let mut c = client
                .get_connection()
                .map_err(|e| format!("connection failed: {e}"))?;
            let _: String = redis::cmd("PING")
                .query(&mut c)
                .map_err(|e| format!("ping failed: {e}"))?;
            let info: String = redis::cmd("INFO").arg("server").query(&mut c).unwrap_or_default();
            let version = extract_version(&info);
            *conn = Some(c);
            Ok(Payload::Connected { version })
        }
        Request::Disconnect => {
            *conn = None;
            Ok(Payload::Disconnected)
        }
        other => {
            let c = conn
                .as_mut()
                .ok_or_else(|| "not connected to a Redis server".to_string())?;
            handle_with_conn(c, other)
        }
    }
}

fn handle_with_conn(c: &mut Connection, req: Request) -> WorkerResult {
    match req {
        Request::Connect { .. } | Request::Disconnect => unreachable!(),

        Request::DbSize => {
            let n: u64 = redis::cmd("DBSIZE").query(c).map_err(re)?;
            Ok(Payload::DbSize(n))
        }

        Request::ScanKeys { limit } => {
            let mut names: Vec<String> = Vec::new();
            {
                let iter: redis::Iter<'_, String> = c.scan_match("*").map_err(re)?;
                for key in iter.take(limit) {
                    names.push(key);
                }
            }
            let mut keys = Vec::with_capacity(names.len());
            for name in names {
                let t: String = redis::cmd("TYPE").arg(&name).query(c).map_err(re)?;
                keys.push(KeyInfo {
                    kind: KeyKind::from_type(&t),
                    name,
                });
            }
            Ok(Payload::Keys(keys))
        }

        Request::GetValue { key } => {
            let t: String = redis::cmd("TYPE").arg(&key).query(c).map_err(re)?;
            let kind = KeyKind::from_type(&t);
            let v = match kind {
                KeyKind::String => {
                    let val: Option<String> = redis::cmd("GET").arg(&key).query(c).map_err(re)?;
                    ValueData::String(val.unwrap_or_default())
                }
                KeyKind::Hash => {
                    let flat: Vec<String> = redis::cmd("HGETALL").arg(&key).query(c).map_err(re)?;
                    ValueData::Hash(pairs(flat))
                }
                KeyKind::List => {
                    let items: Vec<String> =
                        redis::cmd("LRANGE").arg(&key).arg(0).arg(-1).query(c).map_err(re)?;
                    ValueData::List(items)
                }
                KeyKind::Set => {
                    let items: Vec<String> = redis::cmd("SMEMBERS").arg(&key).query(c).map_err(re)?;
                    ValueData::Set(items)
                }
                KeyKind::ZSet => {
                    let flat: Vec<String> = redis::cmd("ZRANGE")
                        .arg(&key)
                        .arg(0)
                        .arg(-1)
                        .arg("WITHSCORES")
                        .query(c)
                        .map_err(re)?;
                    ValueData::ZSet(parse_zset(flat))
                }
                KeyKind::Stream => ValueData::Stream(read_stream(c, &key)?),
                KeyKind::None => ValueData::None,
                KeyKind::Unknown => return Err(format!("unknown redis type: {t}")),
            };
            Ok(Payload::Value(v))
        }

        Request::Set { key, value } => {
            redis::cmd("SET").arg(&key).arg(&value).query::<redis::Value>(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::Del { key } => {
            let _: u64 = redis::cmd("DEL").arg(&key).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::HSet { key, field, value } => {
            let _: u64 = redis::cmd("HSET").arg(&key).arg(&field).arg(&value).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::HDel { key, field } => {
            let _: u64 = redis::cmd("HDEL").arg(&key).arg(&field).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::RPush { key, value } => {
            let _: u64 = redis::cmd("RPUSH").arg(&key).arg(&value).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::LRem { key, value } => {
            let _: u64 = redis::cmd("LREM").arg(&key).arg(0).arg(&value).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::SAdd { key, member } => {
            let _: u64 = redis::cmd("SADD").arg(&key).arg(&member).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::SRem { key, member } => {
            let _: u64 = redis::cmd("SREM").arg(&key).arg(&member).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::ZAdd { key, member, score } => {
            let _: u64 = redis::cmd("ZADD").arg(&key).arg(score).arg(&member).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::ZRem { key, member } => {
            let _: u64 = redis::cmd("ZREM").arg(&key).arg(&member).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::XAdd { key, fields } => {
            if fields.is_empty() {
                return Err("no fields provided".to_string());
            }
            let mut cmd = redis::cmd("XADD");
            cmd.arg(&key).arg("*");
            for (f, v) in &fields {
                cmd.arg(f).arg(v);
            }
            let _: String = cmd.query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::XDel { key, id } => {
            let _: u64 = redis::cmd("XDEL").arg(&key).arg(&id).query(c).map_err(re)?;
            Ok(Payload::Ack)
        }

        Request::CreateHash { key, fields } => {
            if fields.is_empty() {
                return Err("no fields provided".to_string());
            }
            let mut cmd = redis::cmd("HSET");
            cmd.arg(&key);
            for (f, v) in &fields {
                cmd.arg(f).arg(v);
            }
            let _: u64 = cmd.query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::CreateList { key, values } => {
            if values.is_empty() {
                return Err("no elements provided".to_string());
            }
            let mut cmd = redis::cmd("RPUSH");
            cmd.arg(&key);
            for v in &values {
                cmd.arg(v);
            }
            let _: u64 = cmd.query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::CreateSet { key, members } => {
            if members.is_empty() {
                return Err("no members provided".to_string());
            }
            let mut cmd = redis::cmd("SADD");
            cmd.arg(&key);
            for m in &members {
                cmd.arg(m);
            }
            let _: u64 = cmd.query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::CreateZSet { key, members } => {
            if members.is_empty() {
                return Err("no members provided".to_string());
            }
            let mut cmd = redis::cmd("ZADD");
            cmd.arg(&key);
            for (m, s) in &members {
                cmd.arg(*s).arg(m);
            }
            let _: u64 = cmd.query(c).map_err(re)?;
            Ok(Payload::Ack)
        }
        Request::CreateStream { key, fields } => {
            if fields.is_empty() {
                return Err("no fields provided".to_string());
            }
            let mut cmd = redis::cmd("XADD");
            cmd.arg(&key).arg("*");
            for (f, v) in &fields {
                cmd.arg(f).arg(v);
            }
            let _: String = cmd.query(c).map_err(re)?;
            Ok(Payload::Ack)
        }

        Request::PubSubChannels => {
            let names: Vec<String> = redis::cmd("PUBSUB").arg("CHANNELS").query(c).map_err(re)?;
            let mut map: HashMap<String, i64> = HashMap::new();
            if !names.is_empty() {
                let mut cmd = redis::cmd("PUBSUB");
                cmd.arg("NUMSUB");
                for n in &names {
                    cmd.arg(n);
                }
                let v: redis::Value = cmd.query(c).map_err(re)?;
                if let redis::Value::Bulk(items) = v {
                    let mut i = 0;
                    while i + 1 < items.len() {
                        let name = value_to_string(&items[i]);
                        let count = value_to_string(&items[i + 1]).parse::<i64>().unwrap_or(0);
                        map.insert(name, count);
                        i += 2;
                    }
                }
            }
            let mut channels = Vec::with_capacity(names.len());
            for n in names {
                let subscribers = map.get(&n).copied().unwrap_or(0);
                channels.push(ChannelInfo { name: n, subscribers });
            }
            Ok(Payload::Channels(channels))
        }

        Request::AclUsers => {
            let users: Vec<String> = redis::cmd("ACL").arg("USERS").query(c).map_err(re)?;
            Ok(Payload::AclUsers(users))
        }
        Request::AclGetUser { name } => {
            let v: redis::Value = redis::cmd("ACL").arg("GETUSER").arg(&name).query(c).map_err(re)?;
            Ok(Payload::AclUser(format_acl_user(&name, &v)))
        }
    }
}

fn re(e: redis::RedisError) -> String {
    format!("redis error: {e}")
}

fn extract_version(info: &str) -> String {
    for line in info.lines() {
        if let Some(v) = line.strip_prefix("redis_version:") {
            return v.trim().to_string();
        }
    }
    "unknown".to_string()
}

fn pairs(flat: Vec<String>) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(flat.len() / 2);
    let mut it = flat.into_iter();
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        out.push((k, v));
    }
    out
}

fn parse_zset(flat: Vec<String>) -> Vec<(String, f64)> {
    let mut out = Vec::with_capacity(flat.len() / 2);
    let mut it = flat.into_iter();
    while let (Some(m), Some(s)) = (it.next(), it.next()) {
        out.push((m, s.parse::<f64>().unwrap_or(0.0)));
    }
    out
}

fn read_stream(c: &mut Connection, key: &str) -> Result<Vec<StreamEntry>, String> {
    let v: redis::Value = redis::cmd("XRANGE")
        .arg(key)
        .arg("-")
        .arg("+")
        .query(c)
        .map_err(re)?;
    let mut out = Vec::new();
    if let redis::Value::Bulk(entries) = v {
        for e in entries {
            if let redis::Value::Bulk(parts) = e {
                if parts.len() >= 2 {
                    let id = value_to_string(&parts[0]);
                    let mut fields = Vec::new();
                    if let redis::Value::Bulk(f) = &parts[1] {
                        let mut i = 0;
                        while i + 1 < f.len() {
                            fields.push((value_to_string(&f[i]), value_to_string(&f[i + 1])));
                            i += 2;
                        }
                    }
                    out.push(StreamEntry { id, fields });
                }
            }
        }
    }
    Ok(out)
}

fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::Nil => "(nil)".to_string(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Data(d) => String::from_utf8_lossy(d).into_owned(),
        redis::Value::Bulk(items) => {
            let inner: Vec<String> = items.iter().map(value_to_string).collect();
            format!("[{}]", inner.join(", "))
        }
        redis::Value::Status(s) => s.clone(),
        redis::Value::Okay => "OK".to_string(),
    }
}

fn format_acl_user(name: &str, v: &redis::Value) -> Vec<String> {
    let mut lines = vec![format!("user: {name}")];
    if let redis::Value::Bulk(items) = v {
        let mut i = 0;
        while i < items.len() {
            let field = value_to_string(&items[i]);
            let value = if i + 1 < items.len() {
                value_to_string(&items[i + 1])
            } else {
                String::new()
            };
            lines.push(format!("{field}: {value}"));
            i += 2;
        }
    }
    lines
}
