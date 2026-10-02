//! Redis access layer.
//!
//! Every function here talks to a real Redis server through the `redis` crate;
//! nothing in this module fabricates data. Values are read as raw bytes and
//! converted lossily so that binary-unsafe payloads never abort a render.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use redis::{Client, Commands, Connection, ConnectionLike, Value};

use crate::util::sanitize;

/// Decode arbitrary Redis bytes into a string that is safe to render: invalid
/// UTF-8 becomes U+FFFD and control bytes become a visible `·`, so a value can
/// never emit an escape sequence or move the cursor.
fn text(bytes: &[u8]) -> String {
    sanitize(&String::from_utf8_lossy(bytes))
}

/// Upper bound on how many keys a single refresh will pull in, so that a huge
/// keyspace cannot freeze the interface.
pub const SCAN_LIMIT: usize = 50_000;
/// Upper bound on collection members rendered for one key.
pub const MEMBER_LIMIT: isize = 5_000;
/// Upper bound on stream entries rendered for one stream.
pub const STREAM_LIMIT: usize = 500;

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum KeyKind {
    String,
    Hash,
    List,
    Set,
    ZSet,
    Stream,
    Other,
}

impl KeyKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "string" => KeyKind::String,
            "hash" => KeyKind::Hash,
            "list" => KeyKind::List,
            "set" => KeyKind::Set,
            "zset" => KeyKind::ZSet,
            "stream" => KeyKind::Stream,
            _ => KeyKind::Other,
        }
    }

    /// The name Redis itself uses (also what `SCAN ... TYPE` expects).
    pub fn redis_name(&self) -> &'static str {
        match self {
            KeyKind::String => "string",
            KeyKind::Hash => "hash",
            KeyKind::List => "list",
            KeyKind::Set => "set",
            KeyKind::ZSet => "zset",
            KeyKind::Stream => "stream",
            KeyKind::Other => "other",
        }
    }

    /// Human label used in the type filter bar.
    pub fn label(&self) -> &'static str {
        match self {
            KeyKind::String => "string",
            KeyKind::Hash => "hash",
            KeyKind::List => "list",
            KeyKind::Set => "set",
            KeyKind::ZSet => "sorted set",
            KeyKind::Stream => "stream",
            KeyKind::Other => "other",
        }
    }

    pub fn short(&self) -> &'static str {
        match self {
            KeyKind::String => "str",
            KeyKind::Hash => "hash",
            KeyKind::List => "list",
            KeyKind::Set => "set",
            KeyKind::ZSet => "zset",
            KeyKind::Stream => "strm",
            KeyKind::Other => "?",
        }
    }

    pub const ALL: [KeyKind; 7] = [
        KeyKind::String,
        KeyKind::Hash,
        KeyKind::List,
        KeyKind::Set,
        KeyKind::ZSet,
        KeyKind::Stream,
        KeyKind::Other,
    ];
}

#[derive(Clone, Debug)]
pub struct KeyEntry {
    pub name: String,
    pub kind: KeyKind,
}

