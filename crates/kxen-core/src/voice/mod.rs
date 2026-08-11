//! 语音输入多引擎：apple（Speech.framework 本地识别，主引擎）+ provider（OpenAI 兼容转写，降级链）。
//! 麦克风采集仅 macOS 实现（Speech/AVFAudio 手写绑定，objc2 三件套只链 macOS）：其余平台
//! apple/objc 模块编译期排除，引擎表只剩 provider，voice.start 返回明确 unsupported 错误。

#[cfg(target_os = "macos")]
pub mod apple;
#[cfg(target_os = "macos")]
pub mod objc;
pub mod provider;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub id: String,
    pub label: String,
    /// 状态字面量：ready | needs_auth | unconfigured | unavailable
    pub status: String,
    pub detail: String,
}

/// 识别会话事件（apple 本地流式产出）。定义在本层而非 apple.rs：事件负载组装跨平台可测。
#[derive(Debug)]
pub enum SessionEvent {
    Partial(String),
    Final(String),
    Error(String),
}

/// 引擎状态总览（设置页语音区 + mic 菜单数据源）。
pub fn engines(config: &crate::core::config::Config, store: &crate::auth::credential::AuthStore) -> Vec<EngineStatus> {
    let mut out = Vec::new();
    // apple 引擎仅 macOS 存在；其余平台缺席，前端按清单渲染自动降级
    #[cfg(target_os = "macos")]
    out.push(apple::status());
    out.extend(provider::statuses(config, store));
    out
}

// ---------------- 活跃 PTT 会话（按 chat session 键控，多会话并发 PTT 互不打断） ----------------

// ObjC 对象句柄跨线程存放（Speech/AVAudio 回调均走框架队列，stop 路径单线程）
#[cfg(target_os = "macos")]
struct SendWrap<T>(T);
#[cfg(target_os = "macos")]
unsafe impl<T> Send for SendWrap<T> {}
#[cfg(target_os = "macos")]
unsafe impl<T> Sync for SendWrap<T> {}

enum Active {
    /// token 是泵线程身份：槽位被替换/移除后旧泵 ptr_eq 不过立即退出
    /// （无守卫的旧泵永不退出，会吸新会话事件造成串流）
    #[cfg(target_os = "macos")]
    Apple { session: SendWrap<apple::MicSession>, token: std::sync::Arc<()> },
    #[cfg(target_os = "macos")]
    Record { session: SendWrap<provider::RecordSession>, provider: String },
    #[cfg(test)]
    Dummy(u32),
}

impl Active {
    fn cancel(self) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple { session, .. } => session.0.cancel(),
            #[cfg(target_os = "macos")]
            Self::Record { session, .. } => session.0.cancel(),
            #[cfg(test)]
            Self::Dummy(_) => {}
        }
    }
}

static ACTIVE: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Active>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// PTT 按下：按 [主引擎, ...fallback] 顺序尝试启动（apple 流式 / provider 录音），partial 经 bus 泵给前端。
/// 同 session 重复 start = 替换；不同 session 互不打断。
pub fn start(
    config: &crate::core::config::VoiceConfig,
    store: &crate::auth::credential::AuthStore,
    locale: &str,
    bus: crate::core::event::EventBus,
    session_id: &str,
) -> Result<String, String> {
    let mut errors: Vec<String> = Vec::new();
    for engine in std::iter::once(config.engine.as_str()).chain(config.fallback.iter().map(String::as_str)) {
        let capture_cloud = engine == "apple" && cloud_capture_enabled(config, store);
        match start_one(engine, store, locale, &bus, session_id, capture_cloud) {
            Ok(started) => return Ok(started),
            Err(e) => errors.push(format!("{engine}: {e}")),
        }
    }
    Err(format!("全部语音引擎不可用（{}）", errors.join("; ")))
}

