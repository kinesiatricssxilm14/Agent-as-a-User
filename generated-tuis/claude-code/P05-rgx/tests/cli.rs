//! End-to-end tests that run the installed binary against real files.
//!
//! These drive `toole --print`, which shares the matching, offset and
//! replacement code paths with the TUI, so the behaviour the interface shows is
//! what gets asserted here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Path to the binary built by `cargo test`.
fn bin() -> PathBuf {
    // `CARGO_BIN_EXE_<name>` is set by cargo for integration tests.
    PathBuf::from(env!("CARGO_BIN_EXE_toole"))
}

/// A scratch directory unique to one test.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("toole-it-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }

    /// Write `content` to `name` inside the scratch directory.
    fn file(&self, name: &str, content: &str) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, content).expect("write scratch file");
        p
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Run the binary and return its output.
fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("failed to run toole")
}

/// Run and require success, returning stdout.
fn stdout_of(args: &[&str]) -> String {
    let out = run(args);
    assert!(
        out.status.success(),
        "toole {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout should be UTF-8")
}

/// The replaced content between the report's delimiters.
fn replaced_section(report: &str) -> String {
    let start = report
        .find("--- replaced content ---\n")
        .map(|i| i + "--- replaced content ---\n".len())
        .expect("report should contain a replaced-content section");
    let end = report[start..]
        .rfind("--- end ---")
        .map(|i| start + i)
        .expect("report should contain an end marker");
    report[start..end].to_string()
}

/// Parse the offset table into `(index, start, end, line:col, text)` rows.
fn rows(report: &str) -> Vec<(usize, usize, usize, String, String)> {
    let mut out = Vec::new();
    let mut in_table = false;
    for line in report.lines() {
        if line.trim_start().starts_with("#") && line.contains("start") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if line.trim().is_empty() || line.starts_with("---") || line.starts_with("replacement:") {
            break;
        }
        // Capture-group detail rows are indented far further than match rows.
        if line.starts_with("        $") {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(i), Some(s), Some(e), Some(lc)) = (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        let Ok(i) = i.parse() else { continue };
        out.push((
            i,
            s.parse().expect("start should be a number"),
            e.parse().expect("end should be a number"),
            lc.to_string(),
            it.collect::<Vec<_>>().join(" "),
        ));
    }
    out
}

#[test]
fn version_and_help_are_available() {
    let v = stdout_of(&["--version"]);
    assert!(v.contains("toole"), "unexpected version output: {v}");
    let h = stdout_of(&["--help"]);
    // The documented launch flag must be discoverable from the CLI too.
    assert!(h.contains("-f"), "help should document -f");
    assert!(
        h.contains("/bench/data/input.txt"),
        "help should show the default input path"
    );
}

#[test]
fn default_input_path_is_the_documented_one() {
    let h = stdout_of(&["--help"]);
    assert!(
        h.contains("/bench/data/input.txt"),
        "the default input file must be /bench/data/input.txt"
    );
}

#[test]
fn reports_offsets_for_a_simple_pattern() {
    let s = Scratch::new("simple");
    let f = s.file("in.txt", "abc def abc\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "abc", "--print"]);
    let rows = rows(&r);
    assert_eq!(rows.len(), 2);
    // 0-indexed, end exclusive.
    assert_eq!((rows[0].1, rows[0].2), (0, 3));
    assert_eq!((rows[1].1, rows[1].2), (8, 11));
    assert_eq!(rows[0].3, "0:0");
    assert_eq!(rows[1].3, "0:8");
    assert_eq!(rows[0].4, "abc");
}

#[test]
fn offsets_are_zero_indexed_from_the_start_of_the_file() {
    let s = Scratch::new("zero");
    // The very first character must be reported at offset 0.
    let f = s.file("in.txt", "X marks it");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "X", "--print"]);
    let rows = rows(&r);
    assert_eq!((rows[0].1, rows[0].2), (0, 1));
}

#[test]
fn offsets_count_characters_not_bytes() {
    let s = Scratch::new("multibyte");
    // Each CJK character is three bytes but one character.
    let f = s.file("in.txt", "English-only textabcEnglish-only text\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "abc", "--print"]);
    let rows = rows(&r);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].1, rows[0].2),
        (2, 5),
        "offsets must be character-based, not byte-based"
    );
}

