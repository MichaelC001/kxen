//! 语言注册表：扩展名 -> (languageId, server 启动命令)。server 二进制缺失时该语言优雅降级为无 LSP。

use std::path::Path;

pub struct LanguageSpec {
    /// LSP languageId（didOpen 用）。
    pub id: &'static str,
    pub extensions: &'static [&'static str],
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// 降级文案里的安装提示。
    pub install_hint: &'static str,
}

pub const LANGUAGES: &[LanguageSpec] = &[
    LanguageSpec {
        id: "rust",
        extensions: &["rs"],
        command: "rust-analyzer",
        args: &[],
        install_hint: "rustup component add rust-analyzer",
    },
    LanguageSpec {
        id: "typescript",
        extensions: &["ts", "tsx", "mts", "cts"],
        command: "typescript-language-server",
        args: &["--stdio"],
        install_hint: "npm i -g typescript-language-server typescript",
    },
    LanguageSpec {
        id: "javascript",
        extensions: &["js", "jsx", "mjs", "cjs"],
        command: "typescript-language-server",
        args: &["--stdio"],
        install_hint: "npm i -g typescript-language-server typescript",
    },
    LanguageSpec {
        id: "python",
        extensions: &["py", "pyi"],
        command: "pyright-langserver",
        args: &["--stdio"],
        install_hint: "npm i -g pyright",
    },
    LanguageSpec {
        id: "go",
        extensions: &["go"],
        command: "gopls",
        args: &[],
        install_hint: "go install golang.org/x/tools/gopls@latest",
    },
];

/// 扩展名查语言；未注册扩展 -> None（该文件无 LSP，不视为错误）。
pub fn for_path(path: &Path) -> Option<&'static LanguageSpec> {
    let ext = path.extension()?.to_str()?;
    LANGUAGES.iter().find(|l| l.extensions.contains(&ext))
}

/// 可用性探测：`--version` 5s 超时；mise shim 存在但不可执行同样判不可用（快速失败，不进握手）。
pub async fn probe(spec: &LanguageSpec) -> bool {
    let probe = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new(spec.command)
            .arg("--version")
            .output(),
    )
    .await;
    matches!(probe, Ok(Ok(out)) if out.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_mapping() {
        let cases = [
            ("src/main.rs", "rust"),
            ("src/App.tsx", "typescript"),
            ("src/lib/util.ts", "typescript"),
            ("scripts/build.mjs", "javascript"),
            ("pkg/main.go", "go"),
            ("app/main.py", "python"),
        ];
        for (path, id) in cases {
            assert_eq!(
                for_path(Path::new(path)).map(|s| s.id),
                Some(id),
                "mapping for {path}"
            );
        }
    }

    #[test]
    fn ts_and_js_share_server_but_not_language_id() {
        let ts = for_path(Path::new("a.ts")).expect("ts");
        let js = for_path(Path::new("a.js")).expect("js");
        assert_eq!(ts.command, js.command);
        assert_ne!(ts.id, js.id);
    }

    #[test]
    fn unknown_extension_has_no_language() {
        assert!(for_path(Path::new("README.md")).is_none());
        assert!(for_path(Path::new("Makefile")).is_none());
        assert!(for_path(Path::new("Cargo.toml")).is_none());
    }

    #[test]
    fn every_language_has_stdio_command() {
        // 注册表完整性：每种语言都必须能给出 spawn 所需的命令
        for spec in LANGUAGES {
            assert!(!spec.command.is_empty(), "{} missing command", spec.id);
            assert!(
                !spec.install_hint.is_empty(),
                "{} missing install hint",
                spec.id
            );
        }
        let ids: Vec<_> = LANGUAGES.iter().map(|s| s.id).collect();
        for want in ["rust", "typescript", "javascript", "python", "go"] {
            assert!(ids.contains(&want), "registry missing {want}");
        }
    }
}
