//! Agent Teams：lead（主会话）+ teammates（常驻 inbox loop，可绑不同订阅模型）。
//! 存储 `data_dir/teams/<session_id>/`：config.json（members）+ tasks.json + inboxes/<name>.json。
//! 协调：tasks 依赖自动解锁（进程内 Mutex 串行 claim）；mailbox 追加写 + 读取校验；plan 审批门。

use crate::agent::agent_loop::{run_turn, AgentContext};
use crate::agent::cancel::CancelToken;
use crate::core::event::EventBus;
use crate::core::shared::lock;
use crate::llm::mrm::ModelResourceManager;
use crate::llm::{Message, ModelRef};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Notify;

// ---------------- 数据结构 ----------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Working,
    Idle,
    AwaitingPlanApproval,
    Failed,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub role: String,
    pub model: ModelRef,
    pub status: MemberStatus,
    #[serde(default)]
    pub plan_approval: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamTaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    pub id: u64,
    pub title: String,
    pub status: TeamTaskStatus,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<u64>,
}

#[derive(Debug, Deserialize)]
struct InboxEntry {
    from: String,
    text: String,
    #[serde(default)]
    #[allow(dead_code)]
    at: u64,
}

/// spawn 所需的共享依赖（构造 teammate ctx 用）。
#[derive(Clone)]
pub struct SpawnDeps {
    pub registry: Arc<crate::tools::task::TaskRegistry>,
    pub workdir: Arc<Path>,
    pub store: crate::auth::credential::AuthStore,
    pub mrm: Arc<ModelResourceManager>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    pub extras: Arc<crate::agent::agent_loop::SessionExtras>,
}

pub(crate) struct TeamState {
    session_id: String,
    dir: PathBuf,
    manager: std::sync::Weak<TeamManager>,
    members: std::sync::Mutex<Vec<Member>>,
    cancels: std::sync::Mutex<HashMap<String, CancelToken>>,
    notifies: std::sync::Mutex<HashMap<String, Arc<Notify>>>,
    tasks: std::sync::Mutex<Vec<TeamTask>>,
    next_task_id: std::sync::atomic::AtomicU64,
    deps: SpawnDeps,
    bus: EventBus,
}

pub struct TeamManager {
    root: PathBuf,
    sessions: std::sync::Mutex<HashMap<String, Arc<TeamState>>>,
    deps: SpawnDeps,
    bus: EventBus,
}

// ---------------- TeamManager ----------------

impl TeamManager {
    pub fn new(root: PathBuf, deps: SpawnDeps, bus: EventBus) -> Arc<Self> {
        // config 是运行时状态：app 重启即清理（teams 不跨进程存活，对齐 Claude Code in-process 限制）
        let _ = std::fs::remove_dir_all(&root);
        Arc::new(Self { root, sessions: std::sync::Mutex::new(HashMap::new()), deps, bus })
    }

    fn state_for(self: &Arc<Self>, session_id: &str) -> Arc<TeamState> {
        let mut map = lock(&self.sessions);
        map.entry(session_id.to_string()).or_insert_with(|| {
            let dir = self.root.join(session_id);
            let _ = std::fs::create_dir_all(dir.join("inboxes"));
            Arc::new(TeamState {
                session_id: session_id.to_string(),
                dir,
                manager: Arc::downgrade(self),
                members: std::sync::Mutex::new(Vec::new()),
                cancels: std::sync::Mutex::new(HashMap::new()),
                notifies: std::sync::Mutex::new(HashMap::new()),
                tasks: std::sync::Mutex::new(Vec::new()),
                next_task_id: std::sync::atomic::AtomicU64::new(1),
                deps: self.deps.clone(),
                bus: self.bus.clone(),
            })
        }).clone()
    }