#[test]
fn line_and_column_are_reported_per_match() {
    let s = Scratch::new("linecol");
    let f = s.file("in.txt", "one\ntwo\nthree target\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "target", "--print"]);
    let rows = rows(&r);
    assert_eq!(rows[0].3, "2:6");
}

#[test]
fn every_match_appears_in_one_report() {
    let s = Scratch::new("all");
    let body: String = (0..50).map(|i| format!("id{i}\n")).collect();
    let f = s.file("in.txt", &body);
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", r"id\d+", "--print"]);
    assert!(r.contains("matches: 50"));
    assert_eq!(rows(&r).len(), 50);
}

#[test]
fn case_insensitive_via_inline_flag() {
    let s = Scratch::new("inline-i");
    let f = s.file("in.txt", "Error ERROR error eRrOr\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "(?i)error", "--print"]);
    assert!(r.contains("matches: 4"), "got: {r}");
}

#[test]
fn case_insensitive_via_flag_matches_inline_semantics() {
    let s = Scratch::new("flag-i");
    let f = s.file("in.txt", "Error ERROR error eRrOr\n");
    let with_flag = stdout_of(&["-f", f.to_str().unwrap(), "-e", "error", "-i", "--print"]);
    assert!(with_flag.contains("matches: 4"));
    // The report states the pattern actually handed to the engine.
    assert!(with_flag.contains("effective: (?i)error"));
}

#[test]
fn case_insensitive_match_inside_a_token() {
    let s = Scratch::new("midtoken");
    // The requirement calls out matches that sit in the middle of a token.
    let f = s.file("in.txt", "xxERRORxx preERRORpost harmless\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "(?i)err", "--print"]);
    let rows = rows(&r);
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].1, rows[0].2), (2, 5));
    assert_eq!(rows[0].4, "ERR");
    assert_eq!((rows[1].1, rows[1].2), (13, 16));
}

#[test]
fn china_mobile_numbers_are_matched_and_near_misses_are_not() {
    let s = Scratch::new("mobile");
    let f = s.file(
        "in.txt",
        "Zhang 13812345678\nLi 15900001111\nBad 12345678901\nShort 1381234567\n",
    );
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", r"1[3-9]\d{9}", "--print"]);
    let rows = rows(&r);
    let texts: Vec<&str> = rows.iter().map(|r| r.4.as_str()).collect();
    assert_eq!(texts, vec!["13812345678", "15900001111"]);
    // First match starts at character 6 of the file.
    assert_eq!((rows[0].1, rows[0].2), (6, 17));
}

#[test]
fn china_mobile_with_country_code_and_separators() {
    let s = Scratch::new("mobile86");
    let f = s.file("in.txt", "+86 13812345678 and 86-15900001111\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        r"(?:\+?86[-\s]?)?1[3-9]\d{9}",
        "--print",
    ]);
    assert!(r.contains("matches: 2"), "got: {r}");
}

#[test]
fn china_landline_numbers_are_matched() {
    let s = Scratch::new("landline");
    let f = s.file("in.txt", "Beijing 010-88886666 Shanghai 021-59876543\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        r"0\d{2,3}-\d{7,8}",
        "--print",
    ]);
    let rows = rows(&r);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].4, "010-88886666");
    assert_eq!(rows[1].4, "021-59876543");
}

#[test]
fn replacement_prints_the_complete_file_including_untouched_lines() {
    let s = Scratch::new("repl-full");
    let f = s.file("in.txt", "keep one\nfoo two\nkeep three\nfoo four\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        "foo",
        "-r",
        "BAR",
        "--print",
    ]);
    assert_eq!(
        replaced_section(&r),
        "keep one\nBAR two\nkeep three\nBAR four\n"
    );
}

#[test]
fn replacement_expands_capture_groups() {
    let s = Scratch::new("repl-groups");
    let f = s.file("in.txt", "2024-05-06\n2023-01-02\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        r"(\d{4})-(\d{2})-(\d{2})",
        "-r",
        "$3/$2/$1",
        "--print",
    ]);
    assert_eq!(replaced_section(&r), "06/05/2024\n02/01/2023\n");
}

