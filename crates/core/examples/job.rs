//! Developer tool: runs one job in this process and prints its events.
//! `cargo run -p majestic-core --example job -- <source> <id> [version]`
//! (`version` → install that version, else upgrade).

use majestic_core::model::{JobAction, JobRequest, Source};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
        from_version: None,
        to_version: None,
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
