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
    /// 记忆检索的 embedding 语义召回（缺省关闭，纯 BM25）
    pub embedding: EmbeddingConfig,
}

/// embedding 语义召回：三档 provider（openai / openrouter / ollama），缺省 provider 为空 = 关闭。
/// api key 不落 config，复用 auth.json 的同 provider 凭证（ollama 无鉴权）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    /// ""（关闭）| "openai" | "openrouter" | "ollama"
    pub provider: String,
    /// 模型覆盖：缺省 openai/openrouter = text-embedding-3-small，ollama = nomic-embed-text
    pub model: String,
    /// base URL 覆盖（ollama 非默认端口、自建 OpenAI 兼容网关）；缺省按 provider 给官方端点
    pub base_url: String,
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
        Self { engine: "apple".into(), fallback: vec![], locale: "zh-CN".into(), transcribe_model: "whisper-1".into() }
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
        Self { items: ["workdir", "git", "goal", "tasks", "tokens", "ctx", "model"].iter().map(|s| s.to_string()).collect() }
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
        Self { global_concurrent: 8, providers: HashMap::new() }
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
        config.seed_default_roles();
        Ok(config)
    }

    /// 五角色默认绑定：只补缺位（用户 config 逐项覆盖）。面向四订阅持有者择型：
    /// 思考/评审走 claude（评审需独立产出质量），执行走 grok-build（命令调度快），
    /// 研究走 grok-4.5（长上下文检索），规划走 kimi k2 thinking。用户没有的订阅
    /// 由 mrm candidates 跳过（无凭证 provider 不出候选），降级链走到真实持有的订阅。
    fn seed_default_roles(&mut self) {
        let binding = |provider: &str, model: &str, fallback: Option<&str>| RoleBinding {
            provider: provider.into(),
            model: model.into(),
            fallback: fallback.map(String::from),
            account: None,
        };
        let defaults: [(&str, RoleBinding); 5] = [
            ("thinking", binding("anthropic", "claude-opus-4-8", Some("planning"))),
            ("planning", binding("kimi", "kimi-k2-thinking", Some("review"))),
            ("execution", binding("xai", "grok-build-0.1", Some("research"))),
            ("review", binding("anthropic", "claude-sonnet-4-6", Some("thinking"))),
            ("research", binding("xai", "grok-4.5", Some("execution"))),
        ];
        for (role, b) in defaults {
            self.roles.entry(role.to_string()).or_insert(b);
        }
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
        if other.embedding != EmbeddingConfig::default() {
            self.embedding = other.embedding;
        }
    }
}

/// voice.set_engine 的局部更新：覆盖 engine/fallback（空数组 = 清空降级链；
/// 前端两个调用点都显式传当前链，旧的「空 = 不动」语义已无人依赖），
/// locale 仅 Some 时覆盖；transcribe_model 等其他键保留。
pub fn merge_voice_engine(doc: &mut toml::Table, engine: &str, fallback: &[String], locale: Option<&str>) {
    let entry = doc.entry("voice").or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::Table::new());
    }
    let voice = entry.as_table_mut().expect("voice table");
    voice.insert("engine".into(), toml::Value::String(engine.into()));
    voice.insert("fallback".into(), toml::Value::Array(fallback.iter().map(|f| toml::Value::String(f.clone())).collect()));
    if let Some(l) = locale {
        voice.insert("locale".into(), toml::Value::String(l.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_voice_engine_keeps_other_voice_keys() {
        let mut doc: toml::Table =
            toml::from_str("[voice]\nengine = \"apple\"\nfallback = [\"openai\"]\nlocale = \"en-US\"\ntranscribe_model = \"whisper-1\"\n")
                .expect("fixture toml");
        merge_voice_engine(&mut doc, "openai", &["xai".to_string()], None);
        let voice = doc["voice"].as_table().expect("voice table");
        assert_eq!(voice["engine"].as_str(), Some("openai"));
        assert_eq!(voice["fallback"].as_array().map(Vec::len), Some(1));
        assert_eq!(voice["locale"].as_str(), Some("en-US"), "locale 不传不得丢");
        assert_eq!(voice["transcribe_model"].as_str(), Some("whisper-1"), "transcribe_model 不得丢");

        // locale 传入即覆盖
        merge_voice_engine(&mut doc, "apple", &["xai".to_string()], Some("zh-CN"));
        assert_eq!(doc["voice"]["locale"].as_str(), Some("zh-CN"));

        // 空 fallback = 显式清空降级链（前端总是显式传当前链）
        merge_voice_engine(&mut doc, "apple", &[], None);
        let voice = doc["voice"].as_table().expect("voice table");
        assert_eq!(voice["fallback"].as_array().map(Vec::len), Some(0), "空数组必须清链");

        // 无 [voice] 表时新建
        let mut empty = toml::Table::new();
        merge_voice_engine(&mut empty, "apple", &[], None);
        assert_eq!(empty["voice"]["engine"].as_str(), Some("apple"));
    }
}
