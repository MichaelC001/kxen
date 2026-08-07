//! kxen 库目标：按域分文件夹（core / llm / auth / tools / agent），
//! app 的 main.rs 只做 tauri 装配，examples 依赖本库目标。

// 原 bin 私有模块（app_state / background_jobs / doctor / goal_rpc / ws）下沉进 lib 后，
// 文件内既有 `kxen_gui::` 引用经此别名在 lib crate 内继续解析，避免逐文件改写路径。
extern crate self as kxen_gui;

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
pub mod providers;
pub mod tools;
pub mod voice;
pub mod web;
pub mod workspace_runtime;
pub mod ws;

pub use app_state::AppState;
