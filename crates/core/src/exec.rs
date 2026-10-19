//! Running external tools (winget, choco, scoop, powershell) without flashing
//! a console window.
//!
//! OWNER: package-managers agent.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use regex::Regex;

use crate::model::CommandSpec;

/// Lines kept by [`run_streaming`].
const TAIL_LINES: usize = 200;
/// How long output is still collected once the process has exited: an
/// installer it started may have inherited the pipes and keep them open.
const PIPE_GRACE: Duration = Duration::from_secs(2);
/// Polling interval of the wait loop.
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct Output {
    /// Exit code as an unsigned 32-bit value widened to i64, so winget HRESULTs
    /// read naturally (0x8A150101 = 2316632321), never negative.
    pub code: i64,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// The last `count` non-empty lines of stdout then stderr, cleaned and
    /// joined with ` / `, for error messages.
    pub(crate) fn excerpt(&self, count: usize) -> String {
        let lines: Vec<String> = self.stdout.lines().chain(self.stderr.lines()).filter_map(clean_output_line).collect();
        lines[lines.len().saturating_sub(count)..].join(" / ")
    }
}

/// Error returned by [`run`] and [`run_streaming`] when the process had to be
/// killed because it did not finish in time. Callers can recognize it with
/// `err.downcast_ref::<TimedOut>()`.
#[derive(Debug, Clone)]
pub struct TimedOut {
    pub program: String,
    pub timeout: Duration,
    /// Last cleaned output lines before the process was killed.
    pub tail: Vec<String>,
}

impl fmt::Display for TimedOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} did not finish within {} s and was stopped", self.program, self.timeout.as_secs())
    }
}

impl std::error::Error for TimedOut {}

/// Runs `program args` to completion with `CREATE_NO_WINDOW`, stdin closed,
/// output decoded as UTF-8 (lossy). The process is killed and an error
/// returned when `timeout` elapses.
pub fn run(program: &str, args: &[&str], timeout: Duration) -> anyhow::Result<Output> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = supervise(program, args, timeout, &mut |pipe, data| match pipe {
        Pipe::Stdout => stdout.extend_from_slice(data),
        Pipe::Stderr => stderr.extend_from_slice(data),
    })?;
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    match code {
        Some(code) => Ok(Output { code, stdout, stderr }),
        None => {
            let mut tail = Tail::default();
            for line in stdout.split(['\r', '\n']).chain(stderr.split(['\r', '\n'])) {
                if let Some(line) = clean_output_line(line) {
                    tail.push(line);
                }
            }
            Err(TimedOut { program: program.to_string(), timeout, tail: tail.into_vec() }.into())
        }
    }
}

/// Runs a long command (an installation), calling `on_line` for every
/// cleaned output line as soon as it arrives (stdout and stderr merged; lines
/// are split on `\r` *and* `\n`, trimmed, empty and pure-spinner lines dropped).
///
/// Returns the exit code (same convention as [`Output::code`]) and the last
/// 200 cleaned lines (consecutive progress lines are collapsed into the most
/// recent one). Kills the process on timeout and returns an error.
pub fn run_streaming(
    spec: &CommandSpec,
    timeout: Duration,
    on_line: &mut dyn FnMut(&str),
) -> anyhow::Result<(i64, Vec<String>)> {
    let mut tail = Tail::default();
    let mut splitters = [LineSplitter::default(), LineSplitter::default()];
    let mut deliver = |line: String| {
        on_line(&line);
        tail.push(line);
    };
    let code = supervise(&spec.program, &spec.args, timeout, &mut |pipe, data| {
        splitters[pipe as usize].feed(data, &mut deliver);
    })?;
    for splitter in &mut splitters {
        splitter.flush(&mut deliver);
    }
    let tail = tail.into_vec();
    match code {
        Some(code) => Ok((code, tail)),
        None => Err(TimedOut { program: spec.program.clone(), timeout, tail }.into()),
    }
}

/// Progress information found in one output line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineProgress {
    /// 0.0..=1.0
    pub fraction: f32,
    /// Bytes done / total when the line shows sizes.
    pub done_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

