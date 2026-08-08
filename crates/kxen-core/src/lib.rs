//! kxen core 库目标：按域分文件夹（core / llm / auth / tools / agent），
//! src-tauri 的 main.rs 只做 tauri 装配，kxen-cli 与 examples 依赖本库目标。

// crate 内文件以 `kxen_core::` 自引用，经此别名解析到自身（外部 crate 路径默认不指向 self）。
extern crate self as kxen_core;

pub mod agent;
pub mod app_state;
pub mod auth;
pub mod background_jobs;
pub mod core;
pub mod doctor;
pub mod goal_rpc;
pub mod knowledge;
pub mod llm;
pub mod lsp;
pub mod mcp;
pub(crate) mod net_response;
pub mod notify_sink;
pub mod providers;
pub mod tools;
pub mod voice;
pub mod web;
pub mod workspace_runtime;
pub mod ws;

pub use app_state::AppState;
