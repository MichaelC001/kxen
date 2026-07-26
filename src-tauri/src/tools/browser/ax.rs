//! 紧凑无障碍树渲染：纯函数（工具层与 CDP/fake 共用），ref 分配与输出截断的唯一出处。

use super::driver::RawAxNode;
use std::collections::HashMap;

/// 与 webfetch 同档的输出上限。
pub const MAX_SNAPSHOT_CHARS: usize = 50_000;

/// 可交互角色（ref 只发给这些节点；Cline/Roo 同口径）。
const INTERACTIVE_ROLES: &[&str] = &[
    "link",
    "button",
    "textbox",
    "searchbox",
    "combobox",
    "listbox",
    "checkbox",
    "radio",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "tab",
    "switch",
    "slider",
    "spinbutton",
    "treeitem",
];

/// ref -> 元素定位信息：backend 是 CDP 定位凭据，label 供 click/fill 回执回显。
#[derive(Debug, Clone)]
pub struct RefTarget {
    pub id: u32,
    pub backend: i64,
    pub label: String,
}

/// 渲染结果：text 给模型看，refs 是 ref -> backend DOM node id 的映射（click/fill 定位凭据）。
#[derive(Debug, Default)]
pub struct Snapshot {
    pub text: String,
    pub refs: Vec<RefTarget>,
}

pub fn is_interactive(role: &str) -> bool {
    INTERACTIVE_ROLES.contains(&role)
}

/// flat 遍历序节点列表 -> 缩进树文本 + ref 表。first_ref 支持同一页面多次 snapshot 单调递增编号。
pub fn render(nodes: &[RawAxNode], first_ref: u32) -> Snapshot {
    let by_id: HashMap<&str, &RawAxNode> = nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();
    let mut children: HashMap<&str, Vec<&RawAxNode>> = HashMap::new();
    let mut roots: Vec<&RawAxNode> = Vec::new();
    for n in nodes {
        match n.parent_id.as_deref().filter(|p| by_id.contains_key(p)) {
            Some(p) => children.entry(p).or_default().push(n),
            None => roots.push(n),
        }
    }

    let mut out = Snapshot::default();
    let mut next_ref = first_ref;
    let mut shown = 0usize;
    let mut total = 0usize;
    let mut truncated = false;
    // 迭代 DFS（页面嵌套深度不可信，递归有爆栈面）；ignored 子树整棵跳过
    let mut stack: Vec<(&RawAxNode, usize)> = roots.iter().rev().map(|n| (*n, 0)).collect();
    while let Some((node, depth)) = stack.pop() {
        if node.ignored {
            continue;
        }
        if let Some(line) = line_for(node, &mut next_ref, &mut out.refs) {
            total += 1;
            let indent = "  ".repeat(depth.min(24));
            if !truncated && out.text.len() + indent.len() + line.len() < MAX_SNAPSHOT_CHARS {
                out.text.push_str(&indent);
                out.text.push_str(&line);
                out.text.push('\n');
                shown += 1;
            } else {
                truncated = true;
            }
        }
        if let Some(kids) = children.get(node.node_id.as_str()) {
            for kid in kids.iter().rev() {
                stack.push((kid, depth + 1));
            }
        }
    }
    if truncated {
        out.text.push_str(&format!("...(truncated: {shown} of {total} nodes shown, cap {MAX_SNAPSHOT_CHARS} chars)\n"));
    }
    out
}

/// 单节点成行规则：可交互 -> `[ref] role "name" value="v"`；有名字的结构/文本节点 -> `role "name"`；
/// 无名容器（generic/none 等）不成行，但子树继续展开。
fn line_for(node: &RawAxNode, next_ref: &mut u32, refs: &mut Vec<RefTarget>) -> Option<String> {
    let name = escape(&node.name);
    if is_interactive(&node.role) {
        return match node.backend_dom_node_id {
            Some(backend) => {
                let r = *next_ref;
                *next_ref += 1;
                refs.push(RefTarget { id: r, backend, label: format!("{} \"{name}\"", node.role) });
                let value =
                    node.value.as_deref().filter(|v| !v.is_empty()).map(|v| format!(" value=\"{}\"", escape(v))).unwrap_or_default();
                Some(format!("[{r}] {} \"{name}\"{value}", node.role))
            }
            // 可交互角色但没有 DOM 句柄无法定位（罕见），成行但不发 ref
            None => Some(format!("{} \"{name}\" (untargetable)", node.role)),
        };
    }
    if node.role == "StaticText" {
        return (!name.is_empty()).then(|| format!("text \"{name}\""));
    }
    if !name.is_empty() && node.role != "none" && node.role != "generic" && node.role != "InlineTextBox" {
        return Some(format!("{} \"{name}\"", node.role));
    }
    None
}

/// 树是单行格式：名字里的换行/引号压平，防注入额外行伪造结构
fn escape(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").replace('"', "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>, role: &str, name: &str) -> RawAxNode {
        RawAxNode { node_id: id.into(), parent_id: parent.map(str::to_string), role: role.into(), name: name.into(), ..Default::default() }
    }

    #[test]
    fn orphan_parent_treated_as_root() {
        // parent_id 指向不在列表里的节点（partial tree）按根处理，不丢子树
        let nodes = vec![node("a", Some("ghost"), "link", "x")];
        let mut n = nodes[0].clone();
        n.backend_dom_node_id = Some(1);
        let snap = render(&[n], 1);
        assert!(snap.text.contains("[1] link \"x\""));
    }

    #[test]
    fn nameless_containers_render_nothing_but_expand() {
        let mut link = node("c", Some("b"), "link", "deep");
        link.backend_dom_node_id = Some(9);
        let nodes = vec![node("a", None, "RootWebArea", ""), node("b", Some("a"), "generic", ""), link];
        let snap = render(&nodes, 1);
        assert!(!snap.text.contains("generic"));
        assert!(snap.text.contains("    [1] link \"deep\""), "{}", snap.text);
    }

    #[test]
    fn ignored_subtree_skipped_wholesale() {
        let mut hidden = node("h", Some("r"), "button", "ghost");
        hidden.ignored = true;
        hidden.backend_dom_node_id = Some(2);
        let kid = node("k", Some("h"), "StaticText", "inside");
        let nodes = vec![node("r", None, "RootWebArea", "root"), hidden, kid];
        let snap = render(&nodes, 1);
        assert!(!snap.text.contains("ghost") && !snap.text.contains("inside"));
        assert!(snap.refs.is_empty());
    }

    #[test]
    fn newlines_and_quotes_in_names_flattened() {
        let n = node("x", None, "StaticText", "a\nb \"c\"");
        let snap = render(&[n], 1);
        assert!(snap.text.contains("text \"a b 'c'\""));
        assert_eq!(snap.text.lines().count(), 1);
    }
}
