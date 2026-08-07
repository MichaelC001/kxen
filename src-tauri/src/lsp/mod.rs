//! LspManager：per-workspace 多语言 LSP，per-language 懒启动（首个该语言请求才拉起 server）。
//! server 缺失 -> 该语言降级为提示文案并缓存，不阻塞 agent，其余语言不受影响。

mod format;
mod languages;
mod process;
mod protocol;
mod store;
mod uri;

use languages::LanguageSpec;
use process::LspClient;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// diagnostics 无 path 时同步的会话文件总量上限（保护冷启动耗时与 agent context）。
const SYNC_CAP: usize = 50;

pub struct LspManager {
    root: PathBuf,
    /// per-language 状态：Running 复用 / Unavailable 缓存降级文案（命中不再 probe、不重复记日志）。
    states: tokio::sync::Mutex<HashMap<&'static str, LangState>>,
}

enum LangState {
    Running(Arc<LspClient>),
    Unavailable(String),
}

impl LspManager {
    pub fn new(root: PathBuf) -> Arc<Self> {
        Arc::new(Self { root, states: tokio::sync::Mutex::new(HashMap::new()) })
    }

    /// 诊断 root（池断言/排障用）。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 懒启动单语言：probe 先行（shim 存在但不可用时快速失败），再全握手。
    async fn ensure_client(&self, spec: &'static LanguageSpec) -> Result<Arc<LspClient>, String> {
        let mut states = self.states.lock().await;
        match states.get(spec.id) {
            Some(LangState::Running(c)) => Ok(c.clone()),
            Some(LangState::Unavailable(msg)) => Err(msg.clone()),
            None => {
                let outcome = if languages::probe(spec).await {
                    LspClient::start(&self.root, spec).await
                } else {
                    Err(format!(
                        "{} unavailable: install it ({}) for {} language-server features",
                        spec.command, spec.install_hint, spec.id
                    ))
                };
                match outcome {
                    Ok(client) => {
                        states.insert(spec.id, LangState::Running(client.clone()));
                        Ok(client)
                    }
                    Err(msg) => {
                        tracing::info!(language = spec.id, "{msg}");
                        states.insert(spec.id, LangState::Unavailable(msg.clone()));
                        Err(msg)
                    }
                }
            }
        }
    }

    /// 仅 peek：不触发启动（write/edit 后的 didChange 挂点用，未启动就跳过）。
    fn running_client(&self, language: &str) -> Option<Arc<LspClient>> {
        match self.states.try_lock() {
            Ok(guard) => match guard.get(language) {
                Some(LangState::Running(c)) => Some(c.clone()),
                _ => None,
            },
            Err(_) => None,
        }
    }

    /// 工作区切换时调用：杀掉全部语言的 server，回到未启动。
    pub async fn shutdown(&self) {
        let mut states = self.states.lock().await;
        for (_, state) in states.drain() {
            if let LangState::Running(c) = state {
                c.kill().await;
            }
        }
    }

    /// doctor 快照：只列已触发过的语言（懒启动语义：未触发 = 状态未知而非 down）。稳定排序。
    pub async fn status(&self) -> Vec<(String, String)> {
        let states = self.states.lock().await;
        let mut out: Vec<(String, String)> = states
            .iter()
            .map(|(id, st)| {
                let s = match st {
                    LangState::Running(_) => "running".to_string(),
                    LangState::Unavailable(msg) => msg.clone(),
                };
                (id.to_string(), s)
            })
            .collect();
        out.sort();
        out
    }
}

/// fs_tool write/edit 成功后的同步挂点：fire-and-forget，该语言 server 未起不动。
pub fn notify_change(lsp: Option<&Arc<LspManager>>, path: &Path) {
    let Some(lsp) = lsp else { return };
    let Some(spec) = languages::for_path(path) else { return };
    let Some(client) = lsp.running_client(spec.id) else { return };
    let path = path.to_path_buf();
    tokio::spawn(async move {
        if let Err(e) = client.sync_file(&path).await {
            tracing::warn!(error = %e, path = %path.display(), "lsp sync failed");
        }
    });
}

/// lsp 工具单一入口：action = diagnostics(默认)/hover/definition/references/symbols。
pub async fn lsp_tool(lsp: Option<&Arc<LspManager>>, args: &Value, workdir: &Path, tracked: Vec<PathBuf>) -> Result<String, String> {
    let Some(lsp) = lsp else { return Err("lsp not configured".into()) };
    let action = args.get("action").and_then(Value::as_str).unwrap_or("diagnostics");
    let path = args.get("path").and_then(Value::as_str).map(|p| workdir.join(p));
    match action {
        "diagnostics" => diagnostics_action(lsp, path.as_deref(), tracked).await,
        "hover" | "definition" | "references" | "symbols" => navigate_action(lsp, action, path, args).await,
        other => Err(format!("unknown lsp action: {other}")),
    }
}

/// path 给则同步该文件并等发布；无则按语言分组同步会话内文件，降级语言以提示文案并入输出。
async fn diagnostics_action(lsp: &Arc<LspManager>, path: Option<&Path>, tracked: Vec<PathBuf>) -> Result<String, String> {
    if let Some(path) = path {
        let Some(spec) = languages::for_path(path) else {
            return Ok(format!("no language server registered for {}", path.display()));
        };
        let client = match lsp.ensure_client(spec).await {
            Ok(c) => c,
            Err(msg) => return Ok(msg), // 降级文案走正常结果，agent 可读继续干活
        };
        client.sync_file(path).await?;
        wait_publish(&client, &[path]).await;
        return Ok(client.store.snapshot(Some(path)));
    }
    let groups = group_by_language(tracked);
    if groups.is_empty() {
        return Ok("no session-touched files with LSP support".into());
    }
    let mut sections = Vec::new();
    for (spec, files) in groups {
        match lsp.ensure_client(spec).await {
            Ok(client) => {
                for f in &files {
                    client.sync_file(f).await?;
                }
                wait_publish(&client, &files).await;
                sections.push(client.store.snapshot(None));
            }
            Err(msg) => sections.push(msg),
        }
    }
    Ok(sections.join("\n"))
}

