//! 注入 QuickJS 的 JS 片段与纯函数（脚本预处理 / 结果信封）。
//! 与 engine（super::workflow）分离：350 行门禁 + 纯函数可脱离 QuickJS 单测。

use std::time::Duration;

/// 结果格式化函数：与 CONSTRAINTS 同方式注入 JS，脚本返回值统一转可读 markdown——
/// 对象/数组分节，空字符串字段显式标 [EMPTY]（JSON 空字段会被主模型静默吞掉，必须给显式信号）。
pub(crate) const FORMAT_RESULT_JS: &str = r#"
globalThis.__kxenFormatResult = (v) => {
  const EMPTY = '[EMPTY] empty result (likely a failed agent - rerun or report it)';
  const fmt = (x) => {
    if (typeof x === 'string') return x.trim() === '' ? EMPTY : x;
    return JSON.stringify(x, null, 2);
  };
  if (typeof v === 'string') return fmt(v);
  if (Array.isArray(v)) return v.map((item, i) => '## result ' + (i + 1) + '\n\n' + fmt(item)).join('\n\n');
  if (typeof v === 'object') return Object.entries(v).map(([k, x]) => '## ' + k + '\n\n' + fmt(x)).join('\n\n');
  return JSON.stringify(v, null, 2);
};
"#;

/// 深冻结工具：CONSTRAINTS 是宿主侧快照，脚本覆写必须无效（模型拿到的是只读约束视图）。
/// 注入时先 eval 本段，再把 CONSTRAINTS 字面量包进 __kxen_deepFreeze 调用。
pub(crate) const DEEP_FREEZE_JS: &str = r#"
globalThis.__kxen_deepFreeze = (o) => {
  if (o && typeof o === 'object' && !Object.isFrozen(o)) {
    for (const v of Object.values(o)) globalThis.__kxen_deepFreeze(v);
    Object.freeze(o);
  }
  return o;
};
"#;

/// 顶层无 return 的报错文案：直接告诉模型正确写法（静默 "null" 不报错不重试，模型会无声退化成逐条 exec）。
pub(crate) const NO_RETURN_MSG: &str =
    "workflow script returned nothing: top-level return is required (flat statements, do not wrap in a function)";

/// parallel 内置：worker pool 限流（缺省 8），结果顺序与入参一致。
/// 失败项收为 { __failed, error } 而不 reject 全体——一个 agent 死不能拖垮整个 workflow，
/// 用户脚本里的 safeAgent/collect 是用户态补丁，容错应内置于引擎。
pub(crate) const PARALLEL_JS: &str = r#"
globalThis.parallel = async (thunks, opts) => {
  const limit = Math.max(1, (opts && opts.concurrency) || 8);
  const results = new Array(thunks.length);
  let next = 0;
  const worker = async () => {
    while (next < thunks.length) {
      const i = next++;
      try {
        results[i] = await thunks[i]();
      } catch (e) {
        results[i] = { __failed: true, error: String((e && e.message) || e) };
      }
    }
  };
  const pool = [];
  for (let i = 0; i < Math.min(limit, thunks.length); i++) pool.push(worker());
  await Promise.all(pool);
  return results;
};
"#;

/// agent 双签名的 JS 判别层：Rust 侧只认 __kxen_agent(role, prompt, label)。
/// agent(prompt, optsObject) 是 Claude 风格脚本的可移植写法，role 取 opts.agentType || opts.role。
pub(crate) const AGENT_JS: &str = r#"
globalThis.agent = (a, b) => {
  if (b !== null && typeof b === 'object') return globalThis.__kxen_agent(b.agentType || b.role || 'execution', a, b.label);
  return globalThis.__kxen_agent(a, b, undefined);
};
"#;

/// meta 捕获闭包：必须与脚本同一作用域注入（全局函数拿不到脚本里的 const meta）。
/// 脚本跑完后 Rust 侧从 globalThis.__kxen_meta() 取走并 delete。
const META_CAPTURE_JS: &str = "globalThis.__kxen_meta = () => (typeof meta !== 'undefined' ? meta : undefined);";

/// phase 局部定义：同样必须在脚本作用域内闭包捕获 meta（index/total 按 meta.phases title 匹配）。
/// typeof 防未声明；try/catch 防 meta 在 TDZ（phase 先于 const meta 执行）时整脚本炸掉。
const PHASE_JS: &str = r#"const phase = (name) => {
  let index, total, wf;
  try {
    const m = (typeof meta !== 'undefined') ? meta : undefined;
    if (m && typeof m === 'object') {
      wf = typeof m.name === 'string' ? m.name : undefined;
      if (Array.isArray(m.phases)) {
        total = m.phases.length;
        const i = m.phases.findIndex((p) => p && p.title === name);
        if (i >= 0) index = i + 1;
      }
    }
  } catch (e) {}
  globalThis.__kxen_phase(name, index, total, wf);
};"#;

