//! cron 注入决策：直接起 run 还是进 pending 队列。并发 run 会交叉写 JSONL 历史并互相覆盖
//! cancel token，有活跃 run / 队列非空 / 本批已分发一律入队，由 run 结束的队列续跑按序消化

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CronDispatch {
    Spawn,
    Enqueue,
}

pub(crate) fn cron_dispatch(has_active_run: bool, has_queued: bool, dispatched_this_batch: bool) -> CronDispatch {
    if has_active_run || has_queued || dispatched_this_batch { CronDispatch::Enqueue } else { CronDispatch::Spawn }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_dispatch_matrix() {
        // 无活跃 run、队列空、本批首次：唯一直接起 run 的组合
        assert_eq!(cron_dispatch(false, false, false), CronDispatch::Spawn);
        // 有活跃 run：入队，等 run 结束续跑
        assert_eq!(cron_dispatch(true, false, false), CronDispatch::Enqueue);
        // 队列非空（run 续跑窗口）：入队保持 FIFO，不抢跑
        assert_eq!(cron_dispatch(false, true, false), CronDispatch::Enqueue);
        // 同批重复 session：首个已 spawn、token 尚未注册，只能入队
        assert_eq!(cron_dispatch(false, false, true), CronDispatch::Enqueue);
        assert_eq!(cron_dispatch(true, true, true), CronDispatch::Enqueue);
    }
}
