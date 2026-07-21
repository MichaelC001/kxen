//! kxen-tools：exec / 读写删 / safety / hooks / worktree。

pub mod safety;

pub use safety::{evaluate_shell_command, guard_path, Verdict};
