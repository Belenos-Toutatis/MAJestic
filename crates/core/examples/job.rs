//! Developer tool: runs one job in this process and prints its events.
//! `cargo run -p majestic-core --example job -- <source> <id> [version] [--from V] [--to V]`
//! (`version` → install that version, else upgrade; `--from`/`--to` fill the
//! versions the UI would send, which enables the post-install version check).

use majestic_core::model::{JobAction, JobRequest, Source};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str| {
        let i = args.iter().position(|a| a == name)?;
        let value = args.get(i + 1).cloned();
        args.drain(i..(i + 2).min(args.len()));
        value
    };
    let (from_version, to_version) = (flag("--from"), flag("--to"));
    let (Some(source), Some(id)) = (args.first(), args.get(1)) else {
        eprintln!("usage: job <winget|msstore|chocolatey|scoop> <id> [version]");
        std::process::exit(2);
    };
    let source = match source.as_str() {
        "winget" => Source::Winget,
        "msstore" => Source::MsStore,
        "chocolatey" => Source::Chocolatey,
        "scoop" => Source::Scoop,
        other => {
            eprintln!("unknown source {other}");
            std::process::exit(2);
        }
    };
    let action = match args.get(2) {
        Some(v) => JobAction::InstallVersion { version: v.clone() },
        None => JobAction::Upgrade,
    };
    let job = JobRequest {
        key: format!("{}:{id}", source.as_str()),
        source,
        id: id.clone(),
        name: id.clone(),
        action,
        from_version,
        to_version,
        scope: None,
        include_unknown: false,
        include_pinned: false,
        download_size: None,
        installer_type: None,
    };
    let started = std::time::Instant::now();
    let mut emit = |ev: majestic_core::model::JobEvent| {
        println!(
            "[{:6.1}s] {:?} progress={:?} bytes={:?}/{:?} msg={:?}",
            started.elapsed().as_secs_f32(),
            ev.state,
            ev.progress,
            ev.downloaded,
            ev.total_bytes,
            ev.message
        );
    };
    let state = majestic_core::jobs::run_job(&job, &mut emit);
    println!("== final: {state:?} after {:.1}s", started.elapsed().as_secs_f32());
}
