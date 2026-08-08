use super::*;

#[test]
fn parse_extracts_registry_providers() {
    let text = r#"{
      "anthropic": {"name": "Anthropic", "models": {"claude-x": {"name": "Claude X", "reasoning": true, "tool_call": true, "attachment": true, "modalities": {"input": ["text", "image"]}, "limit": {"context": 200000, "output": 64000}}}},
      "302ai": {"name": "302.AI", "models": {"foo": {}}},
      "togetherai": {"name": "Together AI", "models": {"m/t": {"name": "T", "limit": {"context": 131072}}}},
      "moonshotai": {"name": "Moonshot AI", "models": {"kimi-k2.5": {"name": "K2.5", "limit": {"context": 262144}}}},
      "zhipuai": {"name": "Zhipu AI", "models": {"glm-4.6": {"name": "GLM-4.6", "limit": {"context": 204800}}}},
      "xai": {"name": "xAI", "models": {"grok-y": {"name": "Grok Y", "limit": {"context": 100000}}}}
    }"#;
    let c = parse_models_dev(text).unwrap();
    assert_eq!(c.len(), 5);
    let ant = c.iter().find(|p| p.provider == "anthropic").unwrap();
    assert_eq!(ant.provider_name, "Anthropic");
    assert_eq!(ant.models[0].name, "Claude X");
    assert!(ant.models[0].reasoning);
    assert_eq!(ant.models[0].context, 200000);
    assert_eq!(ant.models[0].modalities_in, vec!["text", "image"]);
    assert!(!c.iter().any(|p| p.provider == "302ai"), "registry 外不收");
    let tg = c.iter().find(|p| p.provider == "together").expect("models.dev 的 togetherai 映射到 kxen 的 together");
    assert_eq!(tg.provider_name, "Together AI");
    assert_eq!(tg.models[0].context, 131072);
    let kimi = c.iter().find(|p| p.provider == "kimi").expect("moonshotai 映射到 kimi");
    assert_eq!(kimi.models[0].id, "kimi-k2.5");
    let zhipu = c.iter().find(|p| p.provider == "zhipu").expect("zhipuai 映射到 zhipu");
    assert_eq!(zhipu.models[0].id, "glm-4.6");
}

#[test]
fn parse_rejects_unusable_payloads() {
    assert!(parse_models_dev("not-json{").is_none(), "非 JSON 必须回落静态兜底");
    assert!(parse_models_dev(r#"{"302ai": {"name": "302.AI", "models": {"foo": {}}}}"#).is_none(), "registry 全覆盖不到时为空");
}

#[test]
fn static_catalog_covers_registry() {
    let c = static_catalog();
    assert_eq!(c.len(), crate::providers::all().len());
    for p in &c {
        assert!(!p.models.is_empty(), "{} 静态兜底为空", p.provider);
        assert!(p.models.iter().all(|m| !m.name.is_empty() && m.context > 0));
    }
}

#[test]
fn static_catalog_has_openrouter_and_ollama() {
    let c = static_catalog();
    let or = c.iter().find(|p| p.provider == "openrouter").expect("openrouter 入表");
    assert!(or.models.iter().any(|m| m.id.contains('/')), "openrouter 模型 id 带 provider 前缀");
    let ol = c.iter().find(|p| p.provider == "ollama").expect("ollama 入表");
    assert!(ol.models.iter().any(|m| m.id == "llama3.3"));
}

#[test]
fn static_catalog_contains_verify_default_models() {
    // 静态兜底与 registry 对齐：verify 的默认 ping 模型必须在清单内，否则开箱实测必挂
    let c = static_catalog();
    for spec in crate::providers::all() {
        let entry = c.iter().find(|x| x.provider == spec.key).unwrap_or_else(|| panic!("{} 入静态兜底", spec.key));
        assert!(entry.models.iter().any(|m| m.id == spec.default_model), "{} 静态兜底缺 verify 默认模型 {}", spec.key, spec.default_model);
    }
}

#[test]
fn catalog_serves_models_and_caches_in_memory() {
    let first = catalog();
    assert!(!first.is_empty(), "任何兜底路径都必须给出非空目录");
    assert!(first.iter().all(|p| !p.provider.is_empty() && !p.models.is_empty()));
    let second = catalog();
    assert_eq!(second.len(), first.len(), "第二次调用必须命中内存缓存");
}

#[test]
fn refresh_async_without_reactor_is_skipped_silently() {
    // 纯同步上下文没有 reactor：后台刷新必须静默跳过，不 panic 也不阻塞调用方。
    refresh_async();
    assert!(!catalog().is_empty());
}

#[tokio::test]
async fn refresh_async_single_flights_and_never_panics() {
    // 并发/重复触发共用单飞 guard：第二次调用立即早退；网络结果成败都不影响调用方。
    refresh_async();
    refresh_async();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    refresh_async();
    assert!(!catalog().is_empty(), "刷新失败必须保留旧缓存");
}

#[tokio::test]
async fn refresh_async_panic_still_resets_single_flight_flag() {
    // spawned 任务 panic：单飞 flag 必须由 Drop guard 复位，否则后续刷新永久静默跳过。
    REFRESH_PANIC_FOR_TEST.store(true, std::sync::atomic::Ordering::SeqCst);
    refresh_async();
    let flag = REFRESHING.get().expect("flag initialized");
    // 注入标记是进程全局：可能被并发测试的在途 refresh 抢先消费，本任务退化为真实
    // HTTP 请求（CI 网络慢时接近 20s 超时才复位）。轮询窗口必须覆盖该最坏路径，
    // 命中注入的正常路径第一轮即 break，不为快路径付出等待。
    for _ in 0..1500 {
        if !*crate::core::shared::lock(flag) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!*crate::core::shared::lock(flag), "panic 后 flag 必须复位");
    // 复位后再次触发必须重新进入 spawn 分支（flag 同步置 true），而非单飞早退。
    refresh_async();
    assert!(*crate::core::shared::lock(flag), "复位后再次刷新必须能启动");
}

/// 缓存文件重定向到临时路径：进程级 HOME 翻转会改变并发测试的 data_dir() 解析，
/// 只能用 cache_file() 的专用 env 覆盖隔离（与 KXEN_TRUST_FILE 同规约；Once 写序防重复 set）。
fn isolate_cache_file() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let file = std::env::temp_dir().join(format!("kxen-catalog-cache-{}.json", std::process::id()));
        unsafe { std::env::set_var("KXEN_CATALOG_FILE", &file) };
    });
}

#[test]
fn disk_cache_roundtrips_and_parse_failure_is_an_error() {
    isolate_cache_file();
    // 写入 -> 读回 roundtrip（rename 原子替换 + 目录 sync 路径）。
    // 用空 Vec：catalog() 对空缓存走静态兜底，不会把测试数据带进内存缓存污染同进程其他测试。
    let catalog: Vec<ProviderCatalog> = Vec::new();
    write_disk_cache(&catalog).expect("write disk cache");
    let back = read_disk_cache().expect("read disk cache").expect("cache present");
    assert!(back.is_empty(), "空目录必须原样 roundtrip");
    // 损坏缓存：报错回落，不 panic。
    std::fs::write(cache_file(), "not-json{").expect("corrupt cache");
    let error = read_disk_cache().expect_err("corrupt cache must fail");
    assert!(error.contains("parse"), "{error}");
    // 恢复为空目录：catalog() 读到空缓存会回落静态表，同进程内行为与无缓存一致。
    write_disk_cache(&catalog).expect("restore empty cache");
}