/// hover/definition/references/symbols：均需 path；前三个需 1-based line/character。
async fn navigate_action(lsp: &Arc<LspManager>, action: &str, path: Option<PathBuf>, args: &Value) -> Result<String, String> {
    let path = path.ok_or("missing path")?;
    let Some(spec) = languages::for_path(&path) else {
        return Ok(format!("no language server registered for {}", path.display()));
    };
    let client = match lsp.ensure_client(spec).await {
        Ok(c) => c,
        Err(msg) => return Ok(msg),
    };
    client.sync_file(&path).await?;
    let position = || -> Result<(u64, u64), &'static str> {
        let line = args.get("line").and_then(Value::as_u64).ok_or("missing line")?;
        let character = args.get("character").and_then(Value::as_u64).ok_or("missing character")?;
        Ok((line, character))
    };
    match action {
        "hover" => {
            let (line, ch) = position()?;
            client.hover(&path, line, ch).await.map(|r| format::hover(&r))
        }
        "definition" => {
            let (line, ch) = position()?;
            client.definition(&path, line, ch).await.map(|r| format::locations(&r, "no definition found"))
        }
        "references" => {
            let (line, ch) = position()?;
            client.references(&path, line, ch).await.map(|r| format::locations(&r, "no references found"))
        }
        _ => client.document_symbols(&path).await.map(|r| format::symbols(&r)),
    }
}

/// publishDiagnostics 是异步通知：server 报 version 时等 published >= synced，不报的退回 has_entry 轮询，10s 上限。
async fn wait_publish<P: AsRef<Path> + Sync>(client: &LspClient, files: &[P]) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let arrived = files.iter().all(|f| match (client.store.version(f.as_ref()), client.synced_version(f.as_ref())) {
            (Some(published), Some(synced)) => published >= synced,
            _ => client.store.has_entry(f.as_ref()),
        });
        if arrived || std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// 会话内文件按语言分组（注册表扩展名过滤，总量 cap SYNC_CAP），保持首次出现顺序。
fn group_by_language(tracked: Vec<PathBuf>) -> Vec<(&'static LanguageSpec, Vec<PathBuf>)> {
    let mut order: Vec<&'static LanguageSpec> = Vec::new();
    let mut map: HashMap<&'static str, Vec<PathBuf>> = HashMap::new();
    for path in tracked.into_iter().take(SYNC_CAP) {
        let Some(spec) = languages::for_path(&path) else { continue };
        map.entry(spec.id)
            .or_insert_with(|| {
                order.push(spec);
                Vec::new()
            })
            .push(path);
    }
    order.into_iter().filter_map(|spec| map.remove(spec.id).map(|files| (spec, files))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_by_language_filters_and_groups_in_first_seen_order() {
        let tracked = vec![
            PathBuf::from("/w/a.rs"),
            PathBuf::from("/w/b.ts"),
            PathBuf::from("/w/README.md"),
            PathBuf::from("/w/c.go"),
            PathBuf::from("/w/d.rs"),
        ];
        let groups = group_by_language(tracked);
        let ids: Vec<_> = groups.iter().map(|(s, _)| s.id).collect();
        assert_eq!(ids, ["rust", "typescript", "go"]);
        assert_eq!(groups[0].1, vec![PathBuf::from("/w/a.rs"), PathBuf::from("/w/d.rs")]);
        assert_eq!(groups[1].1, vec![PathBuf::from("/w/b.ts")]);
    }

    #[test]
    fn group_by_language_caps_at_sync_cap() {
        let tracked: Vec<PathBuf> = (0..60).map(|i| PathBuf::from(format!("/w/f{i}.rs"))).collect();
        let groups = group_by_language(tracked);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), SYNC_CAP);
    }

    #[test]
    fn group_by_language_empty_when_nothing_supported() {
        assert!(group_by_language(vec![PathBuf::from("/w/a.txt"), PathBuf::from("/w/Makefile")]).is_empty());
    }

    #[tokio::test]
    async fn manager_peek_and_shutdown_without_servers() {
        let mgr = LspManager::new(PathBuf::from("/w"));
        assert_eq!(mgr.root(), Path::new("/w"));
        // 未启动任何语言时 peek 为空；shutdown 空状态安全返回
        assert!(mgr.running_client("rust").is_none());
        mgr.shutdown().await;
        assert!(mgr.running_client("rust").is_none());
    }

    #[tokio::test]
    async fn unavailable_language_caches_state() {
        // 注册表命令换成必不存在的二进制：不可启动，验证降级状态可复用
        static MISSING: LanguageSpec = LanguageSpec {
            id: "missing-lang",
            extensions: &["missingext"],
            command: "kxen-definitely-missing-lsp-server",
            args: &[],
            install_hint: "install it somehow",
        };
        let mgr = LspManager::new(PathBuf::from("/w"));
        let first = mgr.ensure_client(&MISSING).await.err().expect("must be unavailable");
        assert!(first.contains("kxen-definitely-missing-lsp-server unavailable"), "{first}");
        // 第二次命中缓存的 Unavailable：同一文案，不再 probe（无超时也即快速返回）
        let second = mgr.ensure_client(&MISSING).await.err().expect("cached unavailable");
        assert_eq!(first, second);
    }
}