/// `12.5 MB / 30.0 MB`, `12,5 Mo / 30,0 Mo`, `512 KiB / 2 GiB`, `1 B / 3 o`...
static SIZES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d+(?:[.,]\d+)?)\s*([kmgt]i?[bo]|[bo])\s*/\s*(\d+(?:[.,]\d+)?)\s*([kmgt]i?[bo]|[bo])\b")
        .expect("valid regex")
});

/// `45%`, `12.5 %`.
static PERCENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:[.,]\d+)?)\s*%").expect("valid regex"));

/// Extracts progress from one output line, if it holds some:
/// `██████▒▒▒▒  12.5 MB / 30.0 MB` (also `12,5 Mo / 30,0 Mo`, KB/Ko, GB/Go,
/// B/o, KiB...), `45%`, `Progress: 45%`. Returns `None` otherwise.
pub fn parse_progress(line: &str) -> Option<LineProgress> {
    if let Some(caps) = SIZES.captures_iter(line).last() {
        if let (Some(done), Some(total)) = (decimal(&caps[1]), decimal(&caps[3])) {
            let done = done * unit_bytes(&caps[2]);
            let total = total * unit_bytes(&caps[4]);
            if total > 0.0 {
                return Some(LineProgress {
                    fraction: clamp_fraction(done / total),
                    done_bytes: Some(done.round() as u64),
                    total_bytes: Some(total.round() as u64),
                });
            }
        }
    }
    let caps = PERCENT.captures_iter(line).last()?;
    let percent = decimal(&caps[1])?;
    Some(LineProgress { fraction: clamp_fraction(percent / 100.0), done_bytes: None, total_bytes: None })
}

/// Parses `12.5` or `12,5`.
fn decimal(s: &str) -> Option<f64> {
    s.replace(',', ".").parse().ok()
}

/// Bytes per unit; Windows tools (winget included) use powers of 1024.
fn unit_bytes(unit: &str) -> f64 {
    let power = match unit.bytes().next().map(|b| b.to_ascii_lowercase()) {
        Some(b'k') => 1,
        Some(b'm') => 2,
        Some(b'g') => 3,
        Some(b't') => 4,
        _ => 0,
    };
    1024f64.powi(power)
}

fn clamp_fraction(value: f64) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        (value as f32).clamp(0.0, 1.0)
    }
}

