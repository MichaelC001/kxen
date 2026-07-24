//! 跨 request 用量累加（P1-12）：一轮 tool loop 多次 LLM 请求，
//! 覆盖式只记末轮会漏算（状态栏 tokens 与 goal 预算入账的共同数据源）。

#[derive(Debug, Default)]
pub struct UsageAcc {
    input: u64,
    output: u64,
    /// 最近一次请求的 input（ctx 当前占用；累计值不代表窗口水位）
    last_input: u64,
    /// goal 已入账的累计值（增量入账的游标）
    charged: u64,
}

impl UsageAcc {
    pub fn push(&mut self, input: u64, output: u64) {
        self.input += input;
        self.output += output;
        self.last_input = input;
    }

    pub fn total(&self) -> (u64, u64) {
        (self.input, self.output)
    }

    pub fn last_input(&self) -> u64 {
        self.last_input
    }

    /// goal 预算入账增量：上次入账后新增的用量（无新 usage 返回 0，累计值不重复计）。
    pub fn goal_delta(&mut self) -> u64 {
        let now = self.input + self.output;
        let delta = now.saturating_sub(self.charged);
        self.charged = now;
        delta
    }
}
