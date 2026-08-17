//! doctor 命令逻辑的独立验证（不经 Tauri 窗口）。

use kxen_core::auth::{ProbeOutcome, credential::read_auth_file, credential::write_auth_file, probe_all};
use kxen_core::core::paths;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user = paths::KxenPaths::user();
    let path = user.auth_file();
    let mut store = read_auth_file(&path)?;
    let outcomes = probe_all(&mut store, true);
    if let Err(e) = write_auth_file(&path, &store) {
        eprintln!("write auth.json failed: {e}");
    }

    println!("data_dir: {}", user.root().display());
    println!("config_dir: {}", user.root().display());
    println!();
    for (provider, outcome, display) in &outcomes {
        let mark = match outcome {
            ProbeOutcome::Imported => "imported",
            ProbeOutcome::Fresh => "ok     ",
            ProbeOutcome::Missing => "missing",
            ProbeOutcome::NeedsApproval => "approve",
        };
        let expired = store.get(*provider).is_some_and(|c| c.is_expired());
        println!("{mark}  {display:28} ({provider}){}", if expired { "  [expired, will refresh]" } else { "" });
    }
    Ok(())
}