#[test]
fn replacement_expands_named_groups() {
    let s = Scratch::new("repl-named");
    let f = s.file("in.txt", "bob@corp\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        r"(?P<user>\w+)@(?P<host>\w+)",
        "-r",
        "${host}!${user}",
        "--print",
    ]);
    assert_eq!(replaced_section(&r), "corp!bob\n");
    // Group names are reported alongside the match.
    assert!(r.contains("(user)"), "got: {r}");
    assert!(r.contains("(host)"));
}

#[test]
fn replacement_first_only() {
    let s = Scratch::new("repl-first");
    let f = s.file("in.txt", "a a a\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        "a",
        "-r",
        "b",
        "--first-only",
        "--print",
    ]);
    assert_eq!(replaced_section(&r), "b a a\n");
    assert!(r.contains("first only"));
}

#[test]
fn replacement_on_a_multibyte_file() {
    let s = Scratch::new("repl-mb");
    let f = s.file("in.txt", "English-only text 100 English-only text\nEnglish-only text 250 English-only text\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        r"\d+",
        "-r",
        "N",
        "--print",
    ]);
    assert_eq!(replaced_section(&r), "English-only text N English-only text\nEnglish-only text N English-only text\n");
}

#[test]
fn case_insensitive_replacement_together() {
    let s = Scratch::new("repl-i");
    let f = s.file("in.txt", "Error here\nERROR there\nfine\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        "(?i)error",
        "-r",
        "WARN",
        "--print",
    ]);
    assert_eq!(replaced_section(&r), "WARN here\nWARN there\nfine\n");
}

#[test]
fn literal_mode_disables_metacharacters() {
    let s = Scratch::new("literal");
    let f = s.file("in.txt", "a.c abc\n");
    let loose = stdout_of(&["-f", f.to_str().unwrap(), "-e", "a.c", "--print"]);
    assert!(loose.contains("matches: 2"));
    let strict = stdout_of(&["-f", f.to_str().unwrap(), "-e", "a.c", "-F", "--print"]);
    assert!(strict.contains("matches: 1"), "got: {strict}");
}

#[test]
fn multiline_flag_changes_anchor_behaviour() {
    let s = Scratch::new("multiline");
    let f = s.file("in.txt", "alpha\nbeta\ngamma\n");
    let without = stdout_of(&["-f", f.to_str().unwrap(), "-e", "^beta", "--print"]);
    assert!(without.contains("matches: 0"));
    let with = stdout_of(&["-f", f.to_str().unwrap(), "-e", "^beta", "-M", "--print"]);
    assert!(with.contains("matches: 1"), "got: {with}");
}

#[test]
fn lookahead_falls_back_to_the_backtracking_engine() {
    let s = Scratch::new("lookahead");
    let f = s.file("in.txt", "foobar foobaz\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "foo(?=bar)", "--print"]);
    assert!(r.contains("engine: fancy"), "got: {r}");
    assert!(r.contains("matches: 1"));
}

#[test]
fn backreference_patterns_work() {
    let s = Scratch::new("backref");
    let f = s.file("in.txt", "the the quick brown fox fox\n");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        r"\b(\w+)\s+\1\b",
        "--print",
    ]);
    assert!(r.contains("engine: fancy"));
    let rows = rows(&r);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].4, "the the");
}

#[test]
fn zero_width_matches_are_reported_with_equal_offsets() {
    let s = Scratch::new("zerowidth");
    let f = s.file("in.txt", "ab");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", r"\b", "--print"]);
    let rows = rows(&r);
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].1, rows[0].2), (0, 0));
    assert_eq!((rows[1].1, rows[1].2), (2, 2));
}

