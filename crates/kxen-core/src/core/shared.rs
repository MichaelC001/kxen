//! 共享原语：lock 辅助（poison 取回）与 Arc<str> 别名。

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 取锁；poison 时取回数据（持锁线程 panic 不代表数据损坏，注册表/缓冲类适用）。
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 读锁；poison 时取回（同 lock 的 WHY）。
pub fn read<T>(l: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(|e| e.into_inner())
}

/// 写锁；poison 时取回（同 lock 的 WHY）。
pub fn write<T>(l: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(|e| e.into_inner())
}

/// 共享字符串别名（clone 仅计数，零拷贝共享）。
pub type SharedStr = std::sync::Arc<str>;

/// 可替换文件缓存的元数据身份：仅 `mtime + len` 会漏同尺寸原子 rename；Unix 身份与 ctime 补上。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileStamp {
    exists: bool,
    modified: Option<std::time::SystemTime>,
    created: Option<std::time::SystemTime>,
    len: u64,
    #[cfg(unix)]
    unix_identity: [i128; 4],
}

impl FileStamp {
    pub(crate) fn exists(self) -> bool {
        self.exists
    }
}

pub(crate) fn file_stamp(path: &std::path::Path) -> std::io::Result<FileStamp> {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            #[cfg(unix)]
            let unix_identity = {
                use std::os::unix::fs::MetadataExt;
                [i128::from(metadata.dev()), i128::from(metadata.ino()), i128::from(metadata.ctime()), i128::from(metadata.ctime_nsec())]
            };
            Ok(FileStamp {
                exists: true,
                modified: metadata.modified().ok(),
                created: metadata.created().ok(),
                len: metadata.len(),
                #[cfg(unix)]
                unix_identity,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileStamp {
            exists: false,
            modified: None,
            created: None,
            len: 0,
            #[cfg(unix)]
            unix_identity: [0; 4],
        }),
        Err(error) => Err(error),
    }
}

/// Session 存储、LLM 历史与 provider 投影共享的不可变文本；JSON 仍是普通 string。
#[derive(Clone, Default, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SharedText(std::sync::Arc<str>);

impl SharedText {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::ops::Deref for SharedText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for SharedText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self)
    }
}

impl AsRef<str> for SharedText {
    fn as_ref(&self) -> &str {
        self
    }
}

impl From<String> for SharedText {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl From<&str> for SharedText {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl From<std::sync::Arc<str>> for SharedText {
    fn from(value: std::sync::Arc<str>) -> Self {
        Self(value)
    }
}

impl PartialEq<&str> for SharedText {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<str> for SharedText {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<String> for SharedText {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

/// Unix 毫秒时间戳；时钟异常时回退 0，保证持久化字段不 panic。
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// 小写 hex 编码使用单一预分配 buffer，避免逐字节 `format!` 产生临时 String。
pub fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

/// 把任意 Unicode whitespace run 压成单个 ASCII 空格，不构造临时 `Vec<&str>`。
pub fn normalize_whitespace(input: &str) -> String {
    let mut words = input.split_whitespace();
    let Some(first) = words.next() else { return String::new() };
    let mut output = String::with_capacity(input.len());
    output.push_str(first);
    for word in words {
        output.push(' ');
        output.push_str(word);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::SharedText;

    #[test]
    fn shared_text_clone_reuses_allocation_and_serializes_as_string() {
        let text = SharedText::from("large immutable payload".repeat(256));
        let cloned = text.clone();
        assert!(text.ptr_eq(&cloned));
        let json = serde_json::to_string(&cloned).unwrap();
        assert_eq!(serde_json::from_str::<SharedText>(&json).unwrap(), text);
    }
}
