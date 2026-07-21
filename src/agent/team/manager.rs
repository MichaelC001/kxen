// ---------------- TeamManager ----------------

use crate::core::event::EventBus;
use crate::core::shared::lock;
use crate::llm::ModelRef;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::inbox::{append_inbox, drain_inbox};
use super::tasks::{claim_task, complete_task, create_task};
use super::types::SpawnDeps;
use super::TeamState;

pub struct TeamManager {
    root: PathBuf,
    sessions: std::sync::Mutex<HashMap<String, Arc<TeamState>>>,
    deps: SpawnDeps,
    bus: EventBus,
}

impl TeamManager {
    pub fn new(root: PathBuf, deps: SpawnDeps, bus: EventBus) -> Arc<Self> {
        // config 是运行时状态：app 重启即清理（teams 不跨进程存活，对齐 Claude Code in-process 限制）
        let _ = std::fs::remove_dir_all(&root);
        Arc::new(Self { root, sessions: std::sync::Mutex::new(HashMap::new()), deps, bus })
    }

    pub(super) fn state_for(self: &Arc<Self>, session_id: &str) -> Arc<TeamState> {
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

    pub(super) fn persist_config(&self, state: &Arc<TeamState>) {
        let config = json!({ "session_id": state.session_id, "members": *lock(&state.members) });
        let _ = std::fs::write(state.dir.join("config.json"), serde_json::to_string_pretty(&config).unwrap_or_default());
    }
}
