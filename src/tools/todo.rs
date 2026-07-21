//! todo 工具：会话级任务清单（deferred 工具，经 tool_search 挂载）。

use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TodoItem {
    pub id: u32,
    pub content: String,
    pub done: bool,
}

#[derive(Default)]
pub struct TodoStore {
    items: Mutex<Vec<TodoItem>>,
    next_id: Mutex<u32>,
}

impl TodoStore {
    pub fn add(&self, content: String) -> TodoItem {
        let mut next = self.next_id.lock().expect("todo id");
        *next += 1;
        let item = TodoItem { id: *next, content, done: false };
        self.items.lock().expect("todo").push(item.clone());
        item
    }

    pub fn complete(&self, id: u32) -> bool {
        let mut items = self.items.lock().expect("todo");
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.done = true;
            true
        } else {
            false
        }
    }

    pub fn clear_done(&self) -> usize {
        let mut items = self.items.lock().expect("todo");
        let before = items.len();
        items.retain(|i| !i.done);
        before - items.len()
    }

    pub fn list(&self) -> Vec<TodoItem> {
        self.items.lock().expect("todo").clone()
    }

    pub fn render(&self) -> String {
        let items = self.list();
        if items.is_empty() {
            return "todo list is empty".into();
        }
        items
            .iter()
            .map(|i| format!("{} #{} {}", if i.done { "[x]" } else { "[ ]" }, i.id, i.content))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle() {
        let store = TodoStore::default();
        let a = store.add("task A".into());
        store.add("task B".into());
        assert!(store.complete(a.id));
        assert!(!store.complete(999));
        assert_eq!(store.clear_done(), 1);
        let items = store.list();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "task B");
    }
}
