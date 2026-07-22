//! 静态快照 shell（grok-build 实证模式）：
//! 启动时捕获 login shell 的函数/alias 快照一次，每条命令在 fresh shell 回放。
//! 无跨命令状态污染、subagent 并发天然安全。trash 遮蔽（rm -> /usr/bin/trash）。

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
}

impl ShellKind {
    pub fn binary(&self) -> &'static str {
        match self {
            ShellKind::Zsh => "/bin/zsh",
            ShellKind::Bash => "/bin/bash",
            ShellKind::Fish => "/usr/local/bin/fish",
        }
    }

    pub fn rc_file(&self) -> &'static str {
        match self {
            ShellKind::Zsh => ".zshrc",
            ShellKind::Bash => ".bashrc",
            ShellKind::Fish => ".config/fish/config.fish",
        }
    }

    /// login + rc source 后捕获 alias/function 定义的命令
    fn capture_script(&self) -> String {
        match self {
            ShellKind::Zsh => "alias -L; typeset -f".into(),
            ShellKind::Bash => "alias; declare -f".into(),
            ShellKind::Fish => "alias; functions".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShellSnapshot {
    pub kind: ShellKind,
    pub snapshot: String,
}

static SNAPSHOTS: OnceLock<std::collections::HashMap<ShellKind, ShellSnapshot>> = OnceLock::new();

/// 全量快照（进程级一次）。捕获失败给空快照（rc 不存在也能跑）。
pub fn snapshots() -> &'static std::collections::HashMap<ShellKind, ShellSnapshot> {
    SNAPSHOTS.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for kind in [ShellKind::Zsh, ShellKind::Bash] {
            let output = std::process::Command::new(kind.binary())
                .args(["-lic", &kind.capture_script()])
                .output();
            let snapshot = output
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            map.insert(kind, ShellSnapshot { kind, snapshot });
        }
        map
    })
}

/// 把用户命令包装为「快照回放 + cd + rm->trash 遮蔽 + 命令」的完整 shell 调用。
pub fn wrap_command(kind: ShellKind, workdir: &str, command: &str) -> Vec<String> {
    let snapshot = snapshots().get(&kind).map(|s| s.snapshot.as_str()).unwrap_or("");
    let script = format!(
        "{snapshot}\n{shadow}\ncd -- {workdir}\n{command}",
        shadow = trash_shadow(kind),
        workdir = shell_escape(workdir),
        command = command,
    );
    vec![kind.binary().to_string(), "-c".to_string(), script]
}

/// rm -> trash 遮蔽（grok-build marker 门控模式）：过滤 rm 的 flags，文件列表进回收站。
fn trash_shadow(kind: ShellKind) -> String {
    match kind {
        ShellKind::Fish => "function rm; for a in $argv; switch $a; case '-*'; ; case '*'; command trash $a; end; end; end".into(),
        _ => "rm() { local args=(); for a in \"$@\"; do case \"$a\" in -*) ;; *) args+=(\"$a\");; esac; done; command trash \"${args[@]}\"; }".into(),
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_contains_shadow_and_cd() {
        let wrapped = wrap_command(ShellKind::Zsh, "/tmp/x", "ls -la");
        assert_eq!(wrapped[0], "/bin/zsh");
        let script = &wrapped[2];
        assert!(script.contains("command trash"), "should contain trash shadow");
        assert!(script.contains("cd -- '/tmp/x'"));
        assert!(script.ends_with("ls -la"));
    }

    #[test]
    fn escape_single_quote() {
        assert_eq!(shell_escape("a'b"), "'a'\\''b'");
    }
}