#[cfg(target_os = "macos")]
fn start_one(
    engine: &str,
    store: &crate::auth::credential::AuthStore,
    locale: &str,
    bus: &crate::core::event::EventBus,
    session_id: &str,
    capture_cloud: bool,
) -> Result<String, String> {
    // 同 session 重复 start = 替换：旧槽先移出（旧泵 ptr_eq 不过随即退出）
    let previous = { crate::core::shared::lock(&ACTIVE).remove(session_id) };
    if let Some(previous) = previous {
        previous.cancel();
    }
    match engine {
        "apple" => {
            let session = apple::start_mic(locale, capture_cloud)?;
            let token = std::sync::Arc::new(());
            let token_pump = token.clone();
            // 并发 start_one 同槽：insert 顶掉的旧 Active 必须 cancel（drop 会泄漏麦克风引擎）
            let displaced =
                crate::core::shared::lock(&ACTIVE).insert(session_id.to_string(), Active::Apple { session: SendWrap(session), token });
            if let Some(displaced) = displaced {
                displaced.cancel();
            }
            let bus = bus.clone();
            let key = session_id.to_string();
            std::thread::spawn(move || {
                loop {
                    let events = {
                        let map = crate::core::shared::lock(&ACTIVE);
                        match map.get(&key) {
                            Some(Active::Apple { session, token }) if std::sync::Arc::ptr_eq(token, &token_pump) => session.0.drain(),
                            _ => break,
                        }
                    };
                    for e in events {
                        if let Some(payload) = event_payload(e, &key) {
                            bus.publish(crate::core::event::Event::LlmDelta(payload));
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(80));
                }
            });
            Ok("apple".into())
        }
        #[cfg(test)]
        "dummy" => {
            // 测试引擎：避开麦克风硬件验证槽位语义，序号用于区分替换前后的槽
            static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let displaced = crate::core::shared::lock(&ACTIVE).insert(session_id.to_string(), Active::Dummy(n));
            if let Some(displaced) = displaced {
                displaced.cancel();
            }
            Ok("dummy".into())
        }
        other => {
            if !provider::configured(store, other) {
                return Err(format!("{other} 未配置 API key"));
            }
            let session = provider::start_recording()?;
            let displaced = crate::core::shared::lock(&ACTIVE)
                .insert(session_id.to_string(), Active::Record { session: SendWrap(session), provider: other.to_string() });
            if let Some(displaced) = displaced {
                displaced.cancel();
            }
            Ok(other.to_string())
        }
    }
}

/// 非 macOS：麦克风采集未实现（apple 与 provider 录音均依赖 AVFAudio），start 报明确 unsupported。
/// 槽位语义（替换/取消）经 cfg(test) dummy 引擎照常验证。
#[cfg(not(target_os = "macos"))]
fn start_one(
    engine: &str,
    store: &crate::auth::credential::AuthStore,
    _locale: &str,
    _bus: &crate::core::event::EventBus,
    _session_id: &str,
    _capture_cloud: bool,
) -> Result<String, String> {
    #[cfg(test)]
    if engine == "dummy" {
        // 测试引擎：避开麦克风硬件验证槽位语义，序号用于区分替换前后的槽
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let displaced = crate::core::shared::lock(&ACTIVE).insert(_session_id.to_string(), Active::Dummy(n));
        if let Some(displaced) = displaced {
            displaced.cancel();
        }
        return Ok("dummy".into());
    }
    if engine != "apple" && !provider::configured(store, engine) {
        return Err(format!("{engine} 未配置 API key"));
    }
    Err(format!("语音引擎 {engine} 在当前平台不可用：麦克风采集仅 macOS 实现（当前 {}）", std::env::consts::OS))
}

/// 转写事件统一携带 session_id（ws/stream.rs 的 session ACL 按它准入）；
/// 空 id 是旧全局通道：不带键，否则会被 ACL 当成未知 session 拦掉。
/// Final 不出帧：前端只消费 voice.partial/voice.error，终稿经 voice.stop RPC 返回，发了必被丢弃。
#[cfg(any(target_os = "macos", test))]
fn event_payload(e: SessionEvent, session_id: &str) -> Option<serde_json::Value> {
    let mut payload = match e {
        SessionEvent::Partial(t) => serde_json::json!({"kind": "voice.partial", "text": t}),
        SessionEvent::Final(_) => return None,
        SessionEvent::Error(m) => serde_json::json!({"kind": "voice.error", "message": m}),
    };
    if !session_id.is_empty() {
        payload.as_object_mut().expect("voice payload").insert("session_id".into(), serde_json::Value::String(session_id.into()));
    }
    Some(payload)
}

/// PTT 松开：只停自己槽（别的 session 继续录）。apple 先出本地终稿，有就绪云引擎则云转写升级（本地+云端双轨）；失败回落本地。
pub async fn stop(
    config: &crate::core::config::Config,
    store: &crate::auth::credential::AuthStore,
    session_id: &str,
    mrm: &crate::llm::mrm::ModelResourceManager,
    usage_reporter: &crate::agent::agent_loop::UsageReporter,
) -> Result<Option<String>, String> {
    // 非 macOS：槽位恒空（start 一律 Err），这些参数只服务于 macOS 臂
    #[cfg(not(target_os = "macos"))]
    let _ = (config, store, mrm, usage_reporter);
    // 先取槽再 match：guard 临时量若写在 scrutinee 里会活过 arm 内的 await（非 Send）
    let slot = crate::core::shared::lock(&ACTIVE).remove(session_id);
    match slot {
        None => Ok(None),
        #[cfg(target_os = "macos")]
        Some(Active::Apple { session, .. }) => {
            let (local, wav) = session.0.stop();
            let wav = match wav {
                Ok(wav) => wav,
                Err(error) => return Err(apple_fallback_error(error, local.as_deref())),
            };
            // 云转写终稿：fallback 链里第一个有 key 的 provider（含 audio 自定义）
            if let Some(path) = wav {
                let cloud = match first_ready_cloud(config, store) {
                    Some(engine) => {
                        let result = provider::transcribe_file(config, store, &engine, &path, mrm, usage_reporter).await;
                        let _ = std::fs::remove_file(&path);
                        match result {
                            Ok(text) => Some(text),
                            Err(error) => return Err(apple_fallback_error(error, local.as_deref())),
                        }
                    }
                    None => {
                        let _ = std::fs::remove_file(&path);
                        None
                    }
                };
                return Ok(cloud.or(local));
            }
            Ok(local)
        }
        #[cfg(target_os = "macos")]
        Some(Active::Record { session, provider }) => {
            let (path, _dur) = session.0.stop()?;
            let text = provider::transcribe_file(config, store, &provider, &path, mrm, usage_reporter).await;
            let _ = std::fs::remove_file(&path);
            text.map(Some)
        }
        #[cfg(test)]
        Some(Active::Dummy(_)) => Ok(None),
        // 非 macOS 非测试构建 Active 无可构造变体，match 需要兜底臂
        #[cfg(all(not(target_os = "macos"), not(test)))]
        Some(_) => Ok(None),
    }
}

#[cfg(target_os = "macos")]
fn apple_fallback_error(error: String, local: Option<&str>) -> String {
    match local {
        Some(local) if !local.is_empty() => format!("Apple cloud fallback failed: {error}\nLocal transcript preserved: {local}"),
        _ => format!("Apple cloud fallback failed: {error}"),
    }
}

/// Session 生命周期终点：停止并移除仍占用麦克风的 PTT 槽。
pub fn drop_session(session_id: &str) {
    let active = { crate::core::shared::lock(&ACTIVE).remove(session_id) };
    if let Some(active) = active {
        active.cancel();
    }
}

/// Apple 本地终稿只有显式 fallback 才允许上传。存在任意 Provider 凭证本身
/// 不构成外发授权，避免本地模式静默改变隐私边界。
#[cfg(any(target_os = "macos", test))]
fn first_ready_cloud(config: &crate::core::config::Config, store: &crate::auth::credential::AuthStore) -> Option<String> {
    config.voice.fallback.iter().find(|id| provider::configured(store, id)).cloned()
}

fn cloud_capture_enabled(config: &crate::core::config::VoiceConfig, store: &crate::auth::credential::AuthStore) -> bool {
    config.fallback.iter().any(|candidate| provider::configured(store, candidate))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
