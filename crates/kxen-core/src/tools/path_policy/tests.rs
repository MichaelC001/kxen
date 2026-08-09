use super::*;

fn workspace(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("kxen-path-policy-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn contains_relative_absolute_missing_and_symlink_paths() {
    let work = workspace("contains");
    let inside = work.join("src/main.rs");
    std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
    std::fs::write(&inside, "fn main() {}\n").unwrap();
    assert_eq!(resolve("src/main.rs", &work, &HashSet::new()).unwrap().as_path(), inside.canonicalize().unwrap());
    assert_eq!(resolve(inside.to_str().unwrap(), &work, &HashSet::new()).unwrap().as_path(), inside.canonicalize().unwrap());
    assert!(resolve("new/deep/file.rs", &work, &HashSet::new()).unwrap().as_path().starts_with(work.canonicalize().unwrap()));

    let outside = workspace("outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, work.join("escape")).unwrap();
    assert!(resolve("escape/secret.txt", &work, &HashSet::new()).unwrap_err().contains("escapes workspace"));
    assert!(resolve("../outside/secret.txt", &work, &HashSet::new()).is_err());
    std::fs::remove_dir_all(&work).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn picker_grants_file_or_directory_but_never_credentials() {
    let work = workspace("grant-work");
    let outside = workspace("grant-outside");
    let file = outside.join("picked.txt");
    std::fs::write(&file, "ok").unwrap();
    let grants = HashSet::from([file.canonicalize().unwrap()]);
    assert!(resolve(file.to_str().unwrap(), &work, &grants).is_ok());
    assert!(resolve(outside.join("other.txt").to_str().unwrap(), &work, &grants).is_err());

    let dir_grants = HashSet::from([outside.canonicalize().unwrap()]);
    assert!(resolve(outside.join("new.txt").to_str().unwrap(), &work, &dir_grants).is_ok());
    let auth = crate::core::paths::auth_file();
    let auth_grants = HashSet::from([auth.clone()]);
    assert!(resolve(auth.to_str().unwrap(), &work, &auth_grants).unwrap_err().contains("protected"));
    std::fs::remove_dir_all(&work).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[cfg(unix)]
#[test]
fn sensitive_symlink_target_is_resolved_on_every_check() {
    let base = workspace("sensitive-link");
    let first = base.join("first");
    let second = base.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let root = base.join("protected");
    std::os::unix::fs::symlink(&first, &root).unwrap();
    let first_candidate = canonicalize_lenient(&first.join("secret")).unwrap();
    assert!(sensitive_root_matches_in(&first_candidate, std::slice::from_ref(&root)));

    std::fs::remove_file(&root).unwrap();
    std::os::unix::fs::symlink(&second, &root).unwrap();
    let second_candidate = canonicalize_lenient(&second.join("secret")).unwrap();
    assert!(sensitive_root_matches_in(&second_candidate, std::slice::from_ref(&root)));
    std::fs::remove_dir_all(base).ok();
}