    /// lead 工具入口。
    pub async fn lead_action(self: &Arc<Self>, session_id: &str, args: &Value) -> Result<String, String> {
        let state = self.state_for(session_id);
        match args.get("action").and_then(Value::as_str).ok_or("missing action")? {
            "spawn" => {
                let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?.to_string();
                let role = args.get("role").and_then(Value::as_str).unwrap_or("execution").to_string();
                let prompt = args.get("prompt").and_then(Value::as_str).ok_or("missing prompt")?.to_string();
                let model = args.get("model").and_then(Value::as_str).map(String::from);
                let plan_approval = args.get("plan_approval").and_then(Value::as_bool).unwrap_or(false);
                // 模型解析（显式 model > mrm 角色路由）在这层 await，spawn 本体保持 sync
                let model_ref = match model {
                    Some(m) => {
                        let (provider, model) = m.split_once('/').ok_or("model must be provider/model")?;
                        ModelRef::new(provider, model)
                    }
                    None => {
                        let resolved = state.deps.mrm.resolve(&role).await.ok_or_else(|| format!("no available model for role {role}"))?;
                        ModelRef::new(resolved.provider, resolved.model)
                    }
                };
                self.spawn(&state, name, role, prompt, model_ref, plan_approval)
            }
            "message" => {
                let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
                let text = args.get("text").and_then(Value::as_str).ok_or("missing text")?;
                self.send(&state, "lead", name, text)?;
                Ok(format!("sent to {name}"))
            }
            "approve" | "reject" => {
                let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
                let approve = args.get("action").and_then(Value::as_str) == Some("approve");
                let feedback = args.get("feedback").and_then(Value::as_str).unwrap_or("");
                self.plan_verdict(&state, name, approve, feedback)
            }
            "shutdown" => {
                let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
                self.shutdown(&state, name)
            }
            "list" => Ok(self.render_list(&state)),
            "task_create" => {
                let title = args.get("title").and_then(Value::as_str).ok_or("missing title")?;
                let depends_on: Vec<u64> = args.get("depends_on").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
                let task = create_task(&state, title, depends_on);
                Ok(format!("task #{} created: {}", task.id, task.title))
            }
            other => Err(format!("unknown team action: {other}")),
        }
    }

    fn spawn(&self, state: &Arc<TeamState>, name: String, role: String, prompt: String, model_ref: ModelRef, plan_approval: bool) -> Result<String, String> {
        if lock(&state.members).iter().any(|m| m.name == name) {
            return Err(format!("teammate already exists: {name}"));
        }
        let cancel = CancelToken::new();
        let notify = Arc::new(Notify::new());
        lock(&state.cancels).insert(name.clone(), cancel.clone());
        lock(&state.notifies).insert(name.clone(), notify.clone());
        lock(&state.members).push(Member { name: name.clone(), role: role.clone(), model: model_ref.clone(), status: MemberStatus::Working, plan_approval });
        self.persist_config(state);

        let st = state.clone();
        let (n, r, m, p, pa, c, nt) = (name, role, model_ref.clone(), prompt, plan_approval, cancel, notify);
        tokio::spawn(async move {
            teammate_loop(st, n, r, m, p, pa, c, nt).await;
        });
        Ok(format!("teammate spawned (model {})", model_ref.model))
    }

    fn plan_verdict(&self, state: &Arc<TeamState>, name: &str, approve: bool, feedback: &str) -> Result<String, String> {
        {
            let mut members = lock(&state.members);
            let Some(member) = members.iter_mut().find(|m| m.name == name) else {
                return Err(format!("teammate not found: {name}"));
            };
            if member.status != MemberStatus::AwaitingPlanApproval {
                return Err(format!("{name} is not awaiting plan approval (status: {:?})", member.status));
            }
            member.status = MemberStatus::Working;
        }
        self.persist_config(state);
        let text = if approve {
            "[lead] Plan approved. Proceed with implementation.".to_string()
        } else {
            format!("[lead] Plan rejected. Revise and resubmit. Feedback: {feedback}")
        };
        self.send(state, "lead", name, &text)?;
        Ok(if approve { format!("approved {name}") } else { format!("rejected {name} with feedback") })
    }

