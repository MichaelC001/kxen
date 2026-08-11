//! 循环检测，四层由廉到贵：
//! 1. exact - 同工具 + 完全相同参数连续重复
//! 2. semantic - 同工具 + 归一化参数（大小写/空白折叠）
//! 3. stagnation - 工具结果不变的连续窗口（无新信息）
//! 4. churn - ABABAB 振荡（edit/revert 循环）
//!
//! 触发即停轮并写明原因，避免继续烧预算。

use crate::core::shared::SharedStr;
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const EXACT_REPEAT: usize = 3;
const SEMANTIC_REPEAT: usize = 5;
const STAGNATION_WINDOW: usize = 8;
const CHURN_WINDOW: usize = 6;
const MAX_RECORDS: usize = 16;

#[derive(Debug)]
pub struct LoopStop {
    pub layer: &'static str,
    pub reason: String,
}

impl std::fmt::Display for LoopStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "loop detected ({}) - {}", self.layer, self.reason)
    }
}

#[derive(Debug)]
pub enum LoopVerdict {
    Ok,
    Stop(LoopStop),
}

struct CallRecord {
    name: SharedStr,
    args_sig: u64,
    semantic_sig: u64,
    result_sig: u64,
}

#[derive(Default)]
pub struct LoopDetector {
    records: VecDeque<CallRecord>,
}

impl LoopDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次工具执行；任一层触发则返回 Stop。
    pub fn record(&mut self, name: &str, arguments: &str, result: &str) -> LoopVerdict {
        if self.records.len() == MAX_RECORDS {
            self.records.pop_front();
        }
        self.records.push_back(CallRecord {
            name: SharedStr::from(name),
            args_sig: hash(arguments),
            semantic_sig: hash(&normalize(arguments)),
            result_sig: hash(result),
        });
        self.check()
    }

    fn check(&self) -> LoopVerdict {
        let last = self.records.back().expect("just pushed");

        if self.records.len() >= EXACT_REPEAT {
            let tail = self.records.iter().rev().take(EXACT_REPEAT);
            if tail.filter(|r| r.name == last.name).count() == EXACT_REPEAT
                && self.records.iter().rev().take(EXACT_REPEAT).all(|r| r.args_sig == last.args_sig)
            {
                return LoopVerdict::Stop(LoopStop {
                    layer: "exact",
                    reason: format!("`{}` called {EXACT_REPEAT} times in a row with identical arguments", last.name),
                });
            }
        }

        if self.records.len() >= SEMANTIC_REPEAT {
            let tail: Vec<&CallRecord> = self.records.iter().rev().take(SEMANTIC_REPEAT).collect();
            if tail.iter().all(|r| r.name == last.name && r.semantic_sig == last.semantic_sig) {
                return LoopVerdict::Stop(LoopStop {
                    layer: "semantic",
                    reason: format!(
                        "`{}` called {SEMANTIC_REPEAT} times in a row with equivalent arguments (only formatting/numbers differ)",
                        last.name
                    ),
                });
            }
        }

        if self.records.len() >= STAGNATION_WINDOW
            && self.records.iter().rev().take(STAGNATION_WINDOW).all(|r| r.result_sig == last.result_sig)
        {
            return LoopVerdict::Stop(LoopStop {
                layer: "stagnation",
                reason: format!("last {STAGNATION_WINDOW} tool results are identical - no new information is being produced"),
            });
        }

        if self.records.len() >= CHURN_WINDOW {
            let tail: Vec<&CallRecord> = self.records.iter().rev().take(CHURN_WINDOW).collect();
            // tail[0] 最新。ABABAB 倒序看就是偶数位同 A、奇数位同 B 且 A != B
            let (n_even, s_even) = (&tail[0].name, tail[0].args_sig);
            let (n_odd, s_odd) = (&tail[1].name, tail[1].args_sig);
            let distinct = n_even != n_odd || s_even != s_odd;
            let oscillates = distinct
                && (0..CHURN_WINDOW).all(|i| {
                    let (n, s) = if i % 2 == 0 { (n_even, s_even) } else { (n_odd, s_odd) };
                    tail[i].name.as_ref() == n.as_ref() && tail[i].args_sig == s
                });
            if oscillates {
                return LoopVerdict::Stop(LoopStop {
                    layer: "churn",
                    reason: format!("`{}` is oscillating between two argument sets (edit/revert cycle)", last.name),
                });
            }
        }

        LoopVerdict::Ok
    }
}

fn hash(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// 折叠大小写与空白，使仅表象差异的重复调用仍计为同一调用。
/// 数字故意不折叠：扫 m1.rs/m2.rs 等是合法工作；同数字无产出重试由 stagnation 层兜。
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_ws {
                out.push(' ');
            }
            last_ws = true;
        } else {
            out.push(c.to_ascii_lowercase());
            last_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop_layer(v: LoopVerdict) -> Option<&'static str> {
        match v {
            LoopVerdict::Ok => None,
            LoopVerdict::Stop(s) => Some(s.layer),
        }
    }

    #[test]
    fn exact_repeat_stops() {
        let mut d = LoopDetector::new();
        assert!(matches!(d.record("read", "{\"path\":\"a\"}", "x"), LoopVerdict::Ok));
        assert!(matches!(d.record("read", "{\"path\":\"a\"}", "x"), LoopVerdict::Ok));
        assert_eq!(stop_layer(d.record("read", "{\"path\":\"a\"}", "x")), Some("exact"));
    }

    #[test]
    fn semantic_repeat_stops_despite_formatting_changes() {
        let mut d = LoopDetector::new();
        let variants = [
            "{\"command\":\"git status\"}",
            "{\"command\":\"GIT STATUS\"}",
            "{\"command\":\"git  status\"}",
            "{\"command\":\"Git Status\"}",
        ];
        for args in variants {
            assert!(matches!(d.record("exec", args, "clean"), LoopVerdict::Ok));
        }
        assert_eq!(stop_layer(d.record("exec", "{\"command\":\"gIT stAtus\"}", "clean")), Some("semantic"));
    }

    #[test]
    fn stagnation_stops_when_results_never_change() {
        let mut d = LoopDetector::new();
        let mut layer = None;
        // 参数真正不同（避开 exact/semantic），结果恒同
        for name in ["fa", "fb", "fc", "fd", "fe", "ff", "fg", "fh"] {
            layer = stop_layer(d.record("read", &format!("{{\"path\":\"{name}.rs\"}}"), "same output"));
            if layer.is_some() {
                break;
            }
        }
        assert_eq!(layer, Some("stagnation"));
    }

    #[test]
    fn churn_stops_on_abab_oscillation() {
        let mut d = LoopDetector::new();
        let edits = ["{\"path\":\"a\",\"new_string\":\"x\"}", "{\"path\":\"a\",\"new_string\":\"y\"}"];
        let mut layer = None;
        for i in 0..CHURN_WINDOW {
            // 结果带序号避免触发 stagnation
            layer = stop_layer(d.record("edit", edits[i % 2], &format!("applied {i}")));
            if layer.is_some() {
                break;
            }
        }
        assert_eq!(layer, Some("churn"));
    }

    #[test]
    fn varied_work_does_not_stop() {
        let mut d = LoopDetector::new();
        for i in 0..12 {
            let v = d.record("read", &format!("{{\"path\":\"src/m{i}.rs\"}}"), &format!("content {i}"));
            assert!(matches!(v, LoopVerdict::Ok));
        }
    }
}
