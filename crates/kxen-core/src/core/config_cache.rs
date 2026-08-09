//! 用户 config.toml 的文件身份键控缓存。

use super::config::Config;
use std::path::Path;
use std::sync::Arc;

type CacheEntry = (std::path::PathBuf, crate::core::shared::FileStamp, Arc<Config>);

/// 文件身份键控的 Config 缓存：热路径（prompt 组装 / custom provider 路由）每次全量
/// 读盘解析太贵。所有写入口（set_role/set_limits/...）都是 tmp+rename 覆盖同一文件，
/// stamp 包含 Unix 文件身份，同长度原子替换也会失效。解析失败不缓存（坏配置不静默）。
pub(crate) struct ConfigCache(std::sync::Mutex<Option<CacheEntry>>);

static CACHE: ConfigCache = ConfigCache::new();

impl ConfigCache {
    pub const fn new() -> Self {
        Self(std::sync::Mutex::new(None))
    }

    pub fn get(&self, path: &Path) -> Option<Arc<Config>> {
        match self.get_result(path) {
            Ok(config) => Some(config),
            Err(error) => {
                let fallback = crate::core::shared::lock(&self.0)
                    .as_ref()
                    .filter(|(cached_path, _, _)| cached_path == path)
                    .map(|(_, _, config)| config.clone());
                tracing::error!(%error, using_last_valid = fallback.is_some(), "config reload rejected");
                fallback
            }
        }
    }

    pub fn get_result(&self, path: &Path) -> Result<Arc<Config>, String> {
        let stamp = config_stamp(path)?;
        let mut guard = crate::core::shared::lock(&self.0);
        if let Some((p, cached_stamp, cfg)) = guard.as_ref()
            && p == path
            && *cached_stamp == stamp
        {
            return Ok(cfg.clone());
        }
        let cfg = Arc::new(Config::load(path, None).map_err(|error| error.to_string())?);
        *guard = Some((path.to_path_buf(), stamp, cfg.clone()));
        Ok(cfg)
    }
}

fn config_stamp(path: &Path) -> Result<crate::core::shared::FileStamp, String> {
    let stamp = crate::core::shared::file_stamp(path).map_err(|error| format!("inspect config {}: {error}", path.display()))?;
    if stamp.exists() {
        Ok(stamp)
    } else {
        match std::fs::symlink_metadata(path) {
            Err(link_error) if link_error.kind() == std::io::ErrorKind::NotFound => Ok(stamp),
            Ok(_) => Err(format!("inspect config {}: target not found", path.display())),
            Err(link_error) => Err(format!("inspect config {}: {link_error}", path.display())),
        }
    }
}

pub(crate) fn cached_user_config() -> Option<Arc<Config>> {
    CACHE.get(&super::paths::config_dir().join("config.toml"))
}

pub(crate) fn cached_user_config_result() -> Result<Arc<Config>, String> {
    CACHE.get_result(&super::paths::config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_invalidates_on_mtime_change() {
        let dir = std::env::temp_dir().join(format!("kxen-cfg-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[coding_rules]\nenabled = false\n").expect("write v1");
        let cache = ConfigCache::new();
        assert_eq!(cache.get(&path).map(|c| c.coding_rules.enabled), Some(false));
        // 等 mtime 走一格再重写：内容变化必须触发重读
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, "[coding_rules]\nenabled = true\n").expect("write v2");
        assert_eq!(cache.get(&path).map(|c| c.coding_rules.enabled), Some(true), "mtime 变化必须失效重读");
        // 解析失败不缓存（坏配置不静默）：修好后能再读出
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, "not = [valid").expect("write bad");
        assert_eq!(cache.get(&path).map(|c| c.coding_rules.enabled), Some(true), "坏配置沿用最后一次有效快照");
        let error = cache.get_result(&path).expect_err("checked lookup must preserve the parse error");
        assert!(error.contains(&path.display().to_string()), "error must identify the invalid config: {error}");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, "[coding_rules]\nenabled = true\n").expect("repair config");
        assert_eq!(cache.get(&path).map(|c| c.coding_rules.enabled), Some(true), "坏配置不得污染后续 cache");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn cache_invalidates_on_same_length_atomic_replacement() {
        let dir = std::env::temp_dir().join(format!("kxen-cfg-cache-replace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("config.toml");
        let replacement = dir.join("replacement.toml");
        std::fs::write(&path, "[embedding]\nprovider = 'openai'\n").expect("write v1");
        let cache = ConfigCache::new();
        assert_eq!(cache.get_result(&path).unwrap().embedding.provider, "openai");

        std::fs::write(&replacement, "[embedding]\nprovider = 'ollama'\n").expect("write v2");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), std::fs::metadata(&replacement).unwrap().len());
        std::fs::rename(replacement, &path).expect("atomic replace");

        assert_eq!(cache.get_result(&path).unwrap().embedding.provider, "ollama");
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn cached_missing_config_does_not_hide_a_broken_symlink() {
        let dir = std::env::temp_dir().join(format!("kxen-cfg-cache-broken-link-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("config.toml");
        let cache = ConfigCache::new();
        cache.get_result(&path).expect("missing config uses defaults");
        std::os::unix::fs::symlink(dir.join("missing-target.toml"), &path).expect("create broken symlink");

        let error = cache.get_result(&path).expect_err("broken symlink must not hit the cached missing-config snapshot");
        assert!(error.contains("inspect config"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
