//! Developer tool: what Windows Update has waiting, as MAJestic shows it.
//! `cargo run -p majestic-core --example wupdate [-- --online] [--open]`
//! (`--online`: fresh search like "Check for updates", else Windows' cached
//! results; `--open`: then opens Settings › Windows Update and starts a search).

use majestic_core::system::{shell, windows_update};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let started = std::time::Instant::now();
    let found = windows_update::pending(args.iter().any(|a| a == "--online"));
    println!("{}", serde_json::to_string_pretty(&found).unwrap_or_default());
    eprintln!("({:.1} s)", started.elapsed().as_secs_f32());
    if args.iter().any(|a| a == "--open") {
        if let Err(e) = shell::open_settings(shell::SettingsPage::WindowsUpdateCheck) {
            eprintln!("cannot open Windows Update: {e:#}");
        }
    }
}
