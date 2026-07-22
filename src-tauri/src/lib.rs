//! kxen 库目标：按域分文件夹（core / llm / auth / tools / agent），
//! app 的 main.rs 只做 tauri 装配，examples 依赖本库目标。

pub mod agent;
pub mod auth;
pub mod core;
pub mod knowledge;
pub mod llm;
pub mod tools;
pub mod voice;
