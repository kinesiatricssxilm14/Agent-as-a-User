//! Background Pub/Sub subscriber.
//!
//! Each subscription owns a dedicated Redis connection on its own thread. The
//! connection must stay in subscribed mode for the whole life of the
//! subscription — `redis::PubSub` issues `RESET`/`UNSUBSCRIBE` when it is
//! dropped — so the `PubSub` guard lives on the worker thread's stack and the
//! connection is moved into the thread rather than shared. A short read timeout
//! lets the thread notice a stop request between messages.
//!
//! Received messages land in a shared buffer that the UI drains on each redraw.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

/// How long a blocking read waits before the thread re-checks its stop flag.
const READ_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Debug)]
pub struct Message {
    pub channel: String,
    /// Pattern that matched, when the subscription is a pattern subscription.
    pub via: Option<String>,
    pub payload: String,
    /// Monotonic sequence number, used for display ordering.
    pub seq: u64,
}

struct Sub {
    target: String,
    pattern: bool,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

/// Live subscription set plus the received-message ring buffer.
pub struct Subscriber {
    uri: String,
    subs: Vec<Sub>,
    inbox: Arc<Mutex<Inbox>>,
    capacity: usize,
}

#[derive(Default)]
struct Inbox {
    messages: Vec<Message>,
    seq: u64,
    errors: Vec<String>,
}

impl Subscriber {
    pub fn new(uri: &str) -> Subscriber {
        Subscriber {
            uri: uri.to_string(),
            subs: Vec::new(),
            inbox: Arc::new(Mutex::new(Inbox::default())),
            capacity: 2000,
        }
    }

    /// Point future subscriptions at a different server and drop current ones.
    pub fn retarget(&mut self, uri: &str) {
        self.stop_all();
        self.uri = uri.to_string();
        if let Ok(mut inbox) = self.inbox.lock() {
            inbox.messages.clear();
            inbox.errors.clear();
        }
    }

    pub fn is_subscribed(&self, target: &str) -> bool {
        self.subs.iter().any(|s| s.target == target)
    }

    /// `(target, is_pattern)` for every live subscription.
    pub fn list(&self) -> Vec<(String, bool)> {
        self.subs
            .iter()
            .map(|s| (s.target.clone(), s.pattern))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.subs.len()
    }

    /// Subscribe on a new thread. Blocks until the server has acknowledged the
    /// SUBSCRIBE so that failures are reported to the caller, not swallowed.
    pub fn subscribe(&mut self, target: &str, pattern: bool) -> Result<()> {
        if self.is_subscribed(target) {
            return Ok(());
        }
        let client = redis::Client::open(self.uri.as_str())
            .with_context(|| format!("invalid Redis URI `{}`", self.uri))?;
        let conn = client
            .get_connection_with_timeout(Duration::from_secs(5))
            .with_context(|| format!("cannot connect to {}", self.uri))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let inbox = Arc::clone(&self.inbox);
        let capacity = self.capacity;
        let target_owned = target.to_string();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let handle = thread::Builder::new()
            .name(format!("toolj-sub-{target_owned}"))
            .spawn(move || {
                // `conn` is owned here, so `ps` can borrow it for the whole
                // loop and the subscription survives until the thread exits.
                let mut conn = conn;
                let mut ps = conn.as_pubsub();
                if ps.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
                    let _ = ready_tx.send(Err("cannot set read timeout".into()));
                    return;
                }
                let sub_result = if pattern {
                    ps.psubscribe(&target_owned)
                } else {
                    ps.subscribe(&target_owned)
                };
                if let Err(e) = sub_result {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
                if ready_tx.send(Ok(())).is_err() {
                    return; // caller gave up
                }

                while !stop_thread.load(Ordering::Relaxed) {
                    match ps.get_message() {
                        Ok(msg) => {
                            let channel = msg.get_channel_name().to_string();
                            let via = msg.get_pattern::<Option<String>>().ok().flatten();
                            let payload =
                                String::from_utf8_lossy(msg.get_payload_bytes()).into_owned();
                            if let Ok(mut inbox) = inbox.lock() {
                                inbox.seq += 1;
                                let seq = inbox.seq;
                                inbox.messages.push(Message {
                                    channel,
                                    via,
                                    payload,
                                    seq,
                                });
                                let over = inbox.messages.len().saturating_sub(capacity);
                                if over > 0 {
                                    inbox.messages.drain(0..over);
                                }
                            }
                        }
                        // A read timeout just means "no message yet".
                        Err(e) if e.is_timeout() => {}
                        Err(e) => {
                            if let Ok(mut inbox) = inbox.lock() {
                                inbox.errors.push(format!("{target_owned}: {e}"));
                            }
                            break;
                        }
                    }
                }
            })
            .context("cannot spawn subscriber thread")?;

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                self.subs.push(Sub {
                    target: target.to_string(),
                    pattern,
                    stop,
                    handle: Some(handle),
                });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(anyhow!(
                    "{} {target} failed: {e}",
                    if pattern { "PSUBSCRIBE" } else { "SUBSCRIBE" }
                ))
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err(anyhow!("timed out waiting for the server to confirm the subscription"))
            }
        }
    }

    pub fn unsubscribe(&mut self, target: &str) -> bool {
        if let Some(pos) = self.subs.iter().position(|s| s.target == target) {
            let mut sub = self.subs.remove(pos);
            sub.stop.store(true, Ordering::Relaxed);
            if let Some(h) = sub.handle.take() {
                let _ = h.join();
            }
            true
        } else {
            false
        }
    }

    pub fn toggle(&mut self, target: &str, pattern: bool) -> Result<bool> {
        if self.unsubscribe(target) {
            Ok(false)
        } else {
            self.subscribe(target, pattern)?;
            Ok(true)
        }
    }

    pub fn stop_all(&mut self) {
        for sub in &self.subs {
            sub.stop.store(true, Ordering::Relaxed);
        }
        for mut sub in self.subs.drain(..) {
            if let Some(h) = sub.handle.take() {
                let _ = h.join();
            }
        }
    }

    /// All buffered messages, oldest first.
    pub fn messages(&self) -> Vec<Message> {
        self.inbox
            .lock()
            .map(|i| i.messages.clone())
            .unwrap_or_default()
    }

    pub fn message_count(&self) -> usize {
        self.inbox.lock().map(|i| i.messages.len()).unwrap_or(0)
    }

    pub fn clear_messages(&self) {
        if let Ok(mut inbox) = self.inbox.lock() {
            inbox.messages.clear();
        }
    }

    /// Take pending subscriber errors so the UI can surface them once.
    pub fn take_errors(&self) -> Vec<String> {
        self.inbox
            .lock()
            .map(|mut i| std::mem::take(&mut i.errors))
            .unwrap_or_default()
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.stop_all();
    }
}
