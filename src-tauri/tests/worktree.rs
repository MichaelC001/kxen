// worktree 生命周期与审批门测试（从 tools/worktree.rs 拆出，350 行门禁）。
use kxen_app::agent::approval::ApprovalBroker;
use kxen_app::core::event::{Event, EventBus};
use kxen_app::tools::exec::ApprovalCtx;
use kxen_app::tools::worktree::{create, diff_stat, list, remove, remove_with_approval, validate_name};
use std::path::{Path, PathBuf};

/// 建临时 git 仓库（tag 区分并行测试，避免同 pid 撞目录；先清上次失败的残留）
fn init_repo(tag: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!("kxen-wt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git").args(args).current_dir(&repo).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "hello").unwrap();
    run(&["add", "."]);
    run(&["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-m", "init"]);
    repo
}

/// 同步跑 git 取 stdout（断言分支是否还存在）
fn git_out(repo: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").args(args).current_dir(repo).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 挂总线等审批事件并按 allow 应答（join! 同任务轮询，避开 spawn 的 'static 约束）
async fn respond_via_bus<T>(broker: &ApprovalBroker, bus: &EventBus, allow: bool, fut: impl std::future::Future<Output = T>) -> T {
    let mut rx = bus.subscribe();
    let responder = async {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(Event::LlmDelta(v)) = rx.recv().await {
                    if v.get("kind").and_then(|k| k.as_str()) == Some("approval") {
                        let id = v.get("approval_id").and_then(|i| i.as_str()).unwrap_or_default().to_string();
                        assert!(broker.respond(&id, allow), "approval should be pending");
                        return;
                    }
                }
            }
        })
        .await
        .expect("approval event not published");
    };
    let (out, ()) = tokio::join!(fut, responder);
    out
}

/// 用临时 git 仓库真实跑 create/list/remove。
#[tokio::test]
async fn lifecycle() {
    let repo = init_repo("lc");

    let info = create(&repo, "wt1").await.unwrap();
    assert!(info.path.join("a.txt").exists());
    assert_eq!(info.branch, "kxen/wt1");
    // .gitignore 幂等
    let gi = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert_eq!(gi.matches(".kxen/").count(), 1);

    let trees = list(&repo).await.unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0].name, "wt1");

    // 删分支必须过审批：用户放行后 worktree 与分支一起清掉
    let broker = ApprovalBroker::new();
    let bus = EventBus::default();
    let ctx = ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "t" };
    respond_via_bus(&broker, &bus, true, remove_with_approval(&repo, "wt1", true, Some(&ctx))).await.unwrap();
    assert!(list(&repo).await.unwrap().is_empty());

    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn name_validation() {
    assert!(validate_name("wt-1_a").is_ok());
    for bad in ["", "..", "../x", "a/b", "a\\b", "a b", "a.b", "a;b"] {
        assert!(validate_name(bad).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn remove_and_diff_reject_bad_names() {
    let repo = init_repo("bad");
    for bad in ["../x", "a/b", ".."] {
        assert!(remove(&repo, bad, false).await.unwrap_err().contains("invalid worktree name"), "{bad}");
        assert!(diff_stat(&repo, bad).await.unwrap_err().contains("invalid worktree name"), "{bad}");
    }
    std::fs::remove_dir_all(&repo).ok();
}

/// clean 且保留分支：无数据可丢，无审批通道的旧入口直接放行（分支仍在）
#[tokio::test]
async fn clean_remove_without_channel_ok() {
    let repo = init_repo("clean");
    create(&repo, "c1").await.unwrap();
    remove(&repo, "c1", false).await.unwrap();
    assert!(!repo.join(".kxen/worktrees/c1").exists());
    assert!(git_out(&repo, &["branch", "--list", "kxen/c1"]).contains("kxen/c1"));
    std::fs::remove_dir_all(&repo).ok();
}

/// delete_branch 无通道按拒绝，审批放行后才真删
#[tokio::test]
async fn delete_branch_requires_approval() {
    let repo = init_repo("delbr");
    create(&repo, "b1").await.unwrap();
    let err = remove(&repo, "b1", true).await.unwrap_err();
    assert!(err.contains("审批通道"), "{err}");
    assert!(repo.join(".kxen/worktrees/b1").exists());
    assert!(git_out(&repo, &["branch", "--list", "kxen/b1"]).contains("kxen/b1"));

    let broker = ApprovalBroker::new();
    let bus = EventBus::default();
    let ctx = ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "t" };
    respond_via_bus(&broker, &bus, true, remove_with_approval(&repo, "b1", true, Some(&ctx))).await.unwrap();
    assert!(!repo.join(".kxen/worktrees/b1").exists());
    assert!(git_out(&repo, &["branch", "--list", "kxen/b1"]).trim().is_empty());
    std::fs::remove_dir_all(&repo).ok();
}

/// dirty（未跟踪文件也算）：无通道拒绝、用户拒绝都保留现场，用户放行才删
#[tokio::test]
async fn dirty_remove_guarded_by_approval() {
    let repo = init_repo("dirty");
    create(&repo, "d1").await.unwrap();
    let wt = repo.join(".kxen/worktrees/d1");
    std::fs::write(wt.join("dirty.txt"), "x").unwrap();

    let err = remove(&repo, "d1", false).await.unwrap_err();
    assert!(err.contains("审批通道"), "{err}");
    assert!(wt.join("dirty.txt").exists());

    let broker = ApprovalBroker::new();
    let bus = EventBus::default();
    let ctx = ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "t" };
    let err = respond_via_bus(&broker, &bus, false, remove_with_approval(&repo, "d1", false, Some(&ctx))).await.unwrap_err();
    assert!(err.contains("用户拒绝"), "{err}");
    assert!(wt.join("dirty.txt").exists());

    respond_via_bus(&broker, &bus, true, remove_with_approval(&repo, "d1", false, Some(&ctx))).await.unwrap();
    assert!(!wt.exists());
    std::fs::remove_dir_all(&repo).ok();
}
