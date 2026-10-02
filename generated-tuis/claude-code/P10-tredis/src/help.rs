//! In-app key documentation. The help view renders these lines verbatim, and
//! the footer hints are derived from the same source of truth.

pub enum HelpLine {
    Section(&'static str),
    /// `keys`, `description`
    Key(&'static str, &'static str),
    Blank,
}

pub fn help_lines() -> &'static [HelpLine] {
    use HelpLine::*;
    &[
        Section("GLOBAL"),
        Key("?  F1", "toggle this help page"),
        Key(":", "resource selector / command line (e.g. `:keys`, `:acl`)"),
        Key("1 2 3 4 5 6", "jump to Keys / Streams / Pub-Sub / ACL / Servers / Info"),
        Key("Tab  Shift-Tab", "next / previous resource view"),
        Key("h  l  ←  →", "move focus between the list and the detail pane"),
        Key("↑ ↓  k  j", "move selection up / down"),
        Key("PgUp PgDn  Ctrl-u Ctrl-d", "move selection by a page"),
        Key("g  G  Home  End", "jump to first / last row"),
        Key("/", "filter the current list by name (glob or substring)"),
        Key("Esc", "close prompt / clear filter / leave help"),
        Key("r  F5", "reload the current view from Redis"),
        Key("q  Ctrl-c", "quit"),
        Blank,
        Section("KEYS  (:keys)"),
        Key("Enter", "load the highlighted key and focus the detail pane"),
        Key("t  T", "cycle the type filter forward / backward"),
        Key("s", "set the server-side SCAN MATCH pattern"),
        Key("n", "create a new key (type is chosen in the prompt)"),
        Key("e", "edit the selected value; in the detail pane edits that item"),
        Key("a", "add a field / element / member to the selected key"),
        Key("d", "delete the selected key (asks for confirmation)"),
        Key("D", "delete the highlighted item inside the key"),
        Key("m", "rename the selected key"),
        Key("x", "set or clear the TTL of the selected key"),
        Key("y", "copy the selected key name into the command line"),
        Blank,
        Section("STREAMS  (:streams)"),
        Key("Enter", "focus the message pane of the selected stream"),
        Key("a", "XADD an entry (`* f=v ...` or an explicit id)"),
        Key("D", "XDEL the highlighted entry"),
        Key("d", "delete the whole stream key"),
        Blank,
        Section("PUB/SUB  (:pubsub)"),
        Key("Enter  space", "subscribe / unsubscribe to the selected channel"),
        Key("P", "subscribe to a glob pattern (PSUBSCRIBE)"),
        Key("p", "publish a message to the selected channel"),
        Key("c", "clear the received-message buffer"),
        Key("u", "unsubscribe from every channel"),
        Blank,
        Section("ACL  (:acl)"),
        Key("a", "create or modify a user (`name on >pass ~* +@all`)"),
        Key("e", "edit the selected user's rules (pre-filled)"),
        Key("d", "delete the selected user"),
        Blank,
        Section("SERVERS  (:servers)"),
        Key("Enter", "connect to the highlighted server"),
        Key("a", "add a named server (`name redis://host:port/db`)"),
        Key("e", "edit the highlighted server entry"),
        Key("d", "remove the highlighted server from the list"),
        Key("w", "save the server list to disk"),
        Blank,
        Section("COMMAND LINE  (:)"),
        Key(":keys :streams :pubsub", "switch to the keys / streams / pubsub view"),
        Key(":acl :servers :info :help", "switch to acl / servers / server info / help"),
        Key(":connect <name|uri>", "connect to a saved name or a raw URI"),
        Key(":add <name> <uri>", "add a named server and connect to it"),
        Key(":select <db>", "SELECT a different logical database"),
        Key(":scan <pattern>", "set the SCAN MATCH pattern"),
        Key(":filter <text>", "set the client-side name filter"),
        Key(":get <key>", "jump to a key by exact name"),
        Key(":del <key>", "delete a key by name"),
        Key(":publish <chan> <msg>", "PUBLISH a message"),
        Key(":subscribe <chan>", "SUBSCRIBE to a channel"),
        Key(":cmd <redis command>", "run any Redis command and show the reply"),
        Key(":q", "quit"),
        Blank,
        Section("PROMPTS"),
        Key("Enter", "confirm"),
        Key("Esc  Ctrl-c", "cancel"),
        Key("← → Home End", "move the caret"),
        Key("Ctrl-w  Ctrl-u  Ctrl-k", "delete word / to start / to end"),
        Key("↑ ↓", "recall command history (command line only)"),
        Key("y / n", "answer a confirmation question"),
    ]
}

/// Column width the help view reserves for the key column.
pub const KEY_COL: usize = 26;

/// Short, context-dependent hints for the footer.
pub fn footer_hints(view: crate::app::View, focus: crate::app::Focus) -> &'static str {
    use crate::app::{Focus, View};
    match (view, focus) {
        (View::Keys, Focus::List) => {
            "↑↓ select · Enter open · t type · / filter · s scan · n new · e edit · a add · d del · m rename · x ttl · r reload · ? help"
        }
        (View::Keys, Focus::Detail) => {
            "↑↓ item · h back to list · e edit item · a add · D delete item · d del key · r reload · ? help"
        }
        (View::Streams, Focus::List) => {
            "↑↓ select · Enter messages · a XADD · d del stream · / filter · r reload · ? help"
        }
        (View::Streams, Focus::Detail) => {
            "↑↓ entry · h back to list · a XADD · D XDEL entry · r reload · ? help"
        }
        (View::PubSub, Focus::List) => {
            "↑↓ select · Enter sub/unsub · P pattern · p publish · u unsub all · c clear · / filter · ? help"
        }
        (View::PubSub, Focus::Detail) => {
            "↑↓ message · h back to list · c clear buffer · p publish · u unsub all · ? help"
        }
        (View::Acl, Focus::List) => {
            "↑↓ select · a new user · e edit rules · d delete · / filter · r reload · ? help"
        }
        (View::Acl, Focus::Detail) => "↑↓ attribute · h back to list · e edit rules · ? help",
        (View::Servers, _) => {
            "↑↓ select · Enter connect · a add · e edit · d remove · w save · ? help"
        }
        (View::Info, _) => "↑↓ scroll · / filter · r reload · ? help",
        (View::Help, _) => "↑↓ scroll · Esc / ? close help · q quit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The help view lays keys out in a fixed-width column; an over-long key
    /// string would collide with its description text.
    #[test]
    fn key_column_fits() {
        for line in help_lines() {
            if let HelpLine::Key(k, _) = line {
                assert!(
                    crate::util::width(k) < KEY_COL,
                    "`{k}` is {} cols, needs < {KEY_COL}",
                    crate::util::width(k)
                );
            }
        }
    }

    #[test]
    fn every_view_has_footer_hints() {
        use crate::app::{Focus, View};
        for v in View::TABS.iter().chain([View::Help].iter()) {
            for f in [Focus::List, Focus::Detail] {
                assert!(!footer_hints(*v, f).is_empty());
            }
        }
    }
}
