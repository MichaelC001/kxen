let meta = #{
    name: "comment-cleanup",
    description: "Batch-clean source comments: wipe AI deletion residue, compress verbose notes, keep Chinese WHY only",
    when_to_use: "After AI-heavy edits leave process/deletion residual comments or verbose what-comments across the repo",
    phases: [
        #{ title: "Scope", detail: "resolve non-overlapping module roots" },
        #{ title: "Clean", detail: "parallel comment cleanup per root" },
        #{ title: "Verify", detail: "comment-only diffs + residual pattern scan" },
        #{ title: "Report", detail: "aggregate residual/compress/why stats" },
    ],
};

let clean_schema = #{
    "type": "object",
    "required": ["root", "files_touched", "residual_deleted", "verbose_compressed", "why_kept", "done"],
    "properties": #{
        "root": #{ "type": "string" },
        "files_touched": #{ "type": "array", "items": #{ "type": "string" } },
        "residual_deleted": #{ "type": "integer", "minimum": 0 },
        "verbose_compressed": #{ "type": "integer", "minimum": 0 },
        "why_kept": #{
            "type": "array",
            "maxItems": 20,
            "items": #{
                "type": "object",
                "required": ["file", "summary"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "summary": #{ "type": "string" },
                },
            },
        },
        "done": #{ "type": "boolean" },
        "notes": #{ "type": "string" },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["root", "pass", "evidence"],
    "properties": #{
        "root": #{ "type": "string" },
        "pass": #{ "type": "boolean" },
        "logic_changed": #{ "type": "boolean" },
        "residual_hits": #{ "type": "integer", "minimum": 0 },
        "evidence": #{ "type": "string" },
        "failures": #{ "type": "array", "items": #{ "type": "string" } },
    },
};

let inventory_schema = #{
    "type": "object",
    "required": ["roots", "total_suspects"],
    "properties": #{
        "roots": #{
            "type": "array",
            "maxItems": 16,
            "items": #{
                "type": "object",
                "required": ["root", "suspect_count", "sample_paths"],
                "properties": #{
                    "root": #{ "type": "string" },
                    "suspect_count": #{ "type": "integer", "minimum": 0 },
                    "sample_paths": #{ "type": "array", "items": #{ "type": "string" } },
                },
            },
        },
        "total_suspects": #{ "type": "integer", "minimum": 0 },
    },
};

// --- helpers ---
fn is_allowed_root(path) {
    if type_of(path) != "string" { return false; }
    if path == "crates/kxen-core" { return true; }
    if path == "crates/kxen-cli" { return true; }
    if path == "src-tauri" { return true; }
    if path == "src" { return true; }
    if path == "website/src" { return true; }
    if path == "scripts" { return true; }
    false
}

fn default_roots() {
    [
        "crates/kxen-core",
        "crates/kxen-cli",
        "src-tauri",
        "src",
        "website/src",
        "scripts",
    ]
}

fn clean_rules() {
    let r = "";
    r += "你是代码注释清理 specialist。只改注释与由此产生的多余空行；严禁改业务逻辑、命名、类型、import、格式化策略。\n";
    r += "强制规则：\n";
    r += "1) 删除残留彻底清零：代码已删/重构/搬走则对应注释必须删除。禁止过程性/行动性注释，如「原逻辑是」「已删除因为」「迁移自」「临时保留」「Removed previous」「This was deleted because」及任何记录刚删了什么的文字。代码净值 0 → 注释净值 0。\n";
    r += "2) 可汇总的注释压成一句；优先让代码自解释。\n";
    r += "3) 保留/改写的注释必须用简体中文，且只保留真正的 WHY：非显而易见的业务约束、性能陷阱、第三方 quirk、安全边界、历史踩坑原因。禁止复述 what、解释已自明命名、空 TODO、变更历史。\n";
    r += "4) 匹配文件原有注释密度：原本几乎无注释 → 清理后仍几乎无注释。不要为了「好看」新增注释。\n";
    r += "范围：仅处理 .rs .ts .tsx .js .mjs .astro 源码；跳过 node_modules target dist .git lock 文件与生成物。\n";
    r += "必须用工具：先 rg/grep 扫描可疑注释，再 read_file 后用编辑工具改文件。禁止凭记忆改写。\n";
    r += "不要 commit、不要 push、不要改测试期望除非注释本身。\n";
    r
}

// --- Scope ---
phase("Scope");

