use super::*;

impl TeamManager {
    pub(super) fn restore(self: &Arc<Self>) {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let directory = entry.path();
            if directory.is_dir() {
                self.restore_dir(directory);
            }
        }
    }

    pub fn restore_session(self: &Arc<Self>, session_id: &str) {
        if crate::core::ids::validate_id(session_id).is_err() {
            return;
        }
        self.detach_session(session_id);
        self.restore_dir(self.root.join(session_id));
    }

    fn restore_dir(self: &Arc<Self>, directory: PathBuf) {
        let Some(session_id) = directory.file_name().and_then(|name| name.to_str()).map(String::from) else {
            return;
        };
        if crate::core::ids::validate_id(&session_id).is_err() {
            return;
        }
        let Ok(text) = std::fs::read_to_string(directory.join("config.json")) else {
            return;
        };
        let Ok(config) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        let mut members: Vec<super::super::types::Member> =
            config.get("members").and_then(|members| serde_json::from_value(members.clone()).ok()).unwrap_or_default();
        let restart: Vec<super::super::types::Member> = members
            .iter()
            .filter(|member| {
                !member.prompt.is_empty()
                    && !matches!(member.status, super::super::types::MemberStatus::Shutdown | super::super::types::MemberStatus::Failed)
            })
            .cloned()
            .collect();
        for member in &mut members {
            if member.status != super::super::types::MemberStatus::Shutdown && member.status != super::super::types::MemberStatus::Failed {
                member.status = if restart.iter().any(|entry| entry.name == member.name) {
                    super::super::types::MemberStatus::Idle
                } else {
                    super::super::types::MemberStatus::Shutdown
                };
            }
        }
        let tasks: Vec<super::super::types::TeamTask> = std::fs::read_to_string(directory.join("tasks.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let next_id = tasks.iter().map(|task| task.id).max().unwrap_or(0) + 1;
        let _ = std::fs::create_dir_all(directory.join("inboxes"));
        let workdir = self.session_workdir(&session_id);
        let state = Arc::new(TeamState {
            session_id,
            dir: directory,
            workdir,
            manager: Arc::downgrade(self),
            members: std::sync::Mutex::new(members),
            cancels: std::sync::Mutex::new(HashMap::new()),
            notifies: std::sync::Mutex::new(HashMap::new()),
            tasks: std::sync::Mutex::new(tasks),
            next_task_id: std::sync::atomic::AtomicU64::new(next_id),
            deps: self.deps.clone(),
            bus: self.bus.clone(),
        });
        for member in restart {
            Self::start_member_loop(&state, member.name, member.role, member.prompt, member.model, member.approved);
        }
        lock(&self.sessions).insert(state.session_id.clone(), state);
    }
}
