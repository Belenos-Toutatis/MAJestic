//! Developer tool: builds the list of programs from a scan, or compares a
//! saved list with this PC. No dialog, nothing installed.
//!
//! `cargo run -p majestic-core --example inventory -- [--save <file.json>] [--plan <file.json>]`

use majestic_core::inventory;
use majestic_core::settings::Settings;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let mut settings = Settings::load();
    // The list does not need online lookups.
    settings.online_intel = false;
    let items = majestic_core::scan::scan(&settings, &|_| {}).items;

    if let Some(file) = value("--plan") {
        let text = std::fs::read_to_string(&file).expect("readable file");
        let inv = inventory::parse(&text).expect("a MAJestic list");
        let plan = inventory::plan(&inv, &items);
        println!("to install ({}):", plan.to_install.len());
        for p in &plan.to_install {
            println!("  {:<10} {:<45} {}", p.source.as_str(), p.id, p.name);
        }
        println!("already here: {} packages, {} other programs", plan.installed.len(), plan.manual_installed);
        println!("to reinstall by hand ({}):", plan.manual.len());
        for m in &plan.manual {
            println!("  {} {}", m.name, m.url.as_deref().unwrap_or(""));
        }
        return;
    }

    let mut inv = inventory::build(&items);
    inventory::add_installed(
        &mut inv,
        majestic_core::pkgmgr::choco::installed(),
        majestic_core::pkgmgr::scoop::installed(),
    );
    println!("{} packages, {} other programs", inv.packages.len(), inv.manual.len());
    let mut by_source = std::collections::BTreeMap::new();
    for p in &inv.packages {
        *by_source.entry(p.source.as_str()).or_insert(0) += 1;
    }
    println!("by source: {by_source:?}");
    if let Some(file) = value("--save") {
        std::fs::write(&file, serde_json::to_vec_pretty(&inv).expect("json")).expect("writable file");
        println!("saved to {file}");
    }
}
