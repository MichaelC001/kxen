//! kxen-auth：订阅凭证探测（官方 CLI 凭证存储 -> 新鲜度比较 -> 导入）。
//! external 凭证文件只读不动（no move/rewrite/permission 变更）。

pub mod credential;
pub mod probe;
pub mod refresh;

pub use credential::{Credential, CredentialKind};
pub use probe::{ProbeOutcome, ProbeRule, probe_all};
