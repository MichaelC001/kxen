//! 语音输入多引擎：apple（Speech.framework 本地识别，主引擎）+ provider（OpenAI 兼容转写，降级链）。

pub mod apple;
pub mod objc;
pub mod provider;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub id: String,
    pub label: String,
    /// ready | needs_auth | unconfigured | unavailable
    pub status: String,
    pub detail: String,
}

/// 引擎状态总览（设置页语音区 + mic 菜单数据源）。
pub fn engines(config: &crate::core::config::Config, store: &crate::auth::credential::AuthStore) -> Vec<EngineStatus> {
    let mut out = vec![apple::status()];
    out.extend(provider::statuses(config, store));
    out
}

/// 文件识别统一入口（E2E 与排障共用）：按引擎 id 分发，空 id 走默认链。
pub async fn transcribe_file(config: &crate::core::config::Config, store: &crate::auth::credential::AuthStore, engine: Option<&str>, path: &str, locale: &str) -> Result<String, String> {
    let id = engine.unwrap_or(&config.voice.engine);
    match id {
        "apple" => apple::recognize_file(path, locale),
        other => provider::transcribe_file(config, store, other, path).await,
    }
}

// ---------------- 活跃 PTT 会话 ----------------

// ObjC 对象句柄跨线程存放（Speech/AVAudio 回调均走框架队列，stop 路径单线程）
struct SendWrap<T>(T);
unsafe impl<T> Send for SendWrap<T> {}
unsafe impl<T> Sync for SendWrap<T> {}

enum Active {
    Apple { session: SendWrap<apple::MicSession>, alive: std::sync::Arc<std::sync::atomic::AtomicBool> },
    Record { session: SendWrap<provider::RecordSession>, provider: String },
}

static ACTIVE: std::sync::Mutex<Option<Active>> = std::sync::Mutex::new(None);

/// PTT 按下：按 [主引擎, ...fallback] 顺序尝试启动（apple 流式 / provider 录音），partial 经 bus 泵给前端。
pub fn start(config: &crate::core::config::VoiceConfig, store: &crate::auth::credential::AuthStore, locale: &str, bus: crate::core::event::EventBus) -> Result<String, String> {
    let mut errors: Vec<String> = Vec::new();
    for engine in std::iter::once(config.engine.as_str()).chain(config.fallback.iter().map(String::as_str)) {
        match start_one(engine, store, locale, &bus) {
            Ok(started) => return Ok(started),
            Err(e) => errors.push(format!("{engine}: {e}")),
        }
    }
    Err(format!("全部语音引擎不可用（{}）", errors.join("; ")))
}

fn start_one(engine: &str, store: &crate::auth::credential::AuthStore, locale: &str, bus: &crate::core::event::EventBus) -> Result<String, String> {
    let _ = stop_now();
    match engine {
        "apple" => {
            let session = apple::start_mic(locale)?;
            let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let alive_pump = alive.clone();
            *crate::core::shared::lock(&ACTIVE) = Some(Active::Apple { session: SendWrap(session), alive });
            let bus = bus.clone();
            std::thread::spawn(move || {
                while alive_pump.load(std::sync::atomic::Ordering::Relaxed) {
                    let events = match crate::core::shared::lock(&ACTIVE).as_ref() {
                        Some(Active::Apple { session, .. }) => session.0.drain(),
                        _ => Vec::new(),
                    };
                    for e in events {
                        let payload = match e {
                            apple::SessionEvent::Partial(t) => serde_json::json!({"kind": "voice.partial", "text": t}),
                            apple::SessionEvent::Final(t) => serde_json::json!({"kind": "voice.final", "text": t}),
                            apple::SessionEvent::Error(m) => serde_json::json!({"kind": "voice.error", "message": m}),
                        };
                        bus.publish(crate::core::event::Event::LlmDelta(payload));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(80));
                }
            });
            Ok("apple".into())
        }
        other => {
            if !provider::configured(store, other) {
                return Err(format!("{other} 未配置 API key"));
            }
            let session = provider::start_recording()?;
            *crate::core::shared::lock(&ACTIVE) = Some(Active::Record { session: SendWrap(session), provider: other.to_string() });
            Ok(other.to_string())
        }
    }
}

fn stop_now() -> Option<Active> {
    crate::core::shared::lock(&ACTIVE).take()
}

/// PTT 松开：apple 等 final；provider 落 WAV 上传转写。返回最终文本（可空）。
pub async fn stop(config: &crate::core::config::Config, store: &crate::auth::credential::AuthStore) -> Result<Option<String>, String> {
    match stop_now() {
        None => Ok(None),
        Some(Active::Apple { session, alive }) => {
            alive.store(false, std::sync::atomic::Ordering::Relaxed);
            Ok(session.0.stop())
        }
        Some(Active::Record { session, provider }) => {
            let (path, _dur) = session.0.stop()?;
            let text = provider::transcribe_file(config, store, &provider, &path).await;
            let _ = std::fs::remove_file(&path);
            text.map(Some)
        }
    }
}