let dry_run = false;
let roots = default_roots();
if args != () {
    if args.dry_run == true { dry_run = true; }
    if args.roots != () && type_of(args.roots) == "array" && args.roots.len() > 0 {
        let filtered = [];
        for p in args.roots {
            if is_allowed_root(p) {
                filtered.push(p);
            } else {
                log("dropped disallowed root: " + json_encode(p));
            }
        }
        if filtered.len() == 0 {
            pause("verification", "args.roots 过滤后为空。允许: crates/kxen-core, crates/kxen-cli, src-tauri, src, website/src, scripts");
        }
        roots = filtered;
    }
}

log("roots=" + json_encode(roots) + " dry_run=" + dry_run.to_string());

if dry_run {
    let inv_p = "";
    inv_p += "仓库根目录下只读盘点注释问题，不要修改文件。\n";
    inv_p += "roots=" + json_encode(roots) + "\n";
    inv_p += "对每个 root 用 rg 搜索过程/删除残留与啰嗦注释线索：原逻辑|已删除|迁移自|临时保留|Removed previous|This was deleted|TODO:|FIXME|原实现|旧实现|wip|WIP|不再使用|历史原因见|刚才|先注释掉。\n";
    inv_p += "也统计多行块注释中复述代码 what 的密度。\n";
    inv_p += "返回每个 root 的 suspect_count 与最多 8 个 sample_paths，以及 total_suspects。\n";
    let inv = agent(inv_p, #{
        label: "inventory",
        capability_mode: "read-only",
        output_schema: inventory_schema,
    });
    if inv == () || !inv.success || inv.output == () {
        pause("infra", "Inventory agent failed; resume when tools are healthy.");
    }
    let report = "";
    report += "# Comment cleanup inventory (dry_run)\n\n";
    report += "total_suspects: " + inv.output.total_suspects.to_string() + "\n\n";
    report += "```json\n" + json_encode(inv.output) + "\n```\n";
    let path = write_scratch_file("comment-cleanup-inventory.md", report);
    complete(#{
        summary: "dry_run inventory complete",
        path: path,
        total_suspects: inv.output.total_suspects,
        roots: inv.output.roots,
    });
}

// --- Clean ---
phase("Clean");

let rules = clean_rules();
let clean_jobs = [];
let ri = 0;
for root in roots {
    if ri >= 8 { break; }
    let p = "";
    p += "模块 root（仅此目录树内）: " + root + "\n";
    p += rules;
    p += "完成后返回: root, files_touched, residual_deleted(删除残留注释处数), verbose_compressed(压缩啰嗦注释处数), why_kept(最多20条保留的真正 WHY 摘要: file+summary), done=true 仅当你已扫描并处理完该 root。\n";
    p += "若该 root 无问题: files_touched=[], 计数 0, why_kept=[], done=true。空结果仅在真实扫描后才允许。\n";
    clean_jobs.push(#{
        prompt: p,
        label: "clean:" + root,
        capability_mode: "read-write",
        output_schema: clean_schema,
    });
    ri += 1;
}
if roots.len() > 8 {
    log("truncated: only first 8 roots cleaned this run");
}

let clean_results = parallel(clean_jobs);
let batch_reports = [];
let total_residual = 0;
let total_compressed = 0;
let total_files = 0;
let failed_clean = 0;
let ci = 0;
for r in clean_results {
    let root_name = roots[ci];
    ci += 1;
    if r == () || !r.success || r.output == () {
        failed_clean += 1;
        batch_reports.push(#{
            root: root_name,
            ok: false,
            residual_deleted: 0,
            verbose_compressed: 0,
            files_touched: [],
            why_kept: [],
            notes: "clean agent failed",
        });
        continue;
    }
    let out = r.output;
    // Enforce root invariant from work-list, not agent text
    let files = [];
    if out.files_touched != () {
        for f in out.files_touched {
            if type_of(f) == "string" {
                // exact root or root/…；避免 src 误匹配 src-tauri
                let under = false;
                if f == root_name {
                    under = true;
                } else if f.len() > root_name.len() {
                    if f.sub_string(0, root_name.len()) == root_name
                        && f.sub_string(root_name.len(), 1) == "/" {
                        under = true;
                    }
                }
                if under {
                    files.push(f);
                } else {
                    log("dropped out-of-scope file from " + root_name + ": " + f);
                }
            }
        }
    }
    let rd = 0;
    let vc = 0;
    if out.residual_deleted != () { rd = out.residual_deleted; }
    if out.verbose_compressed != () { vc = out.verbose_compressed; }
    total_residual += rd;
    total_compressed += vc;
    total_files += files.len();
    let why = [];
    if out.why_kept != () {
        for w in out.why_kept {
            if w != () && w.file != () && w.summary != () {
                why.push(w);
            }
        }
    }
    batch_reports.push(#{
        root: root_name,
        ok: out.done == true,
        residual_deleted: rd,
        verbose_compressed: vc,
        files_touched: files,
        why_kept: why,
        notes: if out.notes != () { out.notes } else { "" },
    });
}