    fn shutdown(&self, state: &Arc<TeamState>, name: &str) -> Result<String, String> {
        let token = lock(&state.cancels).get(name).cloned();
        let Some(token) = token else {
            return Err(format!("teammate not found: {name}"));
        };
        token.cancel();
        if let Some(m) = lock(&state.members).iter_mut().find(|m| m.name == name) {
            m.status = MemberStatus::Shutdown;
        }
        self.persist_config(state);
        Ok(format!("shutdown requested: {name}"))
    }

    /// 追加 inbox + 唤醒（from 是 lead 或 teammate 名）。
    pub(crate) fn send(&self, state: &Arc<TeamState>, from: &str, to: &str, text: &str) -> Result<(), String> {
        if to == "lead" {
            // lead 的信件：bus 推给前端 + 写入 lead inbox 等下次 run 注入
            append_inbox(&state.dir, "lead", from, text)?;
            self.bus.publish(crate::core::event::Event::Notification(format!("teammate {from}: {}", text.chars().take(120).collect::<String>())));
            return Ok(());
        }
        if !lock(&state.members).iter().any(|m| m.name == to) {
            return Err(format!("teammate not found: {to}"));
        }
        append_inbox(&state.dir, to, from, text)?;
        if let Some(n) = lock(&state.notifies).get(to) {
            n.notify_one();
        }
        Ok(())
    }

    /// lead inbox 排空（run_llm 每轮注入用）。
    pub fn drain_lead_inbox(self: &Arc<Self>, session_id: &str) -> Vec<(String, String)> {
        let state = self.state_for(session_id);
        drain_inbox(&state.dir, "lead")
    }

    /// teammate 工具入口（send_message / team_task）。
    pub async fn teammate_action(self: &Arc<Self>, session_id: &str, from: &str, args: &Value) -> Result<String, String> {
        let state = self.state_for(session_id);
        match args.get("action").and_then(Value::as_str).ok_or("missing action")? {
            "send" => {
                let to = args.get("to").and_then(Value::as_str).ok_or("missing to")?;
                let text = args.get("text").and_then(Value::as_str).ok_or("missing text")?;
                self.send(&state, from, to, text)?;
                Ok(format!("sent to {to}"))
            }
            "claim" => claim_task(&state, from),
            "complete" => {
                let id = args.get("id").and_then(Value::as_u64).ok_or("missing id")?;
                complete_task(&state, from, id).await
            }
            "list" => Ok(self.render_list(&state)),
            other => Err(format!("unknown teammate action: {other}")),
        }
    }

    pub fn list_json(self: &Arc<Self>, session_id: &str) -> Value {
        let state = self.state_for(session_id);
        let members = lock(&state.members).clone();
        let tasks = lock(&state.tasks).clone();
        json!({ "members": members, "tasks": tasks })
    }

    fn render_list(&self, state: &Arc<TeamState>) -> String {
        let members = lock(&state.members);
        let tasks = lock(&state.tasks);
        let mut out = String::from("teammates:");
        for m in members.iter() {
            out.push_str(&format!("\n- {} ({}, model {}) [{:?}]", m.name, m.role, m.model.model, m.status));
        }
        if members.is_empty() {
            out.push_str(" (none)");
        }
        out.push_str("\ntasks:");
        for t in tasks.iter() {
            out.push_str(&format!(
                "\n- #{} {} [{:?}]{}{}",
                t.id,
                t.title,
                t.status,
                t.assignee.as_deref().map(|a| format!(" -> {a}")).unwrap_or_default(),
                if t.depends_on.is_empty() { String::new() } else { format!(" (deps: {:?})", t.depends_on) }
            ));
        }
        if tasks.is_empty() {
            out.push_str(" (none)");
        }
        out
    }

    fn persist_config(&self, state: &Arc<TeamState>) {
        let config = json!({ "session_id": state.session_id, "members": *lock(&state.members) });
        let _ = std::fs::write(state.dir.join("config.json"), serde_json::to_string_pretty(&config).unwrap_or_default());
    }
}

