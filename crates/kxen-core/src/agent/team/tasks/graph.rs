use std::collections::{HashMap, HashSet};

use super::{TeamTask, TeamTaskStatus};

pub(super) fn validate_new_task(tasks: &[TeamTask], task: &TeamTask) -> Result<(), String> {
    if tasks.iter().any(|existing| existing.id == task.id) {
        return Err(format!("duplicate task id: #{}", task.id));
    }
    let mut dependencies = HashSet::with_capacity(task.depends_on.len());
    for dependency in &task.depends_on {
        if *dependency == task.id {
            return Err(format!("task #{} cannot depend on itself", task.id));
        }
        if !dependencies.insert(*dependency) {
            return Err(format!("task #{} has duplicate dependency #{}", task.id, dependency));
        }
        if !tasks.iter().any(|existing| existing.id == *dependency) {
            return Err(format!("task #{} depends on unknown task #{}", task.id, dependency));
        }
    }
    // 新 id 尚未被任何已有任务引用，因此只从已有任务向后依赖，不可能成环。
    Ok(())
}

pub(in crate::agent::team) fn validate_task_graph(tasks: &[TeamTask]) -> Result<(), String> {
    let mut by_id = HashMap::with_capacity(tasks.len());
    for task in tasks {
        if by_id.insert(task.id, task).is_some() {
            return Err(format!("duplicate task id: #{}", task.id));
        }
        if let Some(assignee) = &task.assignee {
            crate::core::ids::validate_id(assignee).map_err(|error| format!("task #{} assignee: {error}", task.id))?;
        }
        if let Some(attempt_id) = &task.attempt_id {
            crate::core::ids::validate_id(attempt_id).map_err(|error| format!("task #{} completion attempt: {error}", task.id))?;
        }
        if task.status == TeamTaskStatus::InProgress && task.assignee.is_none() {
            return Err(format!("in-progress task #{} has no assignee", task.id));
        }
        if task.status == TeamTaskStatus::Completing && (task.assignee.is_none() || task.attempt_id.is_none()) {
            return Err(format!("completing task #{} has no assignee or completion attempt", task.id));
        }
        let mut dependencies = HashSet::with_capacity(task.depends_on.len());
        for dependency in &task.depends_on {
            if *dependency == task.id {
                return Err(format!("task #{} cannot depend on itself", task.id));
            }
            if !dependencies.insert(*dependency) {
                return Err(format!("task #{} has duplicate dependency #{}", task.id, dependency));
            }
        }
    }
    for task in tasks {
        for dependency in &task.depends_on {
            if !by_id.contains_key(dependency) {
                return Err(format!("task #{} depends on unknown task #{}", task.id, dependency));
            }
        }
    }

    fn visit(id: u64, by_id: &HashMap<u64, &TeamTask>, colors: &mut HashMap<u64, u8>) -> Result<(), String> {
        match colors.get(&id).copied().unwrap_or(0) {
            1 => return Err(format!("task dependency cycle includes #{id}")),
            2 => return Ok(()),
            _ => {}
        }
        colors.insert(id, 1);
        for dependency in &by_id[&id].depends_on {
            visit(*dependency, by_id, colors)?;
        }
        colors.insert(id, 2);
        Ok(())
    }

    let mut colors = HashMap::with_capacity(tasks.len());
    for id in by_id.keys().copied() {
        visit(id, &by_id, &mut colors)?;
    }
    Ok(())
}