/// What a terminal would show for one physical line (without its `\n`): the
/// text after the last carriage return (or cursor-to-column-1 / erase-line
/// sequence), with backspaces applied and every other escape sequence or
/// control character removed.
pub(crate) fn visible_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => out.clear(),
            '\u{8}' => {
                out.pop();
            }
            '\u{1b}' => match chars.next() {
                // CSI: parameters, then a final character in '@'..='~'.
                Some('[') => {
                    let mut params = String::new();
                    for p in chars.by_ref() {
                        if ('@'..='~').contains(&p) {
                            let back_to_start = p == 'G' && matches!(params.as_str(), "" | "0" | "1");
                            let erase_line = p == 'K' && params == "2";
                            if back_to_start || erase_line {
                                out.clear();
                            }
                            break;
                        }
                        params.push(p);
                    }
                }
                // OSC (taskbar progress, hyperlinks...): up to BEL or ESC '\'.
                Some(']') => {
                    while let Some(p) = chars.next() {
                        if p == '\u{7}' {
                            break;
                        }
                        if p == '\u{1b}' {
                            chars.next_if_eq(&'\\');
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// One cleaned output line, or `None` for empty and pure-spinner lines.
fn clean_output_line(raw: &str) -> Option<String> {
    let text = visible_text(raw);
    let text = text.trim();
    (!text.is_empty() && !is_spinner(text)).then(|| text.to_string())
}

/// Lines made only of spinner and progress-bar glyphs (no digits, no words).
fn is_spinner(text: &str) -> bool {
    text.chars().all(|c| {
        c.is_whitespace()
            || matches!(c, '-' | '\\' | '|' | '/' | '=' | '#' | '>' | '<' | '[' | ']' | '.' | '*' | '·' | '•')
            // Box drawing, block elements and geometric shapes.
            || ('\u{2500}'..='\u{25FF}').contains(&c)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pipe {
    Stdout = 0,
    Stderr = 1,
}

/// Starts `program`, hands its output to `on_data` as it arrives and waits for
/// it. Returns the exit code, or `None` when it was killed after `timeout`.
fn supervise<S: AsRef<OsStr>>(
    program: &str,
    args: &[S],
    timeout: Duration,
    on_data: &mut dyn FnMut(Pipe, &[u8]),
) -> anyhow::Result<Option<i64>> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_window(&mut command);
    let mut child = command.spawn().with_context(|| format!("cannot start {program}"))?;

    let (tx, rx) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        forward(stdout, Pipe::Stdout, tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        forward(stderr, Pipe::Stderr, tx.clone());
    }
    // Only the reader threads hold senders now: the channel disconnects once
    // both pipes are closed.
    drop(tx);

    let deadline = Instant::now() + timeout;
    let mut exited: Option<(i64, Instant)> = None;
    let mut pipes_open = true;
    loop {
        if pipes_open {
            match rx.recv_timeout(POLL) {
                Ok((pipe, data)) => on_data(pipe, &data),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => pipes_open = false,
            }
        } else if exited.is_none() {
            thread::sleep(POLL);
        }

        if exited.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => exited = Some((exit_code(status), Instant::now() + PIPE_GRACE)),
                Ok(None) => {}
                Err(err) => {
                    let _ = child.kill();
                    return Err(err).with_context(|| format!("waiting for {program}"));
                }
            }
        }

        match exited {
            Some((code, grace_end)) => {
                if !pipes_open || Instant::now() >= grace_end {
                    return Ok(Some(code));
                }
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
            }
        }
    }
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    /// Console programs started with this flag get no console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

/// Reads `source` on a dedicated thread until EOF, sending every chunk.
fn forward(mut source: impl Read + Send + 'static, pipe: Pipe, tx: Sender<(Pipe, Vec<u8>)>) {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match source.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send((pipe, buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    });
}

/// Windows exit codes are DWORDs, which std exposes as `i32`.
fn exit_code(status: ExitStatus) -> i64 {
    status.code().map_or(i64::from(u32::MAX), |code| i64::from(code as u32))
}

/// Splits a byte stream into lines on `\r` and `\n`. Works on bytes so that
/// UTF-8 sequences cut between two reads are decoded whole.
#[derive(Default)]
struct LineSplitter {
    pending: Vec<u8>,
}

impl LineSplitter {
    fn feed(&mut self, data: &[u8], deliver: &mut dyn FnMut(String)) {
        for &byte in data {
            if byte == b'\r' || byte == b'\n' {
                self.flush(deliver);
            } else {
                self.pending.push(byte);
            }
        }
    }

    fn flush(&mut self, deliver: &mut dyn FnMut(String)) {
        if self.pending.is_empty() {
            return;
        }
        let raw = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        if let Some(line) = clean_output_line(&raw) {
            deliver(line);
        }
    }
}

/// The last [`TAIL_LINES`] lines, without consecutive duplicates; a run of
/// progress lines only keeps the most recent one.
#[derive(Default)]
struct Tail {
    lines: VecDeque<String>,
    last_is_progress: bool,
}

impl Tail {
    fn push(&mut self, line: String) {
        let progress = parse_progress(&line).is_some();
        if progress && self.last_is_progress {
            if let Some(last) = self.lines.back_mut() {
                *last = line;
            }
            return;
        }
        if self.lines.back() == Some(&line) {
            return;
        }
        self.lines.push_back(line);
        self.last_is_progress = progress;
        if self.lines.len() > TAIL_LINES {
            self.lines.pop_front();
        }
    }

    fn into_vec(self) -> Vec<String> {
        self.lines.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(line: &str) -> Option<(f32, Option<u64>, Option<u64>)> {
        parse_progress(line).map(|p| (p.fraction, p.done_bytes, p.total_bytes))
    }

    #[test]
    fn progress_with_sizes() {
        let (f, done, total) = progress("  ██████████▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒  12.5 MB / 30.0 MB").unwrap();
        assert!((f - 12.5 / 30.0).abs() < 1e-4);
        assert_eq!(done, Some(13_107_200));
        assert_eq!(total, Some(31_457_280));

        let (f, done, total) = progress("12,5 Mo / 30,0 Mo").unwrap();
        assert!((f - 12.5 / 30.0).abs() < 1e-4);
        assert_eq!(done, Some(13_107_200));
        assert_eq!(total, Some(31_457_280));
    }

    #[test]
    fn progress_with_mixed_units() {
        let (f, done, total) = progress("  ▒▒▒▒▒▒▒▒  512 KB / 2.00 MB").unwrap();
        assert!((f - 0.25).abs() < 1e-4);
        assert_eq!(done, Some(524_288));
        assert_eq!(total, Some(2_097_152));

        assert_eq!(progress("300 o / 600 o").map(|p| p.2), Some(Some(600)));
        assert_eq!(progress("1 B / 4 B").map(|p| p.0), Some(0.25));
        assert_eq!(progress("1 Ko / 2 Ko").map(|p| p.2), Some(Some(2048)));
        assert_eq!(progress("1 Go / 4 Go").map(|p| p.0), Some(0.25));
        assert_eq!(progress("1.5 GB / 3 GB").map(|p| p.2), Some(Some(3 * 1024 * 1024 * 1024)));
        assert_eq!(progress("100 KiB / 1 MiB").map(|p| p.1), Some(Some(102_400)));
        assert_eq!(progress("1 GiB / 2 GiB").map(|p| p.0), Some(0.5));
    }

    #[test]
    fn progress_percent() {
        assert_eq!(progress("45%"), Some((0.45, None, None)));
        assert_eq!(progress("Progress: Downloading python313 3.13.16... 45%"), Some((0.45, None, None)));
        assert_eq!(progress("  ███████████████▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒  50%"), Some((0.5, None, None)));
        assert_eq!(progress("12,5 %"), Some((0.125, None, None)));
    }

    #[test]
    fn progress_is_clamped() {
        assert_eq!(progress("140%").map(|p| p.0), Some(1.0));
        assert_eq!(progress("40 MB / 30 MB").map(|p| p.0), Some(1.0));
    }

    #[test]
    fn no_progress() {
        assert_eq!(parse_progress("Successfully installed"), None);
        assert_eq!(parse_progress("Downloading https://example.com/setup-1.2.3.exe"), None);
        assert_eq!(parse_progress("Version 2 / 3 bottles"), None);
        assert_eq!(parse_progress("  12.5 MB"), None);
        assert_eq!(parse_progress("0 B / 0 B"), None);
    }

    #[test]
    fn visible_text_applies_terminal_controls() {
        assert_eq!(visible_text("   - \r   \\ \r   | \rName  Id"), "Name  Id");
        assert_eq!(visible_text("-\u{8}\\\u{8}|\u{8}/\u{8} \u{8}Name"), "Name");
        assert_eq!(visible_text("\u{1b}[2K\u{1b}[0GDone \u{1b}[32mok\u{1b}[0m"), "Done ok");
        assert_eq!(visible_text("\u{1b}]9;4;1;45\u{1b}\\  50%"), "  50%");
        assert_eq!(visible_text("\u{1b}]9;4;0;\u{7}text\tmore"), "text more");
        assert_eq!(visible_text("spinner\rclean"), "clean");
    }

    #[test]
    fn spinner_lines_are_dropped() {
        assert_eq!(clean_output_line("  - "), None);
        assert_eq!(clean_output_line(" \\ "), None);
        assert_eq!(clean_output_line("|"), None);
        assert_eq!(clean_output_line("/"), None);
        assert_eq!(clean_output_line("  ██████▒▒▒▒▒▒▒▒"), None);
        assert_eq!(clean_output_line("   "), None);
        assert_eq!(
            clean_output_line("  ██████▒▒▒▒  12.5 MB / 30.0 MB").as_deref(),
            Some("██████▒▒▒▒  12.5 MB / 30.0 MB")
        );
        assert_eq!(clean_output_line("  Found Git [Git.Git] ").as_deref(), Some("Found Git [Git.Git]"));
    }

    #[test]
    fn splitter_handles_cr_lf_and_split_utf8() {
        let mut lines = Vec::new();
        let mut deliver = |line: String| lines.push(line);
        let mut splitter = LineSplitter::default();
        let text = "Téléchargement\r\n  ██▒▒  1 MB / 2 MB\r  ███▒  2 MB / 2 MB\r\n-\r\\\r|\rFini";
        let bytes = text.as_bytes();
        // Cut inside the two-byte 'é' and inside a block glyph.
        for chunk in [&bytes[..2], &bytes[2..21], &bytes[21..]] {
            splitter.feed(chunk, &mut deliver);
        }
        splitter.flush(&mut deliver);
        assert_eq!(lines, ["Téléchargement", "██▒▒  1 MB / 2 MB", "███▒  2 MB / 2 MB", "Fini"]);
    }

    #[test]
    fn tail_collapses_progress_and_keeps_last_lines() {
        let mut tail = Tail::default();
        tail.push("Downloading".into());
        tail.push("10%".into());
        tail.push("50%".into());
        tail.push("100%".into());
        tail.push("Installing".into());
        tail.push("Installing".into());
        assert_eq!(tail.into_vec(), ["Downloading", "100%", "Installing"]);

        let mut tail = Tail::default();
        for i in 0..250 {
            tail.push(format!("line {i}"));
        }
        let lines = tail.into_vec();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[0], "line 50");
        assert_eq!(lines[TAIL_LINES - 1], "line 249");
    }

    #[cfg(windows)]
    fn cmd(script: &str) -> CommandSpec {
        CommandSpec { program: "cmd".into(), args: vec!["/d".into(), "/c".into(), script.into()] }
    }

    #[cfg(windows)]
    #[test]
    fn run_captures_output_and_exit_code() {
        let out = run("cmd", &["/d", "/c", "echo hello& echo oops 1>&2& exit 3010"], Duration::from_secs(30)).unwrap();
        assert_eq!(out.code, 3010);
        assert_eq!(out.stdout.trim(), "hello");
        assert_eq!(out.stderr.trim(), "oops");
    }

    #[cfg(windows)]
    #[test]
    fn exit_codes_are_unsigned() {
        let out = run("cmd", &["/d", "/c", "exit -1978334975"], Duration::from_secs(30)).unwrap();
        assert_eq!(out.code, 0x8A15_0101);
        let out = run("cmd", &["/d", "/c", "exit -1"], Duration::from_secs(30)).unwrap();
        assert_eq!(out.code, 0xFFFF_FFFF);
    }

    #[cfg(windows)]
    #[test]
    fn run_reports_missing_program() {
        assert!(run("majestic-no-such-program", &[], Duration::from_secs(5)).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn big_outputs_on_both_pipes_do_not_deadlock() {
        let script = "for /L %i in (1,1,3000) do @(echo stdout line %i with some padding to make it longer& echo stderr line %i 1>&2)";
        let out = run("cmd", &["/d", "/c", script], Duration::from_secs(120)).unwrap();
        assert_eq!(out.code, 0);
        assert_eq!(out.stdout.lines().count(), 3000);
        assert_eq!(out.stderr.lines().count(), 3000);
    }

    #[cfg(windows)]
    #[test]
    fn timeout_kills_the_process() {
        let started = Instant::now();
        let err =
            run("cmd", &["/d", "/c", "echo before& ping -n 6 127.0.0.1 >nul"], Duration::from_millis(500)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(4));
        let timed_out = err.downcast_ref::<TimedOut>().expect("TimedOut error");
        assert_eq!(timed_out.program, "cmd");
        assert_eq!(timed_out.tail, ["before"]);
    }

    #[cfg(windows)]
    #[test]
    fn streaming_merges_and_cleans_lines() {
        let mut seen = Vec::new();
        let (code, tail) = run_streaming(
            &cmd("echo   first  & echo.& echo -& echo second 1>&2& echo 1 MB / 4 MB& echo 2 MB / 4 MB& exit 5"),
            Duration::from_secs(30),
            &mut |line| seen.push(line.to_string()),
        )
        .unwrap();
        assert_eq!(code, 5);
        seen.sort();
        assert_eq!(seen, ["1 MB / 4 MB", "2 MB / 4 MB", "first", "second"]);
        for expected in ["first", "second", "2 MB / 4 MB"] {
            assert!(tail.iter().any(|line| line == expected), "{expected} missing from {tail:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn streaming_timeout_returns_tail() {
        let err =
            run_streaming(&cmd("echo working& ping -n 6 127.0.0.1 >nul"), Duration::from_millis(500), &mut |_| {})
                .unwrap_err();
        let timed_out = err.downcast_ref::<TimedOut>().expect("TimedOut error");
        assert_eq!(timed_out.tail, ["working"]);
    }
}