// ---------------- tasks（依赖自动解锁 + 串行 claim） ----------------

fn create_task(state: &Arc<TeamState>, title: &str, depends_on: Vec<u64>) -> TeamTask {
    let id = state.next_task_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let task = TeamTask { id, title: title.into(), status: TeamTaskStatus::Pending, assignee: None, depends_on };
    lock(&state.tasks).push(task.clone());
    persist_tasks(state);
    task
}

fn claim_task(state: &Arc<TeamState>, who: &str) -> Result<String, String> {
    let mut tasks = lock(&state.tasks);
    let done: Vec<u64> = tasks.iter().filter(|t| t.status == TeamTaskStatus::Completed).map(|t| t.id).collect();
    let Some(task) = tasks.iter_mut().find(|t| {
        t.status == TeamTaskStatus::Pending && t.assignee.is_none() && t.depends_on.iter().all(|d| done.contains(d))
    }) else {
        return Err("no claimable task (all claimed or blocked by dependencies)".into());
    };
    task.status = TeamTaskStatus::InProgress;
    task.assignee = Some(who.into());
    let title = task.title.clone();
    let id = task.id;
    drop(tasks);
    persist_tasks(state);
    Ok(format!("claimed task #{id}: {title}"))
}

async fn complete_task(state: &Arc<TeamState>, who: &str, id: u64) -> Result<String, String> {
    let title = {
        let mut tasks = lock(&state.tasks);
        let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
            return Err(format!("task not found: #{id}"));
        };
        if task.assignee.as_deref() != Some(who) {
            return Err(format!("task #{id} is not assigned to {who}"));
        }
        task.status = TeamTaskStatus::Completed;
        task.title.clone()
    };
    persist_tasks(state);
    // task_completed hook：exit 非零 = 打回（回滚 in_progress + 反馈给完成者 inbox）
    if let Some(hooks) = &state.deps.hooks {
        if let Err(feedback) = hooks.run_named("task_completed", &title, &json!({ "task_id": id, "title": title, "assignee": who })).await {
            if let Some(task) = lock(&state.tasks).iter_mut().find(|t| t.id == id) {
                task.status = TeamTaskStatus::InProgress;
            }
            persist_tasks(state);
            let _ = append_inbox(&state.dir, who, "hooks", &format!("task #{id} completion rejected: {feedback}"));
            return Err(format!("task_completed hook rejected: {feedback}"));
        }
    }
    Ok(format!("task #{id} completed"))
}

fn persist_tasks(state: &Arc<TeamState>) {
    let tasks = lock(&state.tasks).clone();
    let _ = std::fs::write(state.dir.join("tasks.json"), serde_json::to_string_pretty(&tasks).unwrap_or_default());
}

// ---------------- inbox ----------------

fn append_inbox(dir: &Path, to: &str, from: &str, text: &str) -> Result<(), String> {
    use std::io::Write;
    let path = dir.join("inboxes").join(format!("{to}.json"));
    let entry = json!({ "from": from, "text": text, "at": now_ms() });
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| e.to_string())?;
    writeln!(file, "{}", entry).map_err(|e| e.to_string())
}

/// 读 + 校验 + 清空（坏行报错剔除，valid 照常送达——对齐 Claude Code v2.1.207+ 行为）。
fn drain_inbox(dir: &Path, name: &str) -> Vec<(String, String)> {
    let path = dir.join("inboxes").join(format!("{name}.json"));
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    let mut out = Vec::new();
    for line in text.lines() {
        match serde_json::from_str::<InboxEntry>(line) {
            Ok(entry) => out.push((entry.from, entry.text)),
            Err(e) => tracing::warn!(inbox = name, error = %e, "dropping malformed inbox entry"),
        }
    }
    let _ = std::fs::write(&path, "");
    out
}

// ---------------- teammate 常驻 loop ----------------

