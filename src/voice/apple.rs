//! Apple 原生语音引擎（Speech.framework，本地离线识别 zh/en）。

use super::objc;
use super::EngineStatus;

pub fn status() -> EngineStatus {
    let (status, detail) = match objc::authorization_status() {
        objc::SpeechAuth::Authorized => ("ready", "Speech.framework 已授权"),
        objc::SpeechAuth::NotDetermined => ("needs_auth", "首次使用将请求语音识别权限"),
        _ => ("unavailable", "语音识别权限被拒绝/受限，请在系统设置开启"),
    };
    EngineStatus { id: "apple".into(), label: "Apple 本地识别".into(), status: status.into(), detail: detail.into() }
}

/// 整文件识别 -> 最终文本（E2E 与排障共用；90s 超时）。
pub fn recognize_file(path: &str, locale: &str) -> Result<String, String> {
    ensure_authorized()?;
    let recognizer = objc::new_recognizer(locale).ok_or_else(|| format!("无法创建识别器（locale {locale}）"))?;
    if !objc::is_available(&recognizer) {
        return Err("识别服务当前不可用".into());
    }
    let request = objc::url_request(path).ok_or("无法创建识别请求")?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    let handler = objc::ResultHandler::new(move |result, error| {
        if let Some(err) = objc::error_text(error) {
            let _ = tx.send(Err(err));
            return;
        }
        if let Some((text, true)) = objc::result_text(result) {
            let _ = tx.send(Ok(text));
        }
    });
    let task = objc::recognition_task(&recognizer, &request, &handler).ok_or("无法启动识别任务")?;
    let out = rx.recv_timeout(std::time::Duration::from_secs(90)).map_err(|_| "识别超时（90s）".to_string());
    objc::cancel_task(&task);
    out?
}

fn ensure_authorized() -> Result<(), String> {
    match objc::authorization_status() {
        objc::SpeechAuth::Authorized => Ok(()),
        objc::SpeechAuth::NotDetermined => {
            let (tx, rx) = std::sync::mpsc::channel();
            objc::request_authorization(move |s| {
                let _ = tx.send(s);
            });
            let s = rx.recv_timeout(std::time::Duration::from_secs(60)).map_err(|_| "授权等待超时".to_string())?;
            if s == objc::SpeechAuth::Authorized { Ok(()) } else { Err("语音识别权限未授予".into()) }
        }
        _ => Err("语音识别权限被拒绝/受限".into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn status_renders() {
        let s = super::status();
        assert_eq!(s.id, "apple");
        assert!(["ready", "needs_auth", "unavailable"].contains(&s.status.as_str()));
    }
}

// ---------------- 麦克风流式会话 ----------------

use objc2::rc::Retained;
use objc2::runtime::AnyObject;

#[derive(Debug)]
pub enum SessionEvent {
    Partial(String),
    Final(String),
    Error(String),
}

pub struct MicSession {
    task: Retained<AnyObject>,
    engine: Retained<AnyObject>,
    request: Retained<AnyObject>,
    rx: std::sync::mpsc::Receiver<SessionEvent>,
}

/// 启动麦克风识别（PTT 按下）。
pub fn start_mic(locale: &str) -> Result<MicSession, String> {
    ensure_authorized()?;
    let recognizer = objc::new_recognizer(locale).ok_or_else(|| format!("无法创建识别器（locale {locale}）"))?;
    if !objc::is_available(&recognizer) {
        return Err("识别服务当前不可用".into());
    }
    let request = objc::buffer_request().ok_or("无法创建缓冲识别请求")?;
    let (tx, rx) = std::sync::mpsc::channel::<SessionEvent>();
    let handler = objc::ResultHandler::new(move |result, error| {
        if let Some(e) = objc::error_text(error) {
            let _ = tx.send(SessionEvent::Error(e));
            return;
        }
        if let Some((text, is_final)) = objc::result_text(result) {
            let _ = tx.send(if is_final { SessionEvent::Final(text) } else { SessionEvent::Partial(text) });
        }
    });
    let task = objc::recognition_task(&recognizer, &request, &handler).ok_or("无法启动识别任务")?;
    // tap 线程持有 request 一份（防悬垂）；session 结束时进程级泄漏回收
    let req_ptr = &*request as *const AnyObject as *mut AnyObject;
    let req_kept = unsafe { objc::retain_autoreleased(req_ptr) }.ok_or("request 持有失败")?;
    let engine = objc::start_mic_capture(move |_input| {
        objc::TapHandler::new(move |buffer: *mut AnyObject, _time: *mut AnyObject| {
            if !buffer.is_null() {
                objc::append_buffer(unsafe { &*req_ptr }, buffer);
            }
        })
    })
    .map(|(e, _rate)| e)?;
    std::mem::forget(req_kept);
    Ok(MicSession { task, engine, request, rx })
}

impl MicSession {
    /// 非阻塞排空已到事件（泵给前端）。
    pub fn drain(&self) -> Vec<SessionEvent> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            out.push(e);
        }
        out
    }

    /// PTT 松开：停止采集 -> endAudio -> 等 final（3s 兜底）-> cancel。
    pub fn stop(self) -> Option<String> {
        objc::stop_mic_engine(&self.engine);
        objc::end_audio(&self.request);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut last: Option<String> = None;
        loop {
            let remain = deadline.saturating_duration_since(std::time::Instant::now());
            if remain.is_zero() {
                break;
            }
            match self.rx.recv_timeout(remain) {
                Ok(SessionEvent::Final(t)) => {
                    last = Some(t);
                    break;
                }
                Ok(SessionEvent::Partial(t)) => last = Some(t),
                Ok(SessionEvent::Error(_)) | Err(_) => break,
            }
        }
        objc::cancel_task(&self.task);
        last
    }
}
