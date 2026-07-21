//! kxen-tools：exec / 读写删 / safety / hooks / worktree。

pub mod dev_server;
pub mod exec;
pub mod fs_tool;
pub mod hashline;
pub mod safety;
pub mod shell;
pub mod task;

pub use safety::{evaluate_shell_command, guard_path, Verdict};