async fn teammate_loop(
    state: Arc<TeamState>,
    name: String,
    role: String,
    model: ModelRef,
    prompt: String,
    plan_approval: bool,
    cancel: CancelToken,
    notify: Arc<Notify>,
) {
    let mut phase_prompt = prompt;
    let mut approved = !plan_approval;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        set_status(&state, &name, MemberStatus::Working);
        // 阶段 ctx：plan_approval 未批准前只读
        let allowed: Option<&'static [&'static str]> = if approved { None } else { Some(READONLY_TEAM_TOOLS) };
        let mut ctx = build_ctx(&state, &name, &role, &model, allowed, cancel.clone());
        let messages = vec![
            Message::system(teammate_system(&name, &role, approved)),
            Message::user(phase_prompt.clone()),
        ];
        let outcome = run_turn(&mut ctx, messages).await;

        if !approved {
            // 计划出炉：递交 lead 审批
            set_status(&state, &name, MemberStatus::AwaitingPlanApproval);
            let _ = append_inbox(&state.dir, "lead", &name, &format!("[plan for approval]\n{}", outcome.final_text));
            state.bus.publish(crate::core::event::Event::Notification(format!("teammate {name} submitted a plan for approval")));
        } else {
            // 本轮成果上报 lead
            if !outcome.final_text.is_empty() {
                let _ = append_inbox(&state.dir, "lead", &name, &outcome.final_text);
            }
            // teammate_idle hook：exit 非零 = 打回（反馈进 inbox， teammate 继续工作）
            if let Some(hooks) = &state.deps.hooks {
                if let Err(feedback) = hooks.run_named("teammate_idle", &name, &json!({ "agent": name, "result": outcome.final_text })).await {
                    let _ = append_inbox(&state.dir, &name, "hooks", &format!("keep working: {feedback}"));
                }
            }
            set_status(&state, &name, MemberStatus::Idle);
        }

        // idle：听 inbox 唤醒
        loop {
            notify.notified().await;
            if cancel.is_cancelled() {
                break;
            }
            let inbox = drain_inbox(&state.dir, &name);
            if inbox.is_empty() {
                continue;
            }
            // 审批结果修改 approved 状态
            for (from, text) in &inbox {
                if from == &"lead" && text.contains("Plan approved") {
                    approved = true;
                }
            }
            phase_prompt = inbox
                .iter()
                .map(|(from, text)| format!("[{from}] {text}"))
                .collect::<Vec<_>>()
                .join("\n");
            break;
        }
        if cancel.is_cancelled() {
            break;
        }
    }
    set_status(&state, &name, MemberStatus::Shutdown);
}

const READONLY_TEAM_TOOLS: &[&str] = &["read", "glob", "grep", "send_message", "team_task"];

fn build_ctx(state: &Arc<TeamState>, name: &str, _role: &str, model: &ModelRef, allowed: Option<&'static [&'static str]>, cancel: CancelToken) -> AgentContext {
    let agent_name = name.to_string();
    let session_id = state.session_id.clone();
    let session_id_event = session_id.clone();
    let bus = state.bus.clone();
    AgentContext {
        registry: state.deps.registry.clone(),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: state.deps.workdir.clone(),
        model: model.clone(),
        store: state.deps.store.clone(),
        max_turns: 16,
        mrm: Some(state.deps.mrm.clone()),
        allowed_tools: allowed,
        extras: Some(state.deps.extras.clone()),
        hooks: state.deps.hooks.clone(),
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: Some(cancel),
        team: state.manager.upgrade(),
        team_identity: Some((session_id.clone(), agent_name.clone())),
        session_id: Some(session_id),
        on_event: Arc::new(move |event| {
            let mut payload = match serde_json::to_value(&event) {
                Ok(v) => v,
                Err(_) => return,
            };
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("agent".into(), json!(agent_name));
                obj.insert("session_id".into(), json!(session_id_event));
            }
            bus.publish(crate::core::event::Event::LlmDelta(payload));
        }),
    }
}

