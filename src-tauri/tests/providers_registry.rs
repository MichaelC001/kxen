//! providers registry 集成测试：查找 / 区域解析 / URL 推导 / 存量兼容 / 区域账号共存。
//! 禁真实网络：只测纯函数与 serde。

use kxen_gui::auth::credential::{AuthStore, CredentialKind, account_id, credential_for};
use kxen_gui::providers::{self, AuthKind, Protocol};

/// 高压线：存量 11 家 provider key 必须原样可解析（旧 config/auth.json/MRM 角色路由零迁移）。
#[test]
fn legacy_eleven_keys_resolve() {
    for key in
        ["anthropic", "openai", "xai", "kimi-for-coding", "openrouter", "ollama", "deepseek", "mistral", "groq", "google", "together"]
    {
        assert!(providers::find(key).is_some(), "存量 key 丢失: {key}");
    }
    assert!(providers::find("no-such-provider").is_none());
}

/// 新增厂商全部可解析（区域四家 + 国产/国际长尾）。
#[test]
fn new_providers_resolve() {
    for key in [
        "kimi",
        "zhipu",
        "qwen",
        "minimax",
        "siliconflow",
        "stepfun",
        "doubao",
        "yi",
        "hunyuan",
        "qianfan",
        "fireworks",
        "cerebras",
        "sambanova",
        "perplexity",
        "cohere",
        "github_models",
        "novita",
    ] {
        assert!(providers::find(key).is_some(), "新厂商缺失: {key}");
    }
}

/// registry 结构不变量：key 唯一 / regions 非空且 key 唯一 / default_model 必在 static_models /
/// base 不带尾斜杠 / 非 https 仅限 localhost / region key 只用 cn|intl|global。
#[test]
fn registry_invariants() {
    let mut keys = std::collections::HashSet::new();
    for s in providers::all() {
        assert!(keys.insert(s.key), "key 重复: {}", s.key);
        assert!(!s.regions.is_empty(), "{} regions 为空", s.key);
        let mut rkeys = std::collections::HashSet::new();
        for r in s.regions {
            assert!(rkeys.insert(r.key), "{} region key 重复: {}", s.key, r.key);
            assert!(["cn", "intl", "global"].contains(&r.key), "{} 非法 region key: {}", s.key, r.key);
            assert!(!r.base_url.ends_with('/'), "{} base 尾斜杠", s.key);
            let local = r.base_url.contains("localhost") || r.base_url.contains("127.0.0.1");
            assert!(r.base_url.starts_with("https://") || local, "{} base 必须 https（本地除外）: {}", s.key, r.base_url);
        }
        assert!(s.static_models.iter().any(|m| m.id == s.default_model), "{} default_model 不在 static_models", s.key);
        assert!(s.static_models.iter().all(|m| m.context > 0), "{} 静态种子 context 必须 > 0", s.key);
        assert!(!s.display.is_empty() && !s.doc_url.is_empty());
    }
    // 有区域差异的厂商 cn 必须是缺省（regions[0]），与国内优先的产品默认一致
    for key in ["kimi", "zhipu", "qwen", "minimax", "siliconflow", "stepfun"] {
        let s = providers::find(key).unwrap();
        assert!(s.has_regions(), "{key} 应该是多区域");
        assert_eq!(s.default_region().key, "cn", "{key} 缺省区域应为 cn");
    }
}

/// 区域查找：显式 key / None 缺省 / 未知 key 回落缺省。
#[test]
fn region_lookup_and_fallback() {
    let kimi = providers::find("kimi").unwrap();
    assert_eq!(kimi.region(Some("cn")).base_url, "https://api.moonshot.cn/v1");
    assert_eq!(kimi.region(Some("intl")).base_url, "https://api.moonshot.ai/v1");
    assert_eq!(kimi.region(None).key, "cn");
    assert_eq!(kimi.region(Some("bogus")).key, "cn", "未知 region 回落缺省");
    let deepseek = providers::find("deepseek").unwrap();
    assert!(!deepseek.has_regions());
    assert_eq!(deepseek.region(Some("intl")).key, "global", "单区域 provider 任何 region 都归一到 global");
}

/// URL 推导：chat/models 端点按协议拼路径；区域只换 host 根。
#[test]
fn url_construction() {
    let kimi = providers::find("kimi").unwrap();
    assert_eq!(kimi.chat_url(Some("cn")), "https://api.moonshot.cn/v1/chat/completions");
    assert_eq!(kimi.chat_url(Some("intl")), "https://api.moonshot.ai/v1/chat/completions");
    assert_eq!(kimi.models_url(Some("cn")).as_deref(), Some("https://api.moonshot.cn/v1/models"));
    let zhipu = providers::find("zhipu").unwrap();
    assert_eq!(zhipu.chat_url(Some("intl")), "https://api.z.ai/api/paas/v4/chat/completions");
    let qwen = providers::find("qwen").unwrap();
    assert_eq!(qwen.chat_url(Some("cn")), "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions");
    let anthropic = providers::find("anthropic").unwrap();
    assert!(matches!(anthropic.protocol, Protocol::Anthropic));
    assert_eq!(anthropic.chat_url(None), "https://api.anthropic.com/v1/messages");
    assert_eq!(anthropic.models_url(None).as_deref(), Some("https://api.anthropic.com/v1/models"));
    // Gemini OpenAI 兼容层未暴露 /models（与旧 compat 表行为一致）
    assert!(providers::find("google").unwrap().models_url(None).is_none());
    assert!(providers::find("kimi-for-coding").unwrap().models_url(None).is_none());
}

