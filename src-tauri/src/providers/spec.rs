//! provider 声明式 spec 类型：内置提供商的唯一真相源（single source of truth）。
//! client 路由 / 模型目录 / verify 默认模型 / 设置页 RPC 全部消费 registry，不再各自硬编码。

/// 协议形态（决定端点路径推导与 wire 实现选择）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// Anthropic /v1/messages（x-api-key 头 + anthropic-version）
    Anthropic,
    /// OpenAI /chat/completions 兼容（bearer 头；覆盖 xAI/月之暗面/国产厂商/本地 ollama 全部长尾）
    OpenAiCompat,
}

/// 认证形态（决定设置页表单与凭证是否必需）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// 官方平台 API key（auth.json 手填）
    ApiKey,
    /// CLI 订阅 OAuth 导入为主（也接受手填 key）
    Oauth,
    /// 本地免鉴权（ollama）
    LocalFree,
}

/// 运营区域：同一家厂商的中国版/国际版是不同 base URL 的独立服务端点。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct RegionSpec {
    /// "cn" | "intl" | "global"（无区域差异一律 global）
    pub key: &'static str,
    /// 短标签（前端拼 "{display} {region.display}" 展示，如「Kimi 中国版」）
    pub display: &'static str,
    /// API 根（不带尾斜杠；chat/models 端点路径按 protocol 推导）
    pub base_url: &'static str,
}

/// 静态兜底模型（models.dev 首次不可达时的最小可用集；live 快照到手后覆盖）。
#[derive(Debug, Clone, Copy)]
pub struct StaticModel {
    pub id: &'static str,
    pub name: &'static str,
    pub context: u64,
    pub reasoning: bool,
    pub attachment: bool,
}

/// 内置提供商声明（编译期常量；全表见 providers::REGISTRY）。
#[derive(Debug, Clone, Copy)]
pub struct ProviderSpec {
    pub key: &'static str,
    pub display: &'static str,
    pub protocol: Protocol,
    pub auth: AuthKind,
    /// 区域列表；缺省区域 = 第一条（存量凭证无 region 字段时的解析落点）
    pub regions: &'static [RegionSpec],
    /// 端点是否暴露 GET /models（false 时 provider.models 明确报不支持，前端用内置目录）
    pub models_endpoint: bool,
    /// verify 实测与路由配置的默认模型（registry 门禁要求出现在 static_models 里）
    pub default_model: &'static str,
    pub doc_url: &'static str,
    /// models.dev api.json 的 key（None = models.dev 未收录官方源，目录只靠静态兜底）
    pub models_dev: Option<&'static str>,
    pub static_models: &'static [StaticModel],
}

impl ProviderSpec {
    /// 缺省区域（regions 首条；registry 测试门禁保证非空）。
    pub fn default_region(&self) -> &'static RegionSpec {
        &self.regions[0]
    }

    /// 区域解析：None / 未知 key 一律回落缺省区域（存量凭证无 region 的兼容路径）。
    pub fn region(&self, key: Option<&str>) -> &'static RegionSpec {
        key.and_then(|k| self.regions.iter().find(|r| r.key == k)).unwrap_or_else(|| self.default_region())
    }

    pub fn has_regions(&self) -> bool {
        self.regions.len() > 1
    }

    /// chat 端点：anthropic = {base}/v1/messages；openai 兼容 = {base}/chat/completions。
    pub fn chat_url(&self, region: Option<&str>) -> String {
        let base = self.region(region).base_url;
        match self.protocol {
            Protocol::Anthropic => format!("{base}/v1/messages"),
            Protocol::OpenAiCompat => format!("{base}/chat/completions"),
        }
    }

    /// /models 端点（None = 端点未暴露，调用方回落静态目录）。
    pub fn models_url(&self, region: Option<&str>) -> Option<String> {
        if !self.models_endpoint {
            return None;
        }
        let base = self.region(region).base_url;
        Some(match self.protocol {
            Protocol::Anthropic => format!("{base}/v1/models"),
            Protocol::OpenAiCompat => format!("{base}/models"),
        })
    }
}