fn teammate_system(name: &str, role: &str, approved: bool) -> String {
    let mode = if approved {
        "You may use your full tool set to implement."
    } else {
        "You are in PLAN-ONLY mode: read-only tools. Produce a concrete plan and stop - the lead must approve it before you implement anything."
    };
    format!(
        "You are teammate \"{name}\" (role: {role}) in a kxen agent team. {mode} \
        Coordinate via send_message (to: \"lead\" or a teammate name) and team_task (claim/complete/list). \
        Report results to the lead when done, then go idle."
    )
}

fn set_status(state: &Arc<TeamState>, name: &str, status: MemberStatus) {
    if let Some(m) = lock(&state.members).iter_mut().find(|m| m.name == name) {
        m.status = status;
    }
    let config = json!({ "session_id": state.session_id, "members": *lock(&state.members) });
    let _ = std::fs::write(state.dir.join("config.json"), serde_json::to_string_pretty(&config).unwrap_or_default());
    let label = match status {
        MemberStatus::Working => "working",
        MemberStatus::Idle => "idle",
        MemberStatus::AwaitingPlanApproval => "awaiting_plan_approval",
        MemberStatus::Failed => "failed",
        MemberStatus::Shutdown => "shutdown",
    };
    state.bus.publish(crate::core::event::Event::TaskUpdate { id: format!("team/{name}"), status: label });
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------- 测试（存储与任务逻辑，不触网） ----------------

#[cfg(test)]
mod tests {
    use super::*;

    fn deps() -> SpawnDeps {
        let config = crate::core::config::Config::default();
        SpawnDeps {
            registry: Arc::new(crate::tools::task::TaskRegistry::new()),
            workdir: Arc::from(Path::new("/tmp")),
            store: crate::auth::credential::AuthStore::default(),
            mrm: Arc::new(ModelResourceManager::new(config)),
            hooks: None,
            extras: Arc::new(crate::agent::agent_loop::SessionExtras::default()),
        }
    }

    fn manager(tag: &str) -> (Arc<TeamManager>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("kxen-team-{tag}-{}", std::process::id()));
        let mgr = TeamManager::new(dir.clone(), deps(), EventBus::default());
        (mgr, dir)
    }

    #[tokio::test]
    async fn task_dependency_unlocks_on_complete() {
        let (mgr, dir) = manager("deps");
        let state = mgr.state_for("s1");
        let t1 = create_task(&state, "first", vec![]);
        let _t2 = create_task(&state, "second", vec![t1.id]);
        assert!(claim_task(&state, "a").unwrap().contains("first"));
        assert!(claim_task(&state, "b").is_err(), "t2 应被依赖阻塞");
        complete_task(&state, "a", t1.id).await.unwrap();
        assert!(claim_task(&state, "b").unwrap().contains("second"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inbox_drain_validates_and_clears() {
        let (mgr, dir) = manager("inbox");
        let state = mgr.state_for("s1");
        append_inbox(&state.dir, "a", "x", "hello").unwrap();
        let path = dir.join("s1/inboxes/a.json");
        let mut content = std::fs::read_to_string(&path).unwrap();
        content.push_str("not json\n");
        std::fs::write(&path, content).unwrap();
        let drained = drain_inbox(&state.dir, "a");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].1, "hello");
        assert!(drain_inbox(&state.dir, "a").is_empty(), "drain 后应清空");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lead_inbox_via_manager() {
        let (mgr, dir) = manager("lead");
        let state = mgr.state_for("s1");
        mgr.send(&state, "worker1", "lead", "result here").unwrap();
        let drained = mgr.drain_lead_inbox("s1");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].0, "worker1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[allow(dead_code)]
fn _assert_futures_send(mgr: &Arc<TeamManager>, args: &serde_json::Value) {
    fn assert_send<T: Send>(_: T) {}
    assert_send(mgr.lead_action("s", args));
}

#[allow(dead_code)]
fn _assert_resolve_send(mrm: &crate::llm::mrm::ModelResourceManager) {
    fn assert_send<T: Send>(_: T) {}
    assert_send(mrm.resolve("thinking"));
}
