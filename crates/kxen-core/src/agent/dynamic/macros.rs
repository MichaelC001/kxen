//! DCP 动态工具宏目录：`<policy 同级>/dynamic-tools/`。
//! tool_define 在 DCP run 内只产出提案（proposals/ 落盘留痕），审批通过后激活为
//! `<name>.json`；当前 run 不生效，新 session 由 runner 加载进 SessionExtras 注册表生效。
//! 加载即校验（name/实现 hash 自洽），任何文件坏掉整个目录 fail-closed。

use std::path::{Path, PathBuf};

use super::{DynamicToolDef, validate_def};

/// 宏目录位置：policy 文件同级；无 policy 文件则动态工具无从定位（runner fail-closed）。
pub fn macro_dir_for(policy_file: Option<&Path>) -> Option<PathBuf> {
    policy_file.and_then(Path::parent).map(|dir| dir.join("dynamic-tools"))
}

/// 提案落盘（审批前）：proposals/<name>.json，审批不过也留痕供人工审查。
pub fn propose(dir: &Path, def: &DynamicToolDef) -> Result<PathBuf, String> {
    write_def(&dir.join("proposals"), def)
}

/// 审批通过后激活：宏目录顶层 <name>.json，新 session 锁解析/运行准备时加载。
pub fn activate(dir: &Path, def: &DynamicToolDef) -> Result<PathBuf, String> {
    write_def(dir, def)
}

fn write_def(dir: &Path, def: &DynamicToolDef) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("create dynamic tool directory {}: {error}", dir.display()))?;
    let path = dir.join(format!("{}.json", def.name));
    let bytes = serde_json::to_vec_pretty(def).map_err(|error| error.to_string())?;
    crate::core::durability::atomic_replace(&path, &bytes)
        .map_err(|error| format!("write dynamic tool macro {}: {error}", path.display()))?;
    Ok(path)
}

/// 加载宏目录顶层全部定义：目录不存在 = 空集；任一文件解析/校验失败整个目录不可用。
pub fn load_active(dir: &Path) -> Result<Vec<DynamicToolDef>, String> {
    let mut defs = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(defs),
        Err(error) => return Err(format!("read dynamic tool macro directory {}: {error}", dir.display())),
    };
    for entry in entries {
        let path = entry.map_err(|error| format!("list dynamic tool macro directory {}: {error}", dir.display()))?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|error| format!("read dynamic tool macro {}: {error}", path.display()))?;
        let def: DynamicToolDef =
            serde_json::from_slice(&bytes).map_err(|error| format!("parse dynamic tool macro {}: {error}", path.display()))?;
        validate_def(&def).map_err(|error| format!("invalid dynamic tool macro {}: {error}", path.display()))?;
        defs.push(def);
    }
    defs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(defs)
}

/// 宏目录 -> 会话注册表：加载（含 hash 校验）+ 注册 + 标记提案模式目录（tool_define 据此走 DCP 路径）。
pub fn load_into_extras(dir: &Path, extras: &crate::agent::agent_loop::SessionExtras) -> Result<usize, String> {
    let defs = load_active(dir)?;
    let count = defs.len();
    {
        let mut registry = crate::core::shared::lock(&extras.dynamic_tools);
        for def in defs {
            registry.insert(def.name.clone(), def);
        }
    }
    *crate::core::shared::lock(&extras.dynamic_macro_dir) = Some(dir.to_path_buf());
    Ok(count)
}