/// 仅剥行首 `export const meta` 的 `export `。
/// 不做全局剥 export：其它 export 在函数体内本就是语法错误，剥了等于把模型写错的语法静默合法化。
pub(crate) fn strip_meta_export(script: &str) -> String {
    let mut output = String::with_capacity(script.len());
    for (index, line) in script.lines().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        if line.trim_start().starts_with("export const meta") {
            let start = line.find("export const meta").expect("trimmed prefix checked");
            output.push_str(&line[..start]);
            output.push_str("const meta");
            output.push_str(&line[start + "export const meta".len()..]);
        } else {
            output.push_str(line);
        }
    }
    output
}

/// 包脚本：meta 捕获闭包 + 局部 phase 与脚本同一 async 作用域；无顶层 return 报错，非字符串走格式化。
pub(crate) fn wrap_script(script: &str) -> String {
    format!(
        "(async () => {{\n{META_CAPTURE_JS}\n{PHASE_JS}\n{script}\n}})().then(v => {{ if (v === undefined || v === null) throw new Error('{NO_RETURN_MSG}'); return __kxenFormatResult(v); }})"
    )
}

/// 完成信封：追加在脚本 return 文本之后，主模型一眼看到哪路挂了（此前失败/空结果被静默吞掉）。
/// 无失败不列 failures 段；无 meta.phases 时 phases 只报已执行数；agents 计数按 role 细分（计数降序，同数按名）。
pub(crate) fn envelope(
    wf_name: &str,
    ok_by_role: &std::collections::HashMap<String, u32>,
    failures: &[(String, String)],
    phases_done: u32,
    phases_total: Option<u32>,
    elapsed: Duration,
) -> String {
    use std::fmt::Write as _;

    let agents_total = ok_by_role.values().sum::<u32>() + failures.len() as u32;
    let mut out = format!("\n\n---\n[{wf_name}] {agents_total} agents");
    let mut roles: Vec<_> = ok_by_role.iter().collect();
    roles.sort_by(|(ra, na), (rb, nb)| nb.cmp(na).then(ra.cmp(rb)));
    let mut detail = String::new();
    for (role, count) in roles {
        if !detail.is_empty() {
            detail.push(' ');
        }
        write!(detail, "{role}:{count}").expect("writing to String cannot fail");
    }
    if !failures.is_empty() {
        if !detail.is_empty() {
            detail.push_str(", ");
        }
        write!(&mut detail, "{} failed", failures.len()).expect("writing to String cannot fail");
    }
    if !detail.is_empty() {
        write!(&mut out, " ({detail})").expect("writing to String cannot fail");
    }
    match phases_total {
        Some(total) => write!(&mut out, ", phases {phases_done}/{total}").expect("writing to String cannot fail"),
        None => write!(&mut out, ", phases {phases_done}").expect("writing to String cannot fail"),
    }
    write!(&mut out, ", {:.1}s", elapsed.as_secs_f64()).expect("writing to String cannot fail");
    if !failures.is_empty() {
        out.push_str("\nfailures: ");
        for (index, (label, error)) in failures.iter().enumerate() {
            if index > 0 {
                out.push_str("; ");
            }
            write!(out, "{label}: {error}").expect("writing to String cannot fail");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_meta_export() {
        let src = "export const meta = { name: 'x' };\nexport const other = 1;\n  export const meta2 = {};\nreturn 1";
        let out = strip_meta_export(src);
        assert!(out.starts_with("const meta = { name: 'x' };"), "{out}");
        assert!(out.contains("export const other = 1;"), "{out}");
        // 前缀匹配按 token 边界不做严格判定：meta2 这种误剥同样只是去掉 export，不改变语义
        assert!(out.contains("const meta2 = {};"), "{out}");
    }

    #[test]
    fn envelope_full_and_minimal() {
        let ok: std::collections::HashMap<String, u32> = [("research".to_string(), 3), ("execution".to_string(), 2)].into_iter().collect();
        let failures = vec![("env契约".to_string(), "boom".to_string())];
        let out = envelope("wf", &ok, &failures, 3, Some(10), Duration::from_millis(12_400));
        assert_eq!(out, "\n\n---\n[wf] 6 agents (research:3 execution:2, 1 failed), phases 3/10, 12.4s\nfailures: env契约: boom");
        let clean = envelope("workflow", &Default::default(), &[], 0, None, Duration::from_millis(500));
        assert_eq!(clean, "\n\n---\n[workflow] 0 agents, phases 0, 0.5s");
        assert!(!clean.contains("failures:"));
    }
}