/// 高压线：旧 compat 表五家 + openrouter/ollama/kimi-for-coding/xai 的端点逐字符不变。
#[test]
fn legacy_endpoint_urls_unchanged() {
    let cases = [
        ("deepseek", "https://api.deepseek.com/chat/completions"),
        ("mistral", "https://api.mistral.ai/v1/chat/completions"),
        ("groq", "https://api.groq.com/openai/v1/chat/completions"),
        ("google", "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"),
        ("together", "https://api.together.xyz/v1/chat/completions"),
        ("openrouter", "https://openrouter.ai/api/v1/chat/completions"),
        ("ollama", "http://localhost:11434/v1/chat/completions"),
        ("kimi-for-coding", "https://api.kimi.com/coding/v1/chat/completions"),
        ("xai", "https://api.x.ai/v1/chat/completions"),
    ];
    for (key, url) in cases {
        let spec = providers::find(key).unwrap();
        assert_eq!(spec.chat_url(None), url, "{key} chat 端点漂移");
    }
    // /models 端点（google 除外，上面已测 None）
    assert_eq!(providers::find("deepseek").unwrap().models_url(None).as_deref(), Some("https://api.deepseek.com/models"));
    assert_eq!(providers::find("together").unwrap().models_url(None).as_deref(), Some("https://api.together.xyz/v1/models"));
    // verify 默认模型对齐旧表
    assert_eq!(providers::find("openrouter").unwrap().default_model, "openai/gpt-5.4");
    assert_eq!(providers::find("ollama").unwrap().default_model, "llama3.3");
}

/// 向后兼容：旧 auth.json（无 region 字段）正常加载，行为 = 缺省区域。
#[test]
fn legacy_auth_json_without_region_loads() {
    let store: AuthStore =
        serde_json::from_str(r#"{"kimi": {"type": "api", "key": "sk-cn"}, "deepseek": {"type": "api", "key": "sk-ds"}}"#)
            .expect("旧格式 auth.json 必须能加载");
    let kimi_cred = credential_for(&store, "kimi", None).expect("kimi 凭证");
    assert_eq!(kimi_cred.region(), None, "旧条目无 region");
    let spec = providers::find("kimi").unwrap();
    assert_eq!(spec.chat_url(kimi_cred.region()), "https://api.moonshot.cn/v1/chat/completions", "无 region 按缺省 cn");
    let ds = credential_for(&store, "deepseek", None).expect("deepseek 凭证");
    assert_eq!(ds.bearer(), "sk-ds");
    // 写回序列化不带 region 键（skip_serializing_if），文件形态与旧版一致
    let json = serde_json::to_string(&store).unwrap();
    assert!(!json.contains("region"), "无区域的条目不得序列化出 region 键: {json}");
}

/// 区域账号共存：同 provider 的 cn/intl 两个账号各有凭证，路由各自打到对应端点。
#[test]
fn regional_accounts_coexist() {
    let mut store = AuthStore::new();
    store.insert("kimi".into(), CredentialKind::Api { key: "sk-cn".into(), region: Some("cn".into()) });
    store.insert(account_id("kimi", "intl"), CredentialKind::Api { key: "sk-intl".into(), region: Some("intl".into()) });
    let spec = providers::find("kimi").unwrap();
    let cn = credential_for(&store, "kimi", Some("default")).expect("默认账号");
    assert_eq!(spec.chat_url(cn.region()), "https://api.moonshot.cn/v1/chat/completions");
    let intl = credential_for(&store, "kimi", Some("intl")).expect("intl 命名账号");
    assert_eq!(spec.chat_url(intl.region()), "https://api.moonshot.ai/v1/chat/completions");
    assert_eq!(intl.bearer(), "sk-intl");
    // 不指定账号：默认账号优先（存量解析顺序不变）
    let auto = credential_for(&store, "kimi", None).expect("自动解析");
    assert_eq!(auto.bearer(), "sk-cn");
    // region 序列化往返不丢
    let text = serde_json::to_string(&store).unwrap();
    let back: AuthStore = serde_json::from_str(&text).unwrap();
    assert_eq!(credential_for(&back, "kimi", Some("intl")).unwrap().region(), Some("intl"));
}

/// 认证形态归类：订阅四家 Oauth、ollama LocalFree、新厂商全部 ApiKey。
#[test]
fn auth_kind_classification() {
    for key in ["anthropic", "openai", "xai", "kimi-for-coding"] {
        assert!(matches!(providers::find(key).unwrap().auth, AuthKind::Oauth), "{key} 应为 Oauth");
    }
    assert!(matches!(providers::find("ollama").unwrap().auth, AuthKind::LocalFree));
    for key in ["kimi", "zhipu", "qwen", "minimax", "siliconflow", "doubao"] {
        assert!(matches!(providers::find(key).unwrap().auth, AuthKind::ApiKey), "{key} 应为 ApiKey");
    }
}
