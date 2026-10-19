//! Developer tool: runs any command through `exec::run_streaming` and shows
//! the cleaned lines with the progress parsed from them.
//! `cargo run -p majestic-core --example stream -- winget download --id X -d C:\tmp`

use std::time::{Duration, Instant};

use majestic_core::exec;
use majestic_core::model::CommandSpec;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(program) = args.next() else {
        eprintln!("usage: stream <program> [args...]");
        std::process::exit(2);
    };
    let spec = CommandSpec { program, args: args.collect() };
    let started = Instant::now();
    let mut on_line = |line: &str| {
        let p = exec::parse_progress(line);
        println!("[{:6.1}s] {:<60} {:?}", started.elapsed().as_secs_f32(), line.chars().take(60).collect::<String>(), p);
    };
    match exec::run_streaming(&spec, Duration::from_secs(600), &mut on_line) {
        Ok((code, _)) => println!("== exit code {code:#x}"),
        Err(e) => println!("== error {e:#}"),
    }
}
