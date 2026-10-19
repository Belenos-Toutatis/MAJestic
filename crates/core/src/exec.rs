//! Running external tools (winget, choco, scoop, powershell) without flashing
//! a console window.
//!
//! OWNER: package-managers agent.

use std::time::Duration;

use crate::model::CommandSpec;

#[derive(Debug, Clone)]
pub struct Output {
    /// Exit code as an unsigned 32-bit value widened to i64, so winget HRESULTs
    /// read naturally (0x8A150101 = 2316632321), never negative.
    pub code: i64,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program args` to completion with `CREATE_NO_WINDOW`, stdin closed,
/// output decoded as UTF-8 (lossy). The process is killed and an error
/// returned when `timeout` elapses.
pub fn run(program: &str, args: &[&str], timeout: Duration) -> anyhow::Result<Output> {
    let _ = (program, args, timeout);
    todo!("exec::run")
}

/// Runs a long command (an installation), calling `on_line` for every
/// cleaned output line as soon as it arrives (stdout and stderr merged; lines
/// are split on `\r` *and* `\n`, trimmed, empty and pure-spinner lines dropped).
///
/// Returns the exit code (same convention as [`Output::code`]) and the last
/// 200 cleaned lines. Kills the process on timeout and returns an error.
pub fn run_streaming(
    spec: &CommandSpec,
    timeout: Duration,
    on_line: &mut dyn FnMut(&str),
) -> anyhow::Result<(i64, Vec<String>)> {
    let _ = (spec, timeout, on_line);
    todo!("exec::run_streaming")
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

/// Extracts progress from one output line, if it holds some:
/// `██████▒▒▒▒  12.5 MB / 30.0 MB` (also `12,5 Mo / 30,0 Mo`, KB/Ko, GB/Go,
/// B/o, KiB...), `45%`, `Progress: 45%`. Returns `None` otherwise.
pub fn parse_progress(line: &str) -> Option<LineProgress> {
    let _ = line;
    todo!("exec::parse_progress")
}
