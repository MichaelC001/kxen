//! kxen-core：域模型与共享状态（session / goal / config / 事件总线）。
//! 只依赖最底层层级，不依赖任何上层 crate。

pub mod artifact;
pub mod attachment;
pub mod config;
pub mod config_cache;
pub mod delivery;
pub mod durability;
pub mod error;
pub mod event;
pub mod event_store;
pub mod goal;
pub mod identity;
pub mod ids;
pub mod journal;
pub mod net_security;
pub mod notifications;
pub mod operation;
pub mod paths;
pub mod pending_queue;
pub mod recovery;
pub mod rewind_lock;
pub mod schedule;
pub mod scheduler;
pub mod session;
pub mod session_export;
pub mod session_lifecycle;
pub mod session_recovery;
pub mod shared;
pub mod trust;
pub mod usage;
pub mod usage_trend;
pub mod workspace;

pub use error::{Error, Result};
