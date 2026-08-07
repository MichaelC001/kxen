use super::*;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn dummy_config() -> crate::core::config::VoiceConfig {
    crate::core::config::VoiceConfig { engine: "dummy".into(), ..Default::default() }
}

fn dummy_slot(session_id: &str) -> Option<u32> {
    match crate::core::shared::lock(&ACTIVE).get(session_id) {
        Some(Active::Dummy(n)) => Some(*n),
        _ => None,
    }
}

fn reporter() -> crate::agent::agent_loop::UsageReporter {
    crate::agent::agent_loop::UsageReporter::new_unscoped(
        "system_voice_test",
        std::sync::Arc::default(),
        crate::core::event::EventBus::default(),
    )
}

#[tokio::test]
async fn session_slots_are_independent() {
    let _test = TEST_LOCK.lock().await;
    let store = crate::auth::credential::AuthStore::new();
    let bus = crate::core::event::EventBus::default();
    start(&dummy_config(), &store, "zh-CN", bus.clone(), "slot-a").expect("start a");
    start(&dummy_config(), &store, "zh-CN", bus.clone(), "slot-b").expect("start b");
    // 同 session 重复 start = 替换（序号变），别的槽不动
    let before = dummy_slot("slot-a");
    start(&dummy_config(), &store, "zh-CN", bus.clone(), "slot-a").expect("restart a");
    assert_ne!(before, dummy_slot("slot-a"), "同 session 重复 start 应替换旧槽");
    assert!(dummy_slot("slot-b").is_some(), "别的 session 槽位不得受影响");
    // stop 只 remove 自己槽
    let mrm = crate::llm::mrm::ModelResourceManager::new(Default::default());
    let text = stop(&crate::core::config::Config::default(), &store, "slot-a", &mrm, &reporter()).await.expect("stop a");
    assert_eq!(text, None);
    assert!(dummy_slot("slot-a").is_none());
    assert!(dummy_slot("slot-b").is_some(), "stop 别的 session 不得受影响");
    // 未知 session 无操作
    let text = stop(&crate::core::config::Config::default(), &store, "slot-unknown", &mrm, &reporter()).await.expect("stop unknown");
    assert_eq!(text, None);
    crate::core::shared::lock(&ACTIVE).clear();
}

#[tokio::test]
async fn drop_session_reclaims_active_slot() {
    let _test = TEST_LOCK.lock().await;
    let store = crate::auth::credential::AuthStore::new();
    start(&dummy_config(), &store, "zh-CN", crate::core::event::EventBus::default(), "voice-delete").unwrap();
    assert!(dummy_slot("voice-delete").is_some());
    drop_session("voice-delete");
    assert!(dummy_slot("voice-delete").is_none());
}

#[tokio::test]
async fn concurrent_start_same_session_leaves_single_slot() {
    let _test = TEST_LOCK.lock().await;
    // 并发 start_one 同槽：insert 顶掉的旧 Active 必须 cancel 而非直接 drop
    // （apple/provider 引擎 drop 会泄漏麦克风；dummy 下断言收敛为单槽且不 panic）
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(std::thread::spawn(|| {
            let store = crate::auth::credential::AuthStore::new();
            for _ in 0..50 {
                start(&dummy_config(), &store, "zh-CN", crate::core::event::EventBus::default(), "voice-race").unwrap();
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(dummy_slot("voice-race").is_some());
    assert_eq!(crate::core::shared::lock(&ACTIVE).len(), 1, "同槽并发 start 后不得残留多个槽");
    crate::core::shared::lock(&ACTIVE).clear();
}

#[tokio::test]
async fn event_payload_carries_session_id() {
    let _test = TEST_LOCK.lock().await;
    let p = event_payload(SessionEvent::Partial("你好".into()), "s1").unwrap();
    assert_eq!(p["kind"], "voice.partial");
    assert_eq!(p["session_id"], "s1");
    let p = event_payload(SessionEvent::Error("boom".into()), "s2").unwrap();
    assert_eq!(p["session_id"], "s2");
    // 空 session 走旧全局通道：不带 session_id 键
    let p = event_payload(SessionEvent::Partial("完".into()), "").unwrap();
    assert!(p.get("session_id").is_none());
    // Final 不出帧（终稿经 voice.stop RPC 返回）
    assert!(event_payload(SessionEvent::Final("完".into()), "s1").is_none());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn engines_without_macos_exclude_apple() {
    let store = crate::auth::credential::AuthStore::new();
    let list = engines(&crate::core::config::Config::default(), &store);
    assert!(!list.iter().any(|e| e.id == "apple"), "非 macOS 引擎表不得含 apple");
    assert!(list.iter().any(|e| e.id == "openai"), "provider 引擎保留");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn start_reports_unsupported_platform() {
    let store = crate::auth::credential::AuthStore::new();
    let config = crate::core::config::VoiceConfig { engine: "apple".into(), ..Default::default() };
    let error = start(&config, &store, "zh-CN", crate::core::event::EventBus::default(), "s1").expect_err("非 macOS apple 引擎必须失败");
    assert!(error.contains("仅 macOS"), "{error}");
}

#[test]
fn apple_cloud_upgrade_requires_explicit_fallback() {
    let mut store = crate::auth::credential::AuthStore::new();
    store.insert("voice:openai".into(), crate::auth::credential::CredentialKind::Api { key: "configured".into(), region: None });
    let mut config = crate::core::config::Config::default();
    config.voice.engine = "apple".into();
    assert_eq!(first_ready_cloud(&config, &store), None, "凭证存在不能隐式授权本地录音外发");
    assert!(!cloud_capture_enabled(&config.voice, &store), "纯本地模式不应缓冲云上传用 PCM");
    config.voice.fallback = vec!["xai".into(), "openai".into()];
    assert_eq!(first_ready_cloud(&config, &store).as_deref(), Some("openai"));
    assert!(cloud_capture_enabled(&config.voice, &store));
}
