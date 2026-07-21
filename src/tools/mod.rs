//! kxen-tools：exec / 读写删 / safety / hooks / worktree。

pub mod dev_server;
pub mod exec;
pub mod fs_tool;
pub mod hashline;
pub mod hooks;
pub mod safety;
pub mod search;
pub mod shell;
pub mod task;
pub mod todo;
pub mod webfetch;

pub use safety::{evaluate_shell_command, guard_path, Verdict};
