//! 配置（~/.config/kxen/config.toml + 项目 .kxen/config.toml，项目级覆盖用户级）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub roles: HashMap<String, RoleBinding>,
    pub limits: Limits,
    pub hooks: HashMap<String, Vec<HookDef>>,
    pub statusline: StatuslineConfig,
    pub voice: VoiceConfig,
    pub custom_providers: HashMap<String, CustomProviderDef>,
    /// 运行中再发消息的策略：queue（默认，排队接续）| interrupt（打断当前立即发送）
    pub send_when_running: String,
}

/// 自定义类型提供商：base_url + 模型清单 + 协议（openai|anthropic）+ 能力标记（text/vision/audio）。
/// api key 存 auth.json（custom:<name>）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomProviderDef {
    pub base_url: String,
    pub models: Vec<String>,
    pub protocol: String,
    pub capabilities: Vec<String>,
}

impl Default for CustomProviderDef {
    fn default() -> Self {
        Self { base_url: String::new(), models: vec![], protocol: "openai".into(), capabilities: vec!["text".into()] }
    }
}

/// 语音输入：引擎选择 + 降级链 + locale（API key 不落 config，走 auth.json）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    /// 主引擎 id：apple | openai | xai
    pub engine: String,
    /// 失败时依序降级的引擎 id
    pub fallback: Vec<String>,
    /// 识别语言（BCP-47，如 zh-CN / en-US）
    pub locale: String,
    /// provider 引擎的转写模型名（如 whisper-1）
    pub transcribe_model: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            engine: "apple".into(),
            fallback: vec![],
            locale: "zh-CN".into(),
            transcribe_model: "whisper-1".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookDef {
    /// 工具名正则（None = 全部工具）。
    pub matcher: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RoleBinding {
    pub provider: String,
    pub model: String,
    /// 降级目标角色（None = mrm 静态兜底链）。
    pub fallback: Option<String>,
    /// 账号钉选（None = 默认账号链轮转；多账号 quota 池化）
    pub account: Option<String>,
}

/// 状态栏显隐（固定段 + 开关，对齐 Zed 白名单模式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StatuslineConfig {
    pub items: Vec<String>,
}

impl Default for StatuslineConfig {
    fn default() -> Self {
        Self {
            items: ["workdir", "git", "goal", "tasks", "tokens", "ctx", "model"].iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub global_concurrent: u32,
    pub providers: HashMap<String, ProviderLimit>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            global_concurrent: 8,
            providers: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderLimit {
    pub concurrent: Option<u32>,
    pub rpm: Option<u32>,
}

impl Config {
    pub fn load(user: &Path, project: Option<&Path>) -> crate::core::Result<Self> {
        let mut config = Config::default();
        for path in [Some(user.to_path_buf()), project.map(|p| p.to_path_buf())].into_iter().flatten() {
            if !path.exists() {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let parsed: Config = toml::from_str(&text)?;
            config.merge(parsed);
        }
        Ok(config)
    }

    fn merge(&mut self, other: Config) {
        self.roles.extend(other.roles);
        if other.limits.global_concurrent != 0 {
            self.limits.global_concurrent = other.limits.global_concurrent;
        }
        self.limits.providers.extend(other.limits.providers);
        for (event, defs) in other.hooks {
            self.hooks.entry(event).or_default().extend(defs);
        }
        if !other.statusline.items.is_empty() {
            self.statusline = other.statusline;
        }
        if other.voice != VoiceConfig::default() {
            self.voice = other.voice;
        }
        self.custom_providers.extend(other.custom_providers);
    }
}

/// voice.set_engine 的局部更新：在既有 [voice] 表上只覆盖 engine/fallback，
/// locale/transcribe_model 等其他键保留；空 fallback 维持旧语义（不动既有链）。
pub fn merge_voice_engine(doc: &mut toml::Table, engine: &str, fallback: &[String]) {
    let entry = doc.entry("voice").or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::Table::new());
    }
    let voice = entry.as_table_mut().expect("voice table");
    voice.insert("engine".into(), toml::Value::String(engine.into()));
    if !fallback.is_empty() {
        voice.insert("fallback".into(), toml::Value::Array(fallback.iter().map(|f| toml::Value::String(f.clone())).collect()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_voice_engine_keeps_other_voice_keys() {
        let mut doc: toml::Table = toml::from_str(
            "[voice]\nengine = \"apple\"\nfallback = [\"openai\"]\nlocale = \"en-US\"\ntranscribe_model = \"whisper-1\"\n",
        )
        .expect("fixture toml");
        merge_voice_engine(&mut doc, "openai", &["xai".to_string()]);
        let voice = doc["voice"].as_table().expect("voice table");
        assert_eq!(voice["engine"].as_str(), Some("openai"));
        assert_eq!(voice["fallback"].as_array().map(Vec::len), Some(1));
        assert_eq!(voice["locale"].as_str(), Some("en-US"), "locale 不得丢");
        assert_eq!(voice["transcribe_model"].as_str(), Some("whisper-1"), "transcribe_model 不得丢");

        // 空 fallback 维持旧语义：不动既有链
        merge_voice_engine(&mut doc, "apple", &[]);
        let voice = doc["voice"].as_table().expect("voice table");
        assert_eq!(voice["engine"].as_str(), Some("apple"));
        assert_eq!(voice["fallback"].as_array().map(Vec::len), Some(1), "空 fallback 不得清掉既有链");

        // 无 [voice] 表时新建
        let mut empty = toml::Table::new();
        merge_voice_engine(&mut empty, "apple", &[]);
        assert_eq!(empty["voice"]["engine"].as_str(), Some("apple"));
    }
}
