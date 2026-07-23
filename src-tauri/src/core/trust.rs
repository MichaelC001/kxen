//! 项目信任门：未信任 workspace 的项目知识（.agents rules/notes）只索引不进全文。
//! 决定持久化在 data_dir/trusted.json；审批走 ApprovalBroker（与 exec Ask 同一通道）。

use std::path::{Path, PathBuf};

fn store_file() -> PathBuf {
    // 测试隔离：环境变量覆盖（仅 render 测试模块用 Once 设置一次，勿删）
    if let Ok(p) = std::env::var("KXEN_TRUST_FILE") {
        return PathBuf::from(p);
    }
    crate::core::paths::data_dir().join("trusted.json")
}

fn load_from(file: &Path) -> Vec<String> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn trust_into(file: &Path, workdir: &Path) {
    // 读-改-写竞态防护：并发 trust 会互相覆盖丢失条目（并行测试抓出来的真 bug）
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITE_LOCK.lock().expect("trust write lock");
    let mut list = load_from(file);
    let w = workdir.to_string_lossy().into_owned();
    if !list.contains(&w) {
        list.push(w);
        // 原子写（tmp+rename）：非原子写在并发 load_from 下会读到半截文件
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(&list).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, file);
        }
    }
}

pub fn load() -> Vec<String> {
    load_from(&store_file())
}

pub fn is_trusted(workdir: &Path) -> bool {
    let w = workdir.to_string_lossy();
    load().iter().any(|p| p == &w)
}

pub fn trust(workdir: &Path) {
    trust_into(&store_file(), workdir);
}

/// 项目内存在需要信任决策的内容（知识树或项目级 hooks 配置）。
pub fn needs_gate(workdir: &Path) -> bool {
    workdir.join(".agents").is_dir() || workdir.join(".kxen/config.toml").is_file()
}

/// workspace 切换后的信任门：未信任且含知识/项目配置 -> 后台审批（不阻塞切换）。
pub fn gate_async(workdir: &Path, broker: &std::sync::Arc<crate::agent::approval::ApprovalBroker>, bus: &crate::core::event::EventBus) {
    if !needs_gate(workdir) || is_trusted(workdir) {
        return;
    }
    let broker = broker.clone();
    let bus = bus.clone();
    let dir = workdir.to_path_buf();
    tokio::spawn(async move {
        let (id, rx) = broker.register();
        bus.publish(crate::core::event::Event::LlmDelta(serde_json::json!({
            "kind": "approval",
            "approval_id": id,
            "command": dir.display().to_string(),
            "reason": "信任此项目？（.agents 知识与项目配置将注入模型上下文）",
        })));
        if broker.wait(rx, None).await {
            trust(&dir);
            bus.publish(crate::core::event::Event::Notification(format!("已信任项目 {}", dir.display())));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kxen-trust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("trusted.json");
        assert!(load_from(&file).is_empty());
        trust_into(&file, &dir);
        assert!(load_from(&file).iter().any(|p| p == &dir.to_string_lossy()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
