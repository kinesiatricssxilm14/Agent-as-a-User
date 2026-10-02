//! Everything that mutates Redis or the server list, plus the `:` command line.

use anyhow::{anyhow, Result};

use crate::app::{
    format_err, App, Confirm, ConfirmAction, Focus, Level, Prompt, PromptKind, View,
};
use crate::config::normalize_uri;
use crate::db::{KeyData, KeyKind};
use crate::util::{parse_pairs, parse_scored, tokenize};

/// Open a prompt, refusing when there is nothing to act on.
pub fn open_prompt(app: &mut App, kind: PromptKind, label: &str, hint: &str, initial: &str) {
    app.prompt = Some(Prompt::new(kind, label, hint, initial));
    app.history_pos = None;
}

pub fn ask(app: &mut App, question: impl Into<String>, action: ConfirmAction) {
    app.confirm = Some(Confirm {
        question: question.into(),
        action,
    });
}

/// Apply a confirmed destructive action.
pub fn run_confirm(app: &mut App, action: ConfirmAction) {
    match action {
        ConfirmAction::DeleteKey(key) => {
            let r = (|| -> Result<String> {
                let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
                let n = db.del(&key)?;
                if n == 0 {
                    Err(anyhow!("`{key}` did not exist"))
                } else {
                    Ok(format!("Deleted key `{key}`"))
                }
            })();
            app.report(r);
            match app.view {
                View::Streams => app.refresh_streams(),
                _ => app.refresh_keys(),
            }
            app.focus = Focus::List;
        }
        ConfirmAction::DeleteItem { key, label } => {
            let r = delete_item(app, &key, &label);
            app.report(r);
            app.reload_detail();
            let len = app.detail_len();
            app.detail_cur.clamp(len);
            app.stream_detail_cur.clamp(len);
        }
        ConfirmAction::DeleteAclUser(name) => {
            let r = (|| -> Result<String> {
                let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
                let n = db.acl_deluser(&name)?;
                if n == 0 {
                    Err(anyhow!("no ACL user `{name}`"))
                } else {
                    Ok(format!("Deleted ACL user `{name}`"))
                }
            })();
            app.report(r);
            app.refresh_acl();
        }
        ConfirmAction::RemoveServer(name) => {
            if app.cfg.remove(&name) {
                match app.cfg.save() {
                    Ok(()) => app.ok(format!("Removed server `{name}`")),
                    Err(e) => app.error(format!("Removed `{name}` but could not save: {e}")),
                }
            } else {
                app.warn(format!("No server named `{name}`"));
            }
            let len = app.cfg.servers.len();
            app.server_cur.clamp(len);
        }
    }
}

/// Remove one element/field/member from the currently loaded key.
fn delete_item(app: &mut App, key: &str, label: &str) -> Result<String> {
    let idx = match app.view {
        View::Streams => app.stream_detail_cur.sel,
        _ => app.detail_cur.sel,
    };
    let data = app
        .detail
        .as_ref()
        .map(|d| d.data.clone())
        .ok_or_else(|| anyhow!("nothing loaded"))?;
    let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
    match data {
        KeyData::Hash(v) => {
            let (field, _) = v.get(idx).ok_or_else(|| anyhow!("no such field"))?;
            db.hdel(key, field)?;
            Ok(format!("HDEL {key} {field}"))
        }
        KeyData::List(_) => {
            db.lremove_at(key, idx as isize)?;
            Ok(format!("Removed {key}[{idx}]"))
        }
        KeyData::Set(v) => {
            let m = v.get(idx).ok_or_else(|| anyhow!("no such member"))?;
            db.srem(key, m)?;
            Ok(format!("SREM {key} {m}"))
        }
        KeyData::ZSet(v) => {
            let (m, _) = v.get(idx).ok_or_else(|| anyhow!("no such member"))?;
            db.zrem(key, m)?;
            Ok(format!("ZREM {key} {m}"))
        }
        KeyData::Stream(v) => {
            let e = v.get(idx).ok_or_else(|| anyhow!("no such entry"))?;
            db.xdel(key, &e.id)?;
            Ok(format!("XDEL {key} {}", e.id))
        }
        KeyData::Str(_) => {
            db.del(key)?;
            Ok(format!("Deleted string key `{key}`"))
        }
        _ => Err(anyhow!("cannot delete `{label}` from this key type")),
    }
}

