//! kxen-core：域模型与共享状态（session / goal / config / 事件总线）。
//! 只依赖最底层层级，不依赖任何上层 crate。

pub mod config;
pub mod error;
pub mod event;
pub mod goal;
pub mod paths;
pub mod schedule;
pub mod session;
pub mod shared;
pub mod trust;
pub mod workspace;

pub use error::{Error, Result};
