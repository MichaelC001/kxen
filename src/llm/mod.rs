//! kxen-llm：provider 调用层（自研薄层：endpoint + auth + SSE）。
//! 每 provider ~200-400 行；openai-compatible 一条通用实现覆盖长尾。

pub mod anthropic;
pub mod anthropic_sse;
pub mod client;
pub mod mrm;
pub mod openai;
pub mod sse;
pub mod tool;
pub mod types;
pub mod xai;

pub use client::LlmClient;
pub use types::{Delta, Message, ModelRef, Role};