#[test]
fn invalid_pattern_exits_non_zero_with_a_message() {
    let s = Scratch::new("badpat");
    let f = s.file("in.txt", "text\n");
    let out = run(&["-f", f.to_str().unwrap(), "-e", "a(", "--print"]);
    assert!(!out.status.success(), "an invalid pattern must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("toole:"), "stderr should name the tool: {err}");
    assert!(
        err.to_lowercase().contains("group") || err.to_lowercase().contains("pattern"),
        "stderr should explain the problem: {err}"
    );
}

#[test]
fn missing_input_file_exits_non_zero_and_names_the_path() {
    let out = run(&["-f", "/nonexistent/toole/input.txt", "--print"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("/nonexistent/toole/input.txt"),
        "stderr should name the missing file: {err}"
    );
}

#[test]
fn no_pattern_still_reports_the_file() {
    let s = Scratch::new("nopat");
    let f = s.file("in.txt", "one\ntwo\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "--print"]);
    assert!(r.contains("lines: 2"));
    assert!(r.contains("no pattern given"));
}

#[test]
fn empty_file_is_handled_gracefully() {
    let s = Scratch::new("empty");
    let f = s.file("in.txt", "");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "x", "--print"]);
    assert!(r.contains("matches: 0"), "got: {r}");
}

#[test]
fn file_without_a_trailing_newline() {
    let s = Scratch::new("nonewline");
    let f = s.file("in.txt", "last line has no newline");
    let r = stdout_of(&[
        "-f",
        f.to_str().unwrap(),
        "-e",
        "newline",
        "-r",
        "NL",
        "--print",
    ]);
    assert_eq!(replaced_section(&r), "last line has no NL\n");
}

#[test]
fn crlf_file_keeps_its_terminators_through_replacement() {
    let s = Scratch::new("crlf");
    let f = s.file("in.txt", "one\r\ntwo\r\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "two", "-r", "2", "--print"]);
    assert_eq!(
        replaced_section(&r),
        "one\r\n2\r\n",
        "CRLF terminators must survive a replacement"
    );
}

#[test]
fn invalid_utf8_does_not_crash() {
    let s = Scratch::new("badutf8");
    let p = s.path().join("bad.bin");
    fs::write(&p, b"good \xff\xfe bytes\n").expect("write");
    let r = stdout_of(&["-f", p.to_str().unwrap(), "-e", "bytes", "--print"]);
    assert!(r.contains("matches: 1"), "got: {r}");
}

#[test]
fn tabs_are_preserved_in_replacement_output() {
    let s = Scratch::new("tabs");
    let f = s.file("in.txt", "a\tb\tc\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "b", "-r", "B", "--print"]);
    assert_eq!(replaced_section(&r), "a\tB\tc\n");
}

#[test]
fn presets_can_be_listed_and_all_compile() {
    let listed = stdout_of(&["--list-presets"]);
    assert!(listed.contains("CN mobile number"), "got: {listed}");
    // Every listed preset must be usable as an actual pattern.
    let s = Scratch::new("presets");
    let f = s.file("in.txt", "13812345678 anonymous@example.invalid 10.0.0.1 2024-05-06 error\n");
    for line in listed.lines().skip(2) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // The pattern is the last whitespace-delimited field of each row.
        let Some(pat) = line.rsplit("  ").next() else {
            continue;
        };
        let pat = pat.trim();
        if pat.is_empty() {
            continue;
        }
        let out = run(&["-f", f.to_str().unwrap(), "-e", pat, "--print"]);
        assert!(
            out.status.success(),
            "listed preset {pat:?} failed to run: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn tab_width_option_is_accepted_and_validated() {
    let s = Scratch::new("tabwidth");
    let f = s.file("in.txt", "a\tb\n");
    assert!(
        run(&["-f", f.to_str().unwrap(), "--tab-width", "8", "--print"])
            .status
            .success()
    );
    // Out-of-range values are rejected rather than silently clamped.
    assert!(
        !run(&["-f", f.to_str().unwrap(), "--tab-width", "99", "--print"])
            .status
            .success()
    );
}

#[test]
fn piped_stdout_produces_the_report_without_a_tty() {
    // `--print` is implied when stdout is not a terminal, which is exactly the
    // situation these tests run in.
    let s = Scratch::new("piped");
    let f = s.file("in.txt", "hit\n");
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", "hit"]);
    assert!(r.contains("matches: 1"), "got: {r}");
}

#[test]
fn large_file_completes_quickly() {
    let s = Scratch::new("large");
    let body: String = (0..20_000)
        .map(|i| format!("line {i} value=13800{:06}\n", i % 1000))
        .collect();
    let f = s.file("big.txt", &body);
    let start = std::time::Instant::now();
    let r = stdout_of(&["-f", f.to_str().unwrap(), "-e", r"1[3-9]\d{9}", "--print"]);
    let elapsed = start.elapsed();
    assert!(r.contains("matches: 20000"), "should match every line");
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "20k matches took {elapsed:?}, which suggests a performance problem"
    );
}
