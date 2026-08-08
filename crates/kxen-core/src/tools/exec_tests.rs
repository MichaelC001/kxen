use super::*;
use crate::tools::shell::default_shell;

#[tokio::test]
async fn explicit_background_with_timeout_is_watched() {
    let registry = Arc::new(TaskRegistry::new());
    let broker = Arc::new(crate::agent::approval::ApprovalBroker::new());
    let bus = crate::core::event::EventBus::new(8);
    let mut events = bus.subscribe();
    let responder = {
        let broker = broker.clone();
        tokio::spawn(async move {
            loop {
                let Ok(crate::core::event::Event::LlmDelta(payload)) = events.recv().await else {
                    continue;
                };
                if payload["kind"] == "approval" {
                    let id = payload["approval_id"].as_str().expect("approval id");
                    assert!(broker.respond(id, true));
                    return;
                }
            }
        })
    };
    let approval = ApprovalCtx::new(Some(&broker), Some(&bus), None, Some("s1"), None).expect("approval context");
    let owner = TaskOwner::new("s1", "/tmp").expect("owner");
    let params = ExecParams {
        // 看门狗行为与方言无关：CI Linux runner 无 zsh，用可用的默认 shell
        shell_type: default_shell(),
        path: std::env::temp_dir().to_string_lossy().into_owned(),
        command: "sleep 30".into(),
        timeout_ms: Some(300),
        background: true,
    };
    let ExecOutcome::Background { task_id } = exec(params, &registry, "/tmp", &owner, Some(&approval)).await.expect("exec") else {
        panic!("background: true 必须返回 Background");
    };
    responder.await.expect("approval responder");
    let task = registry.get(&owner, &task_id).expect("spawned task registered");
    let mut exited = false;
    for _ in 0..100 {
        if lock(&task.exit_code).is_some() {
            exited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(exited, "显式 background + timeout_ms 必须被看门狗终止");
    assert_eq!(task.status(), crate::tools::task::TaskStatus::Killed);
}

#[tokio::test]
async fn foreground_cancel_terminates_the_process_group() {
    let registry = Arc::new(TaskRegistry::new());
    let broker = Arc::new(crate::agent::approval::ApprovalBroker::new());
    let bus = crate::core::event::EventBus::new(8);
    let cancel = crate::agent::cancel::CancelToken::new();
    let mut events = bus.subscribe();
    let responder = {
        let broker = broker.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                let Ok(crate::core::event::Event::LlmDelta(payload)) = events.recv().await else {
                    continue;
                };
                if payload["kind"] == "approval" {
                    let id = payload["approval_id"].as_str().expect("approval id");
                    assert!(broker.respond(id, true));
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    cancel.cancel();
                    return;
                }
            }
        })
    };
    let approval = ApprovalCtx::new(Some(&broker), Some(&bus), Some(&cancel), Some("s1"), None).expect("approval context");
    let owner = TaskOwner::new("s1", "/tmp").expect("owner");
    let params = ExecParams {
        // cancel 行为与方言无关：CI Linux runner 无 zsh，用可用的默认 shell
        shell_type: default_shell(),
        path: std::env::temp_dir().to_string_lossy().into_owned(),
        command: "sleep 30".into(),
        timeout_ms: Some(30_000),
        background: false,
    };

    let error = exec(params, &registry, "/tmp", &owner, Some(&approval)).await.expect_err("cancel must stop foreground exec");
    assert!(matches!(error, ExecError::Cancelled));
    responder.await.expect("approval responder");
    let tasks = registry.list(&owner);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, crate::tools::task::TaskStatus::Killed);
}

/// 测试桩：记录是否被咨询，返回预置判定（Deny 不可绕过的行为断言依赖 consulted 标记）。
struct StubAuto {
    consulted: std::sync::atomic::AtomicBool,
    verdict: Result<(), String>,
}

impl crate::tools::auto_approve::AutoApprove for StubAuto {
    fn try_auto_allow(&self, _command: &str) -> Result<(), String> {
        self.consulted.store(true, std::sync::atomic::Ordering::Relaxed);
        self.verdict.clone()
    }
}

#[tokio::test]
async fn auto_hit_short_circuits_manual_approval() {
    let broker = crate::agent::approval::ApprovalBroker::new();
    let bus = crate::core::event::EventBus::new(8);
    let mut events = bus.subscribe();
    let auto = StubAuto { consulted: false.into(), verdict: Ok(()) };
    let approval = ApprovalCtx::new(Some(&broker), Some(&bus), None, Some("s1"), Some(&auto)).expect("approval context");
    safety_gate("echo hi", "/tmp", Some(&approval)).await.expect("auto 命中直接放行");
    assert!(auto.consulted.load(std::sync::atomic::Ordering::Relaxed));
    assert!(events.try_recv().is_err(), "auto 命中不得发起人工审批");
}

#[tokio::test]
async fn auto_miss_falls_back_to_manual_approval() {
    let broker = crate::agent::approval::ApprovalBroker::with_timeout(Duration::from_millis(30));
    let bus = crate::core::event::EventBus::new(8);
    let mut events = bus.subscribe();
    let auto = StubAuto { consulted: false.into(), verdict: Err("no allowlist match".into()) };
    let approval = ApprovalCtx::new(Some(&broker), Some(&bus), None, Some("s1"), Some(&auto)).expect("approval context");
    let error = safety_gate("echo hi", "/tmp", Some(&approval)).await.unwrap_err();
    assert!(matches!(error, ExecError::Safety { .. }), "auto 未命中回落人工审批，超时按拒绝");
    assert!(matches!(events.try_recv(), Ok(crate::core::event::Event::LlmDelta(ref p)) if p["kind"] == "approval"), "回落必须发起人工审批");
}

#[tokio::test]
async fn deny_verdict_never_reaches_auto_approve() {
    let broker = crate::agent::approval::ApprovalBroker::new();
    let bus = crate::core::event::EventBus::new(8);
    let auto = StubAuto { consulted: false.into(), verdict: Ok(()) };
    let approval = ApprovalCtx::new(Some(&broker), Some(&bus), None, Some("s1"), Some(&auto)).expect("approval context");
    // rm 是 Safety Deny 档（F5 不可恢复删除）：Deny 在 auto 检查之前返回，物理上不可绕过
    let error = safety_gate("rm -rf junk", "/tmp", Some(&approval)).await.unwrap_err();
    assert!(matches!(error, ExecError::Safety { .. }));
    assert!(!auto.consulted.load(std::sync::atomic::Ordering::Relaxed), "Deny 不得咨询 auto 句柄");
}