/// Handle Enter in a prompt. Returns after refreshing whatever changed.
pub fn submit_prompt(app: &mut App, prompt: Prompt) {
    let input = prompt.input.trim().to_string();
    match prompt.kind {
        PromptKind::Command => {
            if !input.is_empty() {
                app.history.retain(|h| *h != input);
                app.history.push(input.clone());
            }
            run_command(app, &input);
        }
        PromptKind::Search => {
            app.filter = prompt.input.clone();
            app.on_filter_changed();
        }
        PromptKind::ScanPattern => {
            app.scan_pattern = if input.is_empty() { "*".into() } else { input };
            app.info_msg(format!("SCAN MATCH {}", app.scan_pattern));
            app.refresh_current();
        }
        PromptKind::NewKey => {
            let r = create_key(app, &prompt.input);
            app.report(r);
            app.refresh_keys();
        }
        PromptKind::EditString(key) => {
            let value = prompt.input.clone();
            let r = with_db(app, |db| {
                db.set(&key, &value)?;
                Ok(format!("SET {key} ({} bytes)", value.len()))
            });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::HashSet(key) => {
            let r = parse_pairs(&prompt.input)
                .map_err(|e| anyhow!(e))
                .and_then(|pairs| {
                    with_db(app, |db| {
                        let added = db.hset(&key, &pairs)?;
                        Ok(format!("HSET {key}: {} new, {} updated", added, pairs.len() as i64 - added))
                    })
                });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::HashEdit { key, field } => {
            let value = prompt.input.clone();
            let r = with_db(app, |db| {
                db.hset(&key, &[(field.clone(), value.clone())])?;
                Ok(format!("HSET {key} {field}"))
            });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::ListPush { key, front } => {
            let r = tokenize(&prompt.input)
                .map_err(|e| anyhow!(e))
                .and_then(|items| {
                    if items.is_empty() {
                        return Err(anyhow!("nothing to push"));
                    }
                    with_db(app, |db| {
                        let len = if front {
                            db.lpush(&key, &items)?
                        } else {
                            db.rpush(&key, &items)?
                        };
                        Ok(format!(
                            "{} {key}: {} item(s), length {len}",
                            if front { "LPUSH" } else { "RPUSH" },
                            items.len()
                        ))
                    })
                });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::ListEdit { key, index } => {
            let value = prompt.input.clone();
            let r = with_db(app, |db| {
                db.lset(&key, index, &value)?;
                Ok(format!("LSET {key} {index}"))
            });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::SetAdd(key) => {
            let r = tokenize(&prompt.input)
                .map_err(|e| anyhow!(e))
                .and_then(|members| {
                    if members.is_empty() {
                        return Err(anyhow!("nothing to add"));
                    }
                    with_db(app, |db| {
                        let n = db.sadd(&key, &members)?;
                        Ok(format!("SADD {key}: {n} new member(s)"))
                    })
                });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::SetEdit { key, member } => {
            let value = prompt.input.clone();
            let r = with_db(app, |db| {
                if value.is_empty() {
                    return Err(anyhow!("empty member"));
                }
                if value != member {
                    db.srem(&key, &member)?;
                    db.sadd(&key, std::slice::from_ref(&value))?;
                }
                Ok(format!("Replaced member in {key}"))
            });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::ZAdd(key) => {
            let r = parse_scored(&prompt.input)
                .map_err(|e| anyhow!(e))
                .and_then(|members| {
                    with_db(app, |db| {
                        let n = db.zadd(&key, &members)?;
                        Ok(format!("ZADD {key}: {n} new member(s)"))
                    })
                });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::ZEdit { key, member } => {
            let raw = prompt.input.clone();
            let r = raw
                .trim()
                .parse::<f64>()
                .map_err(|_| anyhow!("`{}` is not a number", raw.trim()))
                .and_then(|score| {
                    with_db(app, |db| {
                        db.zadd(&key, &[(member.clone(), score)])?;
                        Ok(format!("ZADD {key} {score} {member}"))
                    })
                });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::XAdd(key) => {
            let r = xadd(app, &key, &prompt.input);
            app.report(r);
            if app.view == View::Streams {
                app.refresh_streams();
            } else {
                app.reload_detail();
            }
        }
        PromptKind::Rename(key) => {
            let to = input.clone();
            let r = with_db(app, |db| {
                if to.is_empty() {
                    return Err(anyhow!("new name is empty"));
                }
                db.rename(&key, &to)?;
                Ok(format!("Renamed `{key}` to `{to}`"))
            });
            app.report(r);
            app.refresh_current();
        }
        PromptKind::Expire(key) => {
            let r = parse_ttl(&input).and_then(|secs| {
                with_db(app, |db| {
                    let n = db.expire(&key, secs)?;
                    if n == 0 {
                        Err(anyhow!("`{key}` has no TTL to change"))
                    } else if secs < 0 {
                        Ok(format!("PERSIST {key}"))
                    } else {
                        Ok(format!("EXPIRE {key} {secs}"))
                    }
                })
            });
            app.report(r);
            app.reload_detail();
        }
        PromptKind::Publish(channel) => {
            let r = publish(app, channel.as_deref(), &prompt.input);
            app.report(r);
            app.refresh_channels();
        }
        PromptKind::Subscribe { pattern } => {
            if input.is_empty() {
                app.warn("No channel given");
            } else {
                match app.sub.toggle(&input, pattern) {
                    Ok(true) => app.ok(format!(
                        "{} {input}",
                        if pattern { "PSUBSCRIBE" } else { "SUBSCRIBE" }
                    )),
                    Ok(false) => app.ok(format!("Unsubscribed from {input}")),
                    Err(e) => app.error(format_err(&e)),
                }
                app.refresh_channels();
            }
        }
        PromptKind::AddServer => {
            let r = add_server(app, &prompt.input);
            match r {
                Ok((name, uri)) => {
                    app.ok(format!("Saved server `{name}`"));
                    app.connect(&name, &uri);
                    if let Some(pos) = app.cfg.servers.iter().position(|s| s.name == name) {
                        app.server_cur.sel = pos;
                    }
                }
                Err(e) => app.error(format_err(&e)),
            }
        }
        PromptKind::AclSetUser(_) => {
            let r = acl_setuser(app, &prompt.input);
            app.report(r);
            app.refresh_acl();
        }
        PromptKind::SelectDb => {
            let r = input
                .parse::<i64>()
                .map_err(|_| anyhow!("`{input}` is not a database index"))
                .and_then(|idx| {
                    with_db(app, |db| {
                        db.select_db(idx)?;
                        Ok(format!("SELECT {idx}"))
                    })
                });
            let ok = r.is_ok();
            app.report(r);
            if ok {
                app.refresh_current();
            }
        }
    }
}

fn with_db<F>(app: &mut App, f: F) -> Result<String>
where
    F: FnOnce(&mut crate::db::Db) -> Result<String>,
{
    let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
    f(db)
}

fn parse_ttl(input: &str) -> Result<i64> {
    let s = input.trim();
    if s.is_empty() {
        return Err(anyhow!("no TTL given (use -1 to persist)"));
    }
    if s.eq_ignore_ascii_case("persist") || s.eq_ignore_ascii_case("none") {
        return Ok(-1);
    }
    // Accept plain seconds or a `90s` / `5m` / `2h` / `1d` suffix.
    let (num, mult) = match s.chars().last().unwrap() {
        's' | 'S' => (&s[..s.len() - 1], 1),
        'm' | 'M' => (&s[..s.len() - 1], 60),
        'h' | 'H' => (&s[..s.len() - 1], 3600),
        'd' | 'D' => (&s[..s.len() - 1], 86400),
        _ => (s, 1),
    };
    let n: i64 = num
        .trim()
        .parse()
        .map_err(|_| anyhow!("`{s}` is not a duration (try 60, 5m, 2h, or -1)"))?;
    Ok(if n < 0 { -1 } else { n * mult })
}

/// `<type> <key> <value...>` — create any of the supported types.
fn create_key(app: &mut App, input: &str) -> Result<String> {
    let toks = tokenize(input).map_err(|e| anyhow!(e))?;
    if toks.len() < 2 {
        return Err(anyhow!(
            "usage: <type> <key> <value...>  (types: string hash list set zset stream)"
        ));
    }
    let kind_word = toks[0].to_lowercase();
    let kind = match kind_word.as_str() {
        "string" | "str" | "s" => KeyKind::String,
        "hash" | "h" => KeyKind::Hash,
        "list" | "l" => KeyKind::List,
        "set" => KeyKind::Set,
        "zset" | "sortedset" | "sorted" | "z" => KeyKind::ZSet,
        "stream" | "x" => KeyKind::Stream,
        other => return Err(anyhow!("unknown type `{other}`")),
    };
    let key = toks[1].clone();
    let rest_words = &toks[2..];
    // Re-derive the raw remainder so quoted values keep their spacing.
    let rest_raw = remainder(input, 2).unwrap_or_default();

    let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
    match kind {
        KeyKind::String => {
            db.set(&key, &rest_raw)?;
            Ok(format!("SET {key} ({} bytes)", rest_raw.len()))
        }
        KeyKind::Hash => {
            let pairs = parse_pairs(&rest_raw).map_err(|e| anyhow!(e))?;
            db.hset(&key, &pairs)?;
            Ok(format!("HSET {key}: {} field(s)", pairs.len()))
        }
        KeyKind::List => {
            if rest_words.is_empty() {
                return Err(anyhow!("a list needs at least one element"));
            }
            let len = db.rpush(&key, rest_words)?;
            Ok(format!("RPUSH {key}: length {len}"))
        }
        KeyKind::Set => {
            if rest_words.is_empty() {
                return Err(anyhow!("a set needs at least one member"));
            }
            let n = db.sadd(&key, rest_words)?;
            Ok(format!("SADD {key}: {n} member(s)"))
        }
        KeyKind::ZSet => {
            let members = parse_scored(&rest_raw).map_err(|e| anyhow!(e))?;
            let n = db.zadd(&key, &members)?;
            Ok(format!("ZADD {key}: {n} member(s)"))
        }
        KeyKind::Stream => {
            let (id, fields) = split_stream_args(&rest_raw)?;
            let id = db.xadd(&key, &id, &fields)?;
            Ok(format!("XADD {key} {id}"))
        }
        KeyKind::Other => Err(anyhow!("unsupported type")),
    }
}

/// Split `[id] field=value ...` for XADD, defaulting the id to `*`.
fn split_stream_args(input: &str) -> Result<(String, Vec<(String, String)>)> {
    let toks = tokenize(input).map_err(|e| anyhow!(e))?;
    if toks.is_empty() {
        return Err(anyhow!("usage: [id] field=value ...  (id `*` auto-generates)"));
    }
    let first = &toks[0];
    let looks_like_id = first == "*"
        || first
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '*');
    let (id, rest_index) = if looks_like_id && !first.contains('=') {
        (first.clone(), 1)
    } else {
        ("*".to_string(), 0)
    };
    let rest = remainder(input, rest_index).unwrap_or_default();
    let fields = parse_pairs(&rest).map_err(|e| anyhow!(e))?;
    Ok((id, fields))
}

fn xadd(app: &mut App, key: &str, input: &str) -> Result<String> {
    let (id, fields) = split_stream_args(input)?;
    let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
    let id = db.xadd(key, &id, &fields)?;
    Ok(format!("XADD {key} {id}"))
}

fn publish(app: &mut App, channel: Option<&str>, input: &str) -> Result<String> {
    let (channel, payload) = match channel {
        Some(c) => (c.to_string(), input.to_string()),
        None => {
            let toks = tokenize(input).map_err(|e| anyhow!(e))?;
            if toks.is_empty() {
                return Err(anyhow!("usage: <channel> <message>"));
            }
            (toks[0].clone(), remainder(input, 1).unwrap_or_default())
        }
    };
    if payload.is_empty() {
        return Err(anyhow!("message is empty"));
    }
    let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
    let n = db.publish(&channel, &payload)?;
    Ok(format!("PUBLISH {channel}: delivered to {n} subscriber(s)"))
}

fn add_server(app: &mut App, input: &str) -> Result<(String, String)> {
    let toks = tokenize(input).map_err(|e| anyhow!(e))?;
    if toks.is_empty() {
        return Err(anyhow!("usage: <name> <redis-uri>"));
    }
    let (name, uri) = if toks.len() == 1 {
        // A bare URI gets a name derived from its host:port.
        let uri = normalize_uri(&toks[0]);
        let name = uri
            .trim_start_matches("redis://")
            .trim_start_matches("rediss://")
            .replace('/', "-");
        (name, uri)
    } else {
        (toks[0].clone(), normalize_uri(&toks[1]))
    };
    app.cfg.upsert(&name, &uri);
    app.cfg.save()?;
    Ok((name, uri))
}

fn acl_setuser(app: &mut App, input: &str) -> Result<String> {
    let toks = tokenize(input).map_err(|e| anyhow!(e))?;
    if toks.is_empty() {
        return Err(anyhow!("usage: <username> [rules...]  e.g. alice on >secret ~* +@read"));
    }
    let name = toks[0].clone();
    let rules: Vec<String> = toks[1..].to_vec();
    let db = app.db.as_mut().ok_or_else(|| anyhow!("not connected"))?;
    db.acl_setuser(&name, &rules)?;
    Ok(format!(
        "ACL SETUSER {name}{}",
        if rules.is_empty() {
            String::new()
        } else {
            format!(" ({} rule(s))", rules.len())
        }
    ))
}

/// Text of `input` after skipping `n` whitespace-separated tokens, preserving
/// the original spacing of the remainder.
fn remainder(input: &str, n: usize) -> Option<String> {
    let mut rest = input.trim_start();
    for _ in 0..n {
        let mut in_quote: Option<char> = None;
        let mut idx = rest.len();
        for (i, c) in rest.char_indices() {
            match (in_quote, c) {
                (None, '"') | (None, '\'') => in_quote = Some(c),
                (Some(q), c) if c == q => in_quote = None,
                (None, ' ') | (None, '\t') => {
                    idx = i;
                    break;
                }
                _ => {}
            }
        }
        rest = rest[idx..].trim_start();
    }
    Some(rest.to_string())
}

/// Execute a `:` command line.
pub fn run_command(app: &mut App, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let line = line.trim_start_matches(':');
    let (head, rest) = match line.split_once(char::is_whitespace) {
        Some((h, r)) => (h.to_lowercase(), r.trim().to_string()),
        None => (line.to_lowercase(), String::new()),
    };

    if let Some(view) = View::from_slug(&head) {
        // `:keys user:*` doubles as a filter shortcut.
        if !rest.is_empty() {
            app.filter = rest.clone();
        }
        app.goto(view);
        if !rest.is_empty() {
            app.on_filter_changed();
            app.info_msg(format!("Filter: {rest}"));
        }
        return;
    }

    match head.as_str() {
        "q" | "quit" | "exit" => app.should_quit = true,
        "connect" | "c" => {
            if rest.is_empty() {
                app.warn("usage: :connect <saved-name|redis-uri>");
                return;
            }
            if let Some(s) = app.cfg.servers.iter().find(|s| s.name == rest).cloned() {
                app.connect(&s.name, &s.uri);
            } else {
                let uri = normalize_uri(&rest);
                app.connect(&rest, &uri);
            }
        }
        "add" | "server" => {
            let r = add_server(app, &rest);
            match r {
                Ok((name, uri)) => {
                    app.ok(format!("Saved server `{name}`"));
                    app.connect(&name, &uri);
                }
                Err(e) => app.error(format_err(&e)),
            }
        }
        "select" | "db" => {
            let r = rest
                .parse::<i64>()
                .map_err(|_| anyhow!("usage: :select <db-index>"))
                .and_then(|idx| {
                    with_db(app, |db| {
                        db.select_db(idx)?;
                        Ok(format!("SELECT {idx}"))
                    })
                });
            let ok = r.is_ok();
            app.report(r);
            if ok {
                app.refresh_current();
            }
        }
        "scan" | "match" => {
            app.scan_pattern = if rest.is_empty() { "*".into() } else { rest.clone() };
            app.info_msg(format!("SCAN MATCH {}", app.scan_pattern));
            app.refresh_current();
        }
        "filter" | "f" => {
            app.filter = rest.clone();
            app.on_filter_changed();
            app.info_msg(if rest.is_empty() {
                "Filter cleared".to_string()
            } else {
                format!("Filter: {rest}")
            });
        }
        "type" => {
            let want = rest.to_lowercase();
            app.type_filter = match want.as_str() {
                "" | "all" | "any" => None,
                "sortedset" | "sorted" | "zset" => Some(KeyKind::ZSet),
                other => match KeyKind::ALL.iter().find(|k| k.redis_name() == other) {
                    Some(k) => Some(*k),
                    None => {
                        app.warn(format!("unknown type `{other}`"));
                        return;
                    }
                },
            };
            app.goto(View::Keys);
            app.on_filter_changed();
            let label = app
                .type_filter
                .map(|t| t.label().to_string())
                .unwrap_or_else(|| "all types".into());
            app.info_msg(format!("Type filter: {label}"));
        }
        "get" | "key" | "open" => {
            if rest.is_empty() {
                app.warn("usage: :get <key>");
                return;
            }
            jump_to_key(app, &rest);
        }
        "set" => {
            let toks = match tokenize(&rest) {
                Ok(t) => t,
                Err(e) => {
                    app.error(e);
                    return;
                }
            };
            if toks.is_empty() {
                app.warn("usage: :set <key> <value>");
                return;
            }
            let key = toks[0].clone();
            let value = remainder(&rest, 1).unwrap_or_default();
            let r = with_db(app, |db| {
                db.set(&key, &value)?;
                Ok(format!("SET {key} ({} bytes)", value.len()))
            });
            app.report(r);
            app.refresh_keys();
            jump_to_key(app, &key);
        }
        "del" | "rm" => {
            if rest.is_empty() {
                app.warn("usage: :del <key>");
                return;
            }
            let key = rest.clone();
            let r = with_db(app, |db| {
                let n = db.del(&key)?;
                if n == 0 {
                    Err(anyhow!("`{key}` did not exist"))
                } else {
                    Ok(format!("Deleted key `{key}`"))
                }
            });
            app.report(r);
            app.refresh_current();
        }
        "publish" | "pub" => {
            let r = publish(app, None, &rest);
            app.report(r);
            app.refresh_channels();
        }
        "subscribe" | "sub" => {
            if rest.is_empty() {
                app.warn("usage: :subscribe <channel>");
                return;
            }
            let pattern = rest.contains('*') || rest.contains('?');
            match app.sub.toggle(&rest, pattern) {
                Ok(true) => app.ok(format!(
                    "{} {rest}",
                    if pattern { "PSUBSCRIBE" } else { "SUBSCRIBE" }
                )),
                Ok(false) => app.ok(format!("Unsubscribed from {rest}")),
                Err(e) => app.error(format_err(&e)),
            }
            app.goto(View::PubSub);
            app.refresh_channels();
        }
        "unsubscribe" | "unsub" => {
            if rest.is_empty() {
                app.sub.stop_all();
                app.ok("Unsubscribed from all channels");
            } else if app.sub.unsubscribe(&rest) {
                app.ok(format!("Unsubscribed from {rest}"));
            } else {
                app.warn(format!("Not subscribed to {rest}"));
            }
            app.refresh_channels();
        }
        "acluser" | "setuser" => {
            let r = acl_setuser(app, &rest);
            app.report(r);
            app.goto(View::Acl);
            app.refresh_acl();
        }
        "cmd" | "raw" | "!" => {
            let r = tokenize(&rest).map_err(|e| anyhow!(e)).and_then(|argv| {
                if argv.is_empty() {
                    return Err(anyhow!("usage: :cmd <redis command>"));
                }
                with_db(app, |db| {
                    let out = db.raw(&argv)?;
                    let first = out.lines().next().unwrap_or("(empty)").to_string();
                    let extra = out.lines().count().saturating_sub(1);
                    Ok(if extra > 0 {
                        format!("{first} (+{extra} more line(s))")
                    } else {
                        first
                    })
                })
            });
            app.report(r);
            app.refresh_current();
        }
        "write" | "w" | "save" => match app.cfg.save() {
            Ok(()) => app.ok(format!("Saved {}", crate::config::Config::path().display())),
            Err(e) => app.error(format_err(&e)),
        },
        "reload" | "r" => app.refresh_current(),
        other => app.set_status(
            Level::Warn,
            format!("Unknown command `:{other}` — press ? for the command list"),
        ),
    }
}

/// Focus a key by exact name, widening the filters if it is hidden.
pub fn jump_to_key(app: &mut App, name: &str) {
    app.goto(View::Keys);
    if !app.keys.iter().any(|k| k.name == name) {
        app.refresh_keys();
    }
    if let Some(entry) = app.keys.iter().find(|k| k.name == name).cloned() {
        if app.type_filter.map(|t| t != entry.kind).unwrap_or(false) {
            app.type_filter = None;
        }
        if !crate::app::matches_filter(name, &app.filter.to_lowercase()) {
            app.filter.clear();
        }
        let visible = app.visible_keys();
        if let Some(pos) = visible.iter().position(|i| app.keys[*i].name == name) {
            app.key_cur.sel = pos;
            app.load_detail();
            app.focus = Focus::Detail;
            app.detail_cur = crate::app::Cursor::default();
            app.ok(format!("Opened `{name}`"));
            return;
        }
    }
    app.warn(format!("No key named `{name}`"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remainder_skips_tokens() {
        assert_eq!(remainder("a b c", 1).unwrap(), "b c");
        assert_eq!(remainder("a b c", 2).unwrap(), "c");
        assert_eq!(remainder("string k hello world", 2).unwrap(), "hello world");
        assert_eq!(remainder(r#"k "a b" c"#, 1).unwrap(), r#""a b" c"#);
    }

    #[test]
    fn ttl_parsing() {
        assert_eq!(parse_ttl("60").unwrap(), 60);
        assert_eq!(parse_ttl("5m").unwrap(), 300);
        assert_eq!(parse_ttl("2h").unwrap(), 7200);
        assert_eq!(parse_ttl("1d").unwrap(), 86400);
        assert_eq!(parse_ttl("-1").unwrap(), -1);
        assert_eq!(parse_ttl("persist").unwrap(), -1);
        assert!(parse_ttl("soon").is_err());
    }

    #[test]
    fn stream_args() {
        let (id, f) = split_stream_args("* a=1 b=2").unwrap();
        assert_eq!(id, "*");
        assert_eq!(f.len(), 2);
        let (id, f) = split_stream_args("a=1").unwrap();
        assert_eq!(id, "*");
        assert_eq!(f, vec![("a".to_string(), "1".to_string())]);
        let (id, _) = split_stream_args("5-1 a=1").unwrap();
        assert_eq!(id, "5-1");
    }
}
