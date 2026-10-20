//! Developer tool: runs MAJestic's self-update on any copy of the executable,
//! without the window: check GitHub, download next to `<exe>`, verify the
//! SHA-256 and the version, swap.
//! `cargo run -p majestic-core --example selfupdate -- <path\to\MAJestic.exe> <pretended current version>`

use majestic_core::selfupdate;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(exe), Some(current)) = (args.first(), args.get(1)) else {
        eprintln!("usage: selfupdate <path to MAJestic.exe> <current version>");
        std::process::exit(2);
    };
    let exe = std::path::Path::new(exe);
    let release = match selfupdate::check(current) {
        Ok(Some(r)) => r,
        Ok(None) => {
            println!("no version newer than {current}");
            return;
        }
        Err(e) => {
            eprintln!("check failed: {e:#}");
            std::process::exit(1);
        }
    };
    println!("found {} ({:?} bytes), notes:\n{}\n", release.version, release.size, release.notes);
    let staging = selfupdate::staging_path(exe);
    let mut last = 0;
    let result = selfupdate::download(&release, &staging, &mut |done, total| {
        let pct = total.map(|t| done * 100 / t.max(1)).unwrap_or(0);
        if pct >= last + 20 || Some(done) == total {
            last = pct;
            println!("downloaded {done} / {total:?} ({pct} %)");
        }
    });
    if let Err(e) = result {
        eprintln!("download refused: {e}");
        std::process::exit(1);
    }
    println!("checked: SHA-256 and version match");
    match selfupdate::swap(exe, &staging) {
        Ok(()) => println!("swapped: {} is now {}", exe.display(), release.version),
        Err(e) => {
            eprintln!("swap failed: {e}");
            std::process::exit(1);
        }
    }
}
