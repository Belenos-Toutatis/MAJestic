//! Developer tool: runs a full scan and prints a summary (or the JSON with
//! `--json`). `cargo run -p majestic-core --example scan [-- --json]`

use majestic_core::model::{ItemKind, ScanProgress};
use majestic_core::settings::Settings;

fn main() {
    let json = std::env::args().any(|a| a == "--json");
    let settings = Settings::load();
    let started = std::time::Instant::now();
    let progress = |p: ScanProgress| {
        if !json {
            eprintln!("[{:6.1}s] {:?} {:?} {}", started.elapsed().as_secs_f32(), p.step, p.state,
                p.fraction.map(|f| format!("{:.0}%", f * 100.0)).unwrap_or_default());
        }
    };
    let result = majestic_core::scan::scan(&settings, &progress);
    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
        return;
    }
    println!("\n== {:?}", result.stats);
    println!("== warnings: {:?}", result.warnings);
    println!("== env: {:?}", result.environment);
    for item in result.items.iter().filter(|i| matches!(i.kind, ItemKind::Update | ItemKind::Manual)) {
        println!(
            "{:<11} {:<9} {:<45} {:>14} -> {:<14} {:?} size={:?} type={:?} running={} reasons={}",
            format!("{:?}", item.assessment.priority),
            item.source.as_str(),
            item.name.chars().take(45).collect::<String>(),
            item.installed_version.clone().unwrap_or("?".into()),
            item.available_version.clone().unwrap_or("?".into()),
            item.version_certainty,
            item.download_size,
            item.installer_type,
            item.running.len(),
            item.assessment.reasons.len(),
        );
    }
    let mut cats = std::collections::BTreeMap::new();
    for item in result.items.iter().filter(|i| i.kind == ItemKind::Unmanaged) {
        *cats.entry(format!("{:?}", item.category)).or_insert(0) += 1;
    }
    println!("== unmanaged by category: {cats:?}");
    println!("== scan took {:.1}s", started.elapsed().as_secs_f32());
}