log("clean done residual=" + total_residual.to_string()
    + " compressed=" + total_compressed.to_string()
    + " files~" + total_files.to_string()
    + " failed=" + failed_clean.to_string());

// --- Verify ---
phase("Verify");

let verify_jobs = [];
let vi = 0;
for root in roots {
    if vi >= 8 { break; }
    let vp = "";
    vp += "只读/执行校验，不要再改业务逻辑。root=" + root + "\n";
    vp += "1) 对该 root 下工作区改动运行 git diff（或 git status + git diff -- path）。\n";
    vp += "2) 判定 logic_changed：若 diff 改变了非注释/非空行内容（语句、符号、import、类型、字符串字面量业务值等）则为 true。\n";
    vp += "3) residual_hits：在 root 内 rg 计数过程性删除残留线索：原逻辑|已删除因为|迁移自|临时保留|Removed previous|This was deleted|刚才删|不再使用的旧|原实现如下。\n";
    vp += "4) pass=true 仅当 logic_changed=false 且 residual_hits=0 且你实际看过 diff/rg 输出。\n";
    vp += "evidence 必须粘贴简短命令输出摘要（非空）。failures 列问题。\n";
    verify_jobs.push(#{
        prompt: vp,
        label: "verify:" + root,
        capability_mode: "execute",
        output_schema: verify_schema,
    });
    vi += 1;
}

let verify_results = parallel(verify_jobs);
let verify_rows = [];
let verify_fail = 0;
let vj = 0;
for r in verify_results {
    let root_name = roots[vj];
    vj += 1;
    if r == () || !r.success || r.output == () {
        verify_fail += 1;
        verify_rows.push(#{
            root: root_name,
            pass: false,
            evidence: "verifier missing or failed",
        });
        continue;
    }
    let out = r.output;
    let ok = out.pass == true
        && out.evidence != ()
        && out.evidence != ""
        && (out.logic_changed == () || out.logic_changed == false)
        && (out.residual_hits == () || out.residual_hits == 0);
    if !ok { verify_fail += 1; }
    verify_rows.push(#{
        root: root_name,
        pass: ok,
        logic_changed: out.logic_changed,
        residual_hits: out.residual_hits,
        evidence: out.evidence,
        failures: out.failures,
    });
}

log("verify fail_count=" + verify_fail.to_string());

// --- Report ---
phase("Report");

let rep_p = "";
rep_p += "根据以下结构化批次结果，写一份简体中文 Markdown 报告。不要再改代码。\n";
rep_p += "必须包含：总体计数、每批 residual_deleted/verbose_compressed/files、保留的 WHY 摘要、校验是否通过、失败根因。\n";
rep_p += "batch_reports JSON:\n" + json_encode(batch_reports) + "\n";
rep_p += "verify_rows JSON:\n" + json_encode(verify_rows) + "\n";
rep_p += "totals: residual=" + total_residual.to_string()
    + " compressed=" + total_compressed.to_string()
    + " files=" + total_files.to_string()
    + " clean_failed=" + failed_clean.to_string()
    + " verify_failed=" + verify_fail.to_string() + "\n";
rep_p += "只输出完整 Markdown 正文。\n";

let report_agent = agent(rep_p, #{
    label: "report",
    capability_mode: "read-only",
});

let report_body = "# Comment cleanup report\n\n";
report_body += "residual_deleted=" + total_residual.to_string() + "\n";
report_body += "verbose_compressed=" + total_compressed.to_string() + "\n";
report_body += "files_touched~" + total_files.to_string() + "\n";
report_body += "verify_failed=" + verify_fail.to_string() + "\n\n";
report_body += "```json\n" + json_encode(batch_reports) + "\n```\n";
report_body += "```json\n" + json_encode(verify_rows) + "\n```\n";
if report_agent != () && report_agent.success && report_agent.output != () {
    if type_of(report_agent.output) == "string" {
        report_body = report_agent.output;
    } else {
        report_body += "\n## agent_report\n\n```json\n" + json_encode(report_agent.output) + "\n```\n";
    }
}

let report_path = write_scratch_file("comment-cleanup-report.md", report_body);

let all_pass = failed_clean == 0 && verify_fail == 0;
complete(#{
    summary: if all_pass {
        "comment cleanup complete; all batches verified"
    } else {
        "comment cleanup finished with failures; see report"
    },
    path: report_path,
    residual_deleted: total_residual,
    verbose_compressed: total_compressed,
    files_touched: total_files,
    clean_failed: failed_clean,
    verify_failed: verify_fail,
    batches: batch_reports,
    verifications: verify_rows,
    all_pass: all_pass,
});