#[derive(Clone, Debug)]
pub struct StreamEntry {
    pub id: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub enum KeyData {
    Str(String),
    Hash(Vec<(String, String)>),
    List(Vec<String>),
    Set(Vec<String>),
    ZSet(Vec<(String, f64)>),
    Stream(Vec<StreamEntry>),
    Missing,
    Other(String),
}

impl KeyData {
    /// Number of addressable items in the detail pane.
    pub fn item_count(&self) -> usize {
        match self {
            KeyData::Str(_) => 1,
            KeyData::Hash(v) => v.len(),
            KeyData::List(v) => v.len(),
            KeyData::Set(v) => v.len(),
            KeyData::ZSet(v) => v.len(),
            KeyData::Stream(v) => v.len(),
            KeyData::Missing | KeyData::Other(_) => 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct KeyDetail {
    pub name: String,
    pub kind: KeyKind,
    pub ttl: i64,
    pub encoding: String,
    pub memory: Option<usize>,
    /// Total element count reported by the server (may exceed what was loaded).
    pub total: usize,
    pub loaded: usize,
    pub data: KeyData,
    /// Extra facts shown in the header, e.g. stream first/last id.
    pub extra: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct StreamSummary {
    pub name: String,
    pub length: usize,
    pub groups: usize,
    pub last_id: String,
    pub first_id: String,
}

#[derive(Clone, Debug)]
pub struct ChannelInfo {
    pub name: String,
    pub subscribers: i64,
    /// True when this process holds a subscription on the channel.
    pub local: bool,
    /// True when the entry is a glob pattern subscription rather than a channel.
    pub pattern: bool,
}

#[derive(Clone, Debug)]
pub struct AclUser {
    pub name: String,
    /// `ACL GETUSER` reply, flattened to ordered display rows.
    pub attrs: Vec<(String, String)>,
    /// `ACL LIST` rule line for the user.
    pub rule: String,
}

pub struct Db {
    conn: Connection,
    pub uri: String,
    pub name: String,
    pub db_index: i64,
    pub server_version: String,
    pub server_mode: String,
}

impl Db {
    pub fn connect(name: &str, uri: &str) -> Result<Db> {
        let client = Client::open(uri).with_context(|| format!("invalid Redis URI `{uri}`"))?;
        let mut conn = client
            .get_connection_with_timeout(Duration::from_secs(5))
            .with_context(|| format!("cannot connect to {uri}"))?;
        conn.set_read_timeout(Some(Duration::from_secs(10))).ok();
        conn.set_write_timeout(Some(Duration::from_secs(10))).ok();
        let db_index = conn.get_db();
        let _: () = redis::cmd("CLIENT")
            .arg("SETNAME")
            .arg("toolj")
            .query(&mut conn)
            .unwrap_or(());
        let mut db = Db {
            conn,
            uri: uri.to_string(),
            name: name.to_string(),
            db_index,
            server_version: String::new(),
            server_mode: String::new(),
        };
        let info = db.info("server").unwrap_or_default();
        db.server_version = info
            .get("redis_version")
            .cloned()
            .unwrap_or_else(|| "?".into());
        db.server_mode = info
            .get("redis_mode")
            .cloned()
            .unwrap_or_else(|| "standalone".into());
        Ok(db)
    }

    pub fn select_db(&mut self, index: i64) -> Result<()> {
        let _: () = redis::cmd("SELECT").arg(index).query(&mut self.conn)?;
        self.db_index = index;
        if let Some((base, _)) = self.uri.rsplit_once('/') {
            if !base.is_empty() && base != "redis:/" {
                self.uri = format!("{base}/{index}");
            }
        }
        Ok(())
    }

    pub fn info(&mut self, section: &str) -> Result<BTreeMap<String, String>> {
        let raw: String = redis::cmd("INFO").arg(section).query(&mut self.conn)?;
        let mut out = BTreeMap::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once(':') {
                out.insert(k.to_string(), v.to_string());
            }
        }
        Ok(out)
    }

    pub fn dbsize(&mut self) -> Result<usize> {
        Ok(redis::cmd("DBSIZE").query(&mut self.conn)?)
    }

    /// Number of keys per logical database, as reported by `INFO keyspace`.
    pub fn keyspace(&mut self) -> Vec<(i64, usize)> {
        let mut out = Vec::new();
        if let Ok(map) = self.info("keyspace") {
            for (k, v) in map {
                if let Some(idx) = k.strip_prefix("db").and_then(|s| s.parse::<i64>().ok()) {
                    let keys = v
                        .split(',')
                        .find_map(|p| p.trim().strip_prefix("keys=")?.parse::<usize>().ok())
                        .unwrap_or(0);
                    out.push((idx, keys));
                }
            }
        }
        out.sort_by_key(|(i, _)| *i);
        out
    }

    /// Full `SCAN` sweep of the current database, resolving every key's type.
    /// Returns the entries plus a flag telling whether [`SCAN_LIMIT`] cut it short.
    pub fn scan_keys(&mut self, pattern: &str, kind: Option<KeyKind>) -> Result<(Vec<KeyEntry>, bool)> {
        let pattern = if pattern.trim().is_empty() {
            "*".to_string()
        } else {
            pattern.to_string()
        };
        let mut cursor: u64 = 0;
        let mut raw: Vec<Vec<u8>> = Vec::new();
        let mut truncated = false;
        loop {
            let mut cmd = redis::cmd("SCAN");
            cmd.arg(cursor).arg("MATCH").arg(&pattern).arg("COUNT").arg(1000);
            if let Some(k) = kind {
                if k != KeyKind::Other {
                    cmd.arg("TYPE").arg(k.redis_name());
                }
            }
            let (next, batch): (u64, Vec<Vec<u8>>) = cmd.query(&mut self.conn)?;
            raw.extend(batch);
            cursor = next;
            if raw.len() >= SCAN_LIMIT {
                raw.truncate(SCAN_LIMIT);
                truncated = true;
                break;
            }
            if cursor == 0 {
                break;
            }
        }

        let mut entries: Vec<KeyEntry> = Vec::with_capacity(raw.len());
        for chunk in raw.chunks(256) {
            let mut pipe = redis::pipe();
            for k in chunk {
                pipe.cmd("TYPE").arg(k);
            }
            let kinds: Vec<String> = pipe.query(&mut self.conn).unwrap_or_default();
            for (i, k) in chunk.iter().enumerate() {
                let kind = kinds
                    .get(i)
                    .map(|s| KeyKind::parse(s))
                    .unwrap_or(KeyKind::Other);
                entries.push(KeyEntry {
                    name: text(k),
                    kind,
                });
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((entries, truncated))
    }

    pub fn type_of(&mut self, key: &str) -> Result<KeyKind> {
        let t: String = redis::cmd("TYPE").arg(key).query(&mut self.conn)?;
        Ok(KeyKind::parse(&t))
    }

    /// Load everything needed to render one key on a single screen.
    pub fn key_detail(&mut self, key: &str) -> Result<KeyDetail> {
        let kind = self.type_of(key)?;
        if kind == KeyKind::Other {
            let exists: bool = self.conn.exists(key)?;
            if !exists {
                return Ok(KeyDetail {
                    name: key.to_string(),
                    kind,
                    ttl: -2,
                    encoding: "-".into(),
                    memory: None,
                    total: 0,
                    loaded: 0,
                    data: KeyData::Missing,
                    extra: Vec::new(),
                });
            }
        }
        let ttl: i64 = redis::cmd("TTL").arg(key).query(&mut self.conn).unwrap_or(-1);
        let encoding: String = redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(key)
            .query(&mut self.conn)
            .unwrap_or_else(|_| "-".into());
        let memory: Option<usize> = redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(key)
            .query(&mut self.conn)
            .ok();

        let mut extra: Vec<(String, String)> = Vec::new();
        let (total, data) = match kind {
            KeyKind::String => {
                let bytes: Vec<u8> = redis::cmd("GET").arg(key).query(&mut self.conn)?;
                let len = bytes.len();
                extra.push(("length".into(), format!("{len} bytes")));
                (1, KeyData::Str(text(&bytes)))
            }
            KeyKind::Hash => {
                let total: usize = redis::cmd("HLEN").arg(key).query(&mut self.conn)?;
                let flat: Vec<Vec<u8>> = redis::cmd("HGETALL").arg(key).query(&mut self.conn)?;
                let mut pairs: Vec<(String, String)> = flat
                    .chunks(2)
                    .filter(|c| c.len() == 2)
                    .map(|c| {
                        (
                            text(&c[0]),
                            text(&c[1]),
                        )
                    })
                    .collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                (total, KeyData::Hash(pairs))
            }
            KeyKind::List => {
                let total: usize = redis::cmd("LLEN").arg(key).query(&mut self.conn)?;
                let items: Vec<Vec<u8>> = redis::cmd("LRANGE")
                    .arg(key)
                    .arg(0)
                    .arg(MEMBER_LIMIT - 1)
                    .query(&mut self.conn)?;
                (
                    total,
                    KeyData::List(
                        items
                            .iter()
                            .map(|b| text(b))
                            .collect(),
                    ),
                )
            }
            KeyKind::Set => {
                let total: usize = redis::cmd("SCARD").arg(key).query(&mut self.conn)?;
                let mut cursor = 0u64;
                let mut items: Vec<String> = Vec::new();
                loop {
                    let (next, batch): (u64, Vec<Vec<u8>>) = redis::cmd("SSCAN")
                        .arg(key)
                        .arg(cursor)
                        .arg("COUNT")
                        .arg(500)
                        .query(&mut self.conn)?;
                    items.extend(batch.iter().map(|b| text(b)));
                    cursor = next;
                    if cursor == 0 || items.len() as isize >= MEMBER_LIMIT {
                        break;
                    }
                }
                items.sort();
                (total, KeyData::Set(items))
            }
            KeyKind::ZSet => {
                let total: usize = redis::cmd("ZCARD").arg(key).query(&mut self.conn)?;
                let flat: Vec<Vec<u8>> = redis::cmd("ZRANGE")
                    .arg(key)
                    .arg(0)
                    .arg(MEMBER_LIMIT - 1)
                    .arg("WITHSCORES")
                    .query(&mut self.conn)?;
                let members: Vec<(String, f64)> = flat
                    .chunks(2)
                    .filter(|c| c.len() == 2)
                    .map(|c| {
                        (
                            text(&c[0]),
                            String::from_utf8_lossy(&c[1]).parse::<f64>().unwrap_or(0.0),
                        )
                    })
                    .collect();
                (total, KeyData::ZSet(members))
            }
            KeyKind::Stream => {
                let summary = self.stream_summary(key)?;
                extra.push(("groups".into(), summary.groups.to_string()));
                if !summary.first_id.is_empty() {
                    extra.push(("first-id".into(), summary.first_id.clone()));
                }
                if !summary.last_id.is_empty() {
                    extra.push(("last-id".into(), summary.last_id.clone()));
                }
                let entries = self.stream_entries(key, STREAM_LIMIT)?;
                (summary.length, KeyData::Stream(entries))
            }
            KeyKind::Other => (0, KeyData::Other("unsupported type".into())),
        };
        let loaded = data.item_count();
        Ok(KeyDetail {
            name: key.to_string(),
            kind,
            ttl,
            encoding,
            memory,
            total,
            loaded,
            data,
            extra,
        })
    }

    // ---------------------------------------------------------------- streams

    pub fn stream_summary(&mut self, key: &str) -> Result<StreamSummary> {
        let mut out = StreamSummary {
            name: key.to_string(),
            ..Default::default()
        };
        let v: Value = redis::cmd("XINFO").arg("STREAM").arg(key).query(&mut self.conn)?;
        for (k, val) in as_pairs(&v) {
            match k.as_str() {
                "length" => out.length = render(&val).parse().unwrap_or(0),
                "groups" => out.groups = render(&val).parse().unwrap_or(0),
                "last-generated-id" => out.last_id = render(&val),
                _ => {}
            }
        }
        if let Ok(first) = self.stream_entries_range(key, "-", "+", 1) {
            if let Some(e) = first.first() {
                out.first_id = e.id.clone();
            }
        }
        Ok(out)
    }

    /// Newest-first stream entries, capped at `count`.
    pub fn stream_entries(&mut self, key: &str, count: usize) -> Result<Vec<StreamEntry>> {
        let v: Value = redis::cmd("XREVRANGE")
            .arg(key)
            .arg("+")
            .arg("-")
            .arg("COUNT")
            .arg(count)
            .query(&mut self.conn)?;
        Ok(parse_stream_entries(&v))
    }

    fn stream_entries_range(
        &mut self,
        key: &str,
        start: &str,
        end: &str,
        count: usize,
    ) -> Result<Vec<StreamEntry>> {
        let v: Value = redis::cmd("XRANGE")
            .arg(key)
            .arg(start)
            .arg(end)
            .arg("COUNT")
            .arg(count)
            .query(&mut self.conn)?;
        Ok(parse_stream_entries(&v))
    }

    /// Every stream key in the current database, with summary counters.
    pub fn streams(&mut self, pattern: &str) -> Result<Vec<StreamSummary>> {
        let (entries, _) = self.scan_keys(pattern, Some(KeyKind::Stream))?;
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            if e.kind != KeyKind::Stream {
                continue;
            }
            match self.stream_summary(&e.name) {
                Ok(s) => out.push(s),
                Err(_) => out.push(StreamSummary {
                    name: e.name,
                    ..Default::default()
                }),
            }
        }
        Ok(out)
    }

    pub fn stream_groups(&mut self, key: &str) -> Vec<Vec<(String, String)>> {
        let v: Value = match redis::cmd("XINFO")
            .arg("GROUPS")
            .arg(key)
            .query(&mut self.conn)
        {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        as_array(&v).iter().map(as_pairs_str).collect()
    }

    pub fn xadd(&mut self, key: &str, id: &str, fields: &[(String, String)]) -> Result<String> {
        if fields.is_empty() {
            return Err(anyhow!("XADD needs at least one field=value pair"));
        }
        let mut cmd = redis::cmd("XADD");
        cmd.arg(key).arg(id);
        for (f, v) in fields {
            cmd.arg(f).arg(v);
        }
        let id: String = cmd.query(&mut self.conn)?;
        Ok(id)
    }

    pub fn xdel(&mut self, key: &str, id: &str) -> Result<i64> {
        Ok(redis::cmd("XDEL").arg(key).arg(id).query(&mut self.conn)?)
    }

    // ----------------------------------------------------------------- pubsub

    /// Channels known to the server (`PUBSUB CHANNELS`) merged with the
    /// subscriptions this process currently holds.
    pub fn pubsub_channels(
        &mut self,
        pattern: &str,
        local: &[(String, bool)],
    ) -> Result<Vec<ChannelInfo>> {
        let pattern = if pattern.trim().is_empty() { "*" } else { pattern };
        let names: Vec<String> = redis::cmd("PUBSUB")
            .arg("CHANNELS")
            .arg(pattern)
            .query(&mut self.conn)?;
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for chunk in names.chunks(64) {
            let mut cmd = redis::cmd("PUBSUB");
            cmd.arg("NUMSUB");
            for n in chunk {
                cmd.arg(n);
            }
            if let Ok(v) = cmd.query::<Vec<Value>>(&mut self.conn) {
                for pair in v.chunks(2) {
                    if pair.len() == 2 {
                        counts.insert(render(&pair[0]), render(&pair[1]).parse().unwrap_or(0));
                    }
                }
            }
        }
        let mut out: Vec<ChannelInfo> = names
            .into_iter()
            .map(|name| ChannelInfo {
                subscribers: counts.get(&name).copied().unwrap_or(0),
                local: local.iter().any(|(l, pat)| !*pat && *l == name),
                pattern: false,
                name,
            })
            .collect();
        // Local pattern subscriptions have no entry in PUBSUB CHANNELS; show
        // them so the user can see what this session listens to.
        for (name, is_pattern) in local {
            if *is_pattern {
                let subscribers: i64 = redis::cmd("PUBSUB")
                    .arg("NUMPAT")
                    .query(&mut self.conn)
                    .unwrap_or(0);
                out.push(ChannelInfo {
                    name: name.clone(),
                    subscribers,
                    local: true,
                    pattern: true,
                });
            } else if !out.iter().any(|c| c.name == *name) {
                out.push(ChannelInfo {
                    name: name.clone(),
                    subscribers: 1,
                    local: true,
                    pattern: false,
                });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out.dedup_by(|a, b| a.name == b.name && a.pattern == b.pattern);
        Ok(out)
    }

    pub fn pubsub_numpat(&mut self) -> i64 {
        redis::cmd("PUBSUB")
            .arg("NUMPAT")
            .query(&mut self.conn)
            .unwrap_or(0)
    }

    pub fn publish(&mut self, channel: &str, payload: &str) -> Result<i64> {
        Ok(redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .query(&mut self.conn)?)
    }

    // -------------------------------------------------------------------- acl

    pub fn acl_users(&mut self) -> Result<Vec<AclUser>> {
        let names: Vec<String> = redis::cmd("ACL").arg("USERS").query(&mut self.conn)?;
        let rules: Vec<String> = redis::cmd("ACL")
            .arg("LIST")
            .query(&mut self.conn)
            .unwrap_or_default();
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let rule = rules
                .iter()
                .find(|r| {
                    r.split_whitespace().nth(1).map(|n| n == name).unwrap_or(false)
                })
                .cloned()
                .unwrap_or_default();
            let attrs = self.acl_getuser(&name).unwrap_or_default();
            out.push(AclUser { name, attrs, rule });
        }
        Ok(out)
    }

    pub fn acl_getuser(&mut self, name: &str) -> Result<Vec<(String, String)>> {
        let v: Value = redis::cmd("ACL")
            .arg("GETUSER")
            .arg(name)
            .query(&mut self.conn)?;
        if matches!(v, Value::Nil) {
            return Err(anyhow!("no such ACL user `{name}`"));
        }
        Ok(as_pairs_str(&v))
    }

    pub fn acl_whoami(&mut self) -> String {
        redis::cmd("ACL")
            .arg("WHOAMI")
            .query(&mut self.conn)
            .unwrap_or_else(|_| "?".to_string())
    }

    pub fn acl_setuser(&mut self, name: &str, rules: &[String]) -> Result<()> {
        let mut cmd = redis::cmd("ACL");
        cmd.arg("SETUSER").arg(name);
        for r in rules {
            cmd.arg(r);
        }
        let _: Value = cmd.query(&mut self.conn)?;
        Ok(())
    }

    pub fn acl_deluser(&mut self, name: &str) -> Result<i64> {
        Ok(redis::cmd("ACL")
            .arg("DELUSER")
            .arg(name)
            .query(&mut self.conn)?)
    }

    // -------------------------------------------------------------- mutations

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let _: () = redis::cmd("SET").arg(key).arg(value).query(&mut self.conn)?;
        Ok(())
    }

    pub fn del(&mut self, key: &str) -> Result<i64> {
        Ok(redis::cmd("DEL").arg(key).query(&mut self.conn)?)
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let _: () = redis::cmd("RENAME").arg(from).arg(to).query(&mut self.conn)?;
        Ok(())
    }

    pub fn expire(&mut self, key: &str, secs: i64) -> Result<i64> {
        if secs < 0 {
            return Ok(redis::cmd("PERSIST").arg(key).query(&mut self.conn)?);
        }
        Ok(redis::cmd("EXPIRE").arg(key).arg(secs).query(&mut self.conn)?)
    }

    pub fn hset(&mut self, key: &str, pairs: &[(String, String)]) -> Result<i64> {
        let mut cmd = redis::cmd("HSET");
        cmd.arg(key);
        for (f, v) in pairs {
            cmd.arg(f).arg(v);
        }
        Ok(cmd.query(&mut self.conn)?)
    }

    pub fn hdel(&mut self, key: &str, field: &str) -> Result<i64> {
        Ok(redis::cmd("HDEL").arg(key).arg(field).query(&mut self.conn)?)
    }

    pub fn rpush(&mut self, key: &str, items: &[String]) -> Result<i64> {
        let mut cmd = redis::cmd("RPUSH");
        cmd.arg(key);
        for i in items {
            cmd.arg(i);
        }
        Ok(cmd.query(&mut self.conn)?)
    }

    pub fn lpush(&mut self, key: &str, items: &[String]) -> Result<i64> {
        let mut cmd = redis::cmd("LPUSH");
        cmd.arg(key);
        for i in items {
            cmd.arg(i);
        }
        Ok(cmd.query(&mut self.conn)?)
    }

    pub fn lset(&mut self, key: &str, index: isize, value: &str) -> Result<()> {
        let _: () = redis::cmd("LSET")
            .arg(key)
            .arg(index)
            .arg(value)
            .query(&mut self.conn)?;
        Ok(())
    }

    /// Remove the element at `index` while keeping the rest of the list order.
    pub fn lremove_at(&mut self, key: &str, index: isize) -> Result<()> {
        let sentinel = "__toolj_tombstone__";
        self.lset(key, index, sentinel)?;
        let _: i64 = redis::cmd("LREM")
            .arg(key)
            .arg(1)
            .arg(sentinel)
            .query(&mut self.conn)?;
        Ok(())
    }

    pub fn sadd(&mut self, key: &str, members: &[String]) -> Result<i64> {
        let mut cmd = redis::cmd("SADD");
        cmd.arg(key);
        for m in members {
            cmd.arg(m);
        }
        Ok(cmd.query(&mut self.conn)?)
    }

    pub fn srem(&mut self, key: &str, member: &str) -> Result<i64> {
        Ok(redis::cmd("SREM").arg(key).arg(member).query(&mut self.conn)?)
    }

    pub fn zadd(&mut self, key: &str, members: &[(String, f64)]) -> Result<i64> {
        let mut cmd = redis::cmd("ZADD");
        cmd.arg(key);
        for (m, s) in members {
            cmd.arg(*s).arg(m);
        }
        Ok(cmd.query(&mut self.conn)?)
    }

    pub fn zrem(&mut self, key: &str, member: &str) -> Result<i64> {
        Ok(redis::cmd("ZREM").arg(key).arg(member).query(&mut self.conn)?)
    }

    /// Run an arbitrary command and return its reply rendered for display.
    pub fn raw(&mut self, argv: &[String]) -> Result<String> {
        let (name, args) = argv
            .split_first()
            .ok_or_else(|| anyhow!("empty command"))?;
        let mut cmd = redis::cmd(&name.to_uppercase());
        for a in args {
            cmd.arg(a);
        }
        let v: Value = cmd.query(&mut self.conn)?;
        Ok(render_multiline(&v))
    }
}

// -------------------------------------------------------------- Value helpers

/// Render a Redis value as one display line.
pub fn render(v: &Value) -> String {
    match v {
        Value::Nil => "(nil)".into(),
        Value::Int(i) => i.to_string(),
        Value::BulkString(b) => text(b),
        Value::SimpleString(s) => sanitize(s),
        Value::Okay => "OK".into(),
        Value::Double(d) => crate::util::fmt_score(*d),
        Value::Boolean(b) => b.to_string(),
        Value::BigNumber(n) => n.to_string(),
        Value::VerbatimString { text: t, .. } => sanitize(t),
        Value::Array(items) | Value::Set(items) | Value::Push { data: items, .. } => items
            .iter()
            .map(render)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Map(pairs) => pairs
            .iter()
            .map(|(k, v)| format!("{}={}", render(k), render(v)))
            .collect::<Vec<_>>()
            .join(", "),
        Value::Attribute { data, .. } => render(data),
        Value::ServerError(e) => format!(
            "ERR {}",
            e.details().unwrap_or("server error")
        ),
    }
}

/// Render a Redis value across multiple lines (used for raw command replies).
pub fn render_multiline(v: &Value) -> String {
    fn go(v: &Value, indent: usize, out: &mut Vec<String>) {
        let pad = " ".repeat(indent);
        match v {
            Value::Array(items) | Value::Set(items) | Value::Push { data: items, .. } => {
                if items.is_empty() {
                    out.push(format!("{pad}(empty array)"));
                }
                for (i, it) in items.iter().enumerate() {
                    match it {
                        Value::Array(_) | Value::Set(_) | Value::Map(_) => {
                            out.push(format!("{pad}{}) ", i + 1));
                            go(it, indent + 2, out);
                        }
                        _ => out.push(format!("{pad}{}) {}", i + 1, render(it))),
                    }
                }
            }
            Value::Map(pairs) => {
                for (k, val) in pairs {
                    match val {
                        Value::Array(_) | Value::Set(_) | Value::Map(_) => {
                            out.push(format!("{pad}{}:", render(k)));
                            go(val, indent + 2, out);
                        }
                        _ => out.push(format!("{pad}{}: {}", render(k), render(val))),
                    }
                }
            }
            other => out.push(format!("{pad}{}", render(other))),
        }
    }
    let mut out = Vec::new();
    go(v, 0, &mut out);
    out.join("\n")
}

fn as_array(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(items) | Value::Set(items) => items.clone(),
        Value::Nil => Vec::new(),
        other => vec![other.clone()],
    }
}

/// Interpret a value as key/value pairs, accepting both RESP2 flat arrays and
/// RESP3 maps.
fn as_pairs(v: &Value) -> Vec<(String, Value)> {
    match v {
        Value::Map(pairs) => pairs.iter().map(|(k, v)| (render(k), v.clone())).collect(),
        Value::Array(items) | Value::Set(items) => items
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| (render(&c[0]), c[1].clone()))
            .collect(),
        _ => Vec::new(),
    }
}

fn as_pairs_str(v: &Value) -> Vec<(String, String)> {
    as_pairs(v)
        .into_iter()
        .map(|(k, val)| {
            let rendered = match &val {
                Value::Array(items) | Value::Set(items) if items.is_empty() => "(none)".to_string(),
                other => render(other),
            };
            // An empty reply reads as a rendering bug otherwise.
            let rendered = if rendered.is_empty() {
                "(none)".to_string()
            } else {
                rendered
            };
            (k, rendered)
        })
        .collect()
}

fn parse_stream_entries(v: &Value) -> Vec<StreamEntry> {
    as_array(v)
        .iter()
        .filter_map(|e| {
            let parts = as_array(e);
            if parts.len() < 2 {
                return None;
            }
            let id = render(&parts[0]);
            let flat = as_array(&parts[1]);
            let fields = flat
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| (render(&c[0]), render(&c[1])))
                .collect();
            Some(StreamEntry { id, fields })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip() {
        for k in KeyKind::ALL {
            if k != KeyKind::Other {
                assert_eq!(KeyKind::parse(k.redis_name()), k);
            }
        }
    }

    #[test]
    fn pairs_from_resp2_and_resp3() {
        let flat = Value::Array(vec![
            Value::BulkString(b"flags".to_vec()),
            Value::Array(vec![Value::BulkString(b"on".to_vec())]),
        ]);
        assert_eq!(as_pairs_str(&flat), vec![("flags".into(), "on".into())]);
        let map = Value::Map(vec![(
            Value::BulkString(b"flags".to_vec()),
            Value::Array(vec![]),
        )]);
        assert_eq!(as_pairs_str(&map), vec![("flags".into(), "(none)".into())]);
    }

    #[test]
    fn stream_entries_parse() {
        let v = Value::Array(vec![Value::Array(vec![
            Value::BulkString(b"1-1".to_vec()),
            Value::Array(vec![
                Value::BulkString(b"f".to_vec()),
                Value::BulkString(b"v".to_vec()),
            ]),
        ])]);
        let e = parse_stream_entries(&v);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].id, "1-1");
        assert_eq!(e[0].fields, vec![("f".to_string(), "v".to_string())]);
    }
}
