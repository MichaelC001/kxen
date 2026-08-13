use std::collections::{BTreeMap, HashMap};
use std::ffi::{OsStr, OsString};

const INHERITED_ENV_ALLOWLIST: &[&str] = &["HOME", "LANG", "LC_ALL", "LC_CTYPE", "LOGNAME", "PATH", "SHELL", "TMPDIR", "USER"];

pub(super) fn child_environment<I>(
    inherited: I,
    configured: &HashMap<String, String>,
    closed_base: Option<&BTreeMap<OsString, OsString>>,
) -> HashMap<OsString, OsString>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let mut environment: HashMap<_, _> = match closed_base {
        Some(base) => base.iter().map(|(key, value)| (key.clone(), value.clone())).collect(),
        None => inherited.into_iter().filter(|(key, _)| key.to_str().is_some_and(|key| INHERITED_ENV_ALLOWLIST.contains(&key))).collect(),
    };
    environment.extend(configured.iter().map(|(key, value)| (key.into(), value.into())));
    // Headless DCP execution owns these isolation roots. An MCP config may add
    // explicit service credentials, but it cannot redirect the process back to
    // the host user's credential/configuration directories.
    if let Some(base) = closed_base {
        for key in ["HOME", "USERPROFILE", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
            if let Some(value) = base.get(OsStr::new(key)) {
                environment.insert(key.into(), value.clone());
            }
        }
    }
    environment
}
