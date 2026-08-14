use crate::knowledge::{Entry, Scope, slugify};
use std::path::{Path, PathBuf};

pub(super) fn find_entry(scope: Scope, workdir: &Path, identifier: &str) -> Result<Entry, String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/var/empty"));
    find_entry_with_home(scope, workdir, &home, identifier)
}

pub(super) fn find_entry_with_home(scope: Scope, workdir: &Path, home: &Path, identifier: &str) -> Result<Entry, String> {
    let entries = crate::knowledge::scan::scan_all_with_home(workdir, home);
    if let Some(entry) = entries.iter().find(|entry| entry.scope == scope && entry.concept_id == identifier) {
        return Ok(entry.clone());
    }
    // slug 只保留旧 RPC 和手输兼容；任意目录出现同名时必须使用 concept_id。
    let normalized = slugify(identifier);
    let matches: Vec<&Entry> =
        entries.iter().filter(|entry| entry.scope == scope && (entry.slug == identifier || entry.slug == normalized)).collect();
    match matches.as_slice() {
        [entry] => Ok((*entry).clone()),
        [] => Err(format!("not found: {}/{identifier}", scope.as_str())),
        _ => Err(format!("ambiguous knowledge slug: {}/{identifier}; use concept_id", scope.as_str())),
    }
}
