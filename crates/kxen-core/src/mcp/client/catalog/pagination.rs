use serde::Serialize;
use serde::Serializer as _;
use serde::ser::SerializeMap;
use serde_json::Value;

use super::{LOCAL_PAGE_DEFAULT, LOCAL_PAGE_MAX, PromptInfo, ResourceInfo};

pub(in crate::mcp::client) fn list_resources(resources: &[ResourceInfo], args: &Value) -> Result<String, String> {
    list_page("resources", resources, args)
}

pub(in crate::mcp::client) fn list_prompts(prompts: &[PromptInfo], args: &Value) -> Result<String, String> {
    list_page("prompts", prompts, args)
}

fn list_page<T: Serialize>(collection: &str, items: &[T], args: &Value) -> Result<String, String> {
    let start = match args.get("cursor") {
        Some(cursor) => cursor.as_str().ok_or("cursor must be a string")?.parse::<usize>().map_err(|_| "invalid cursor")?,
        None => 0,
    };
    let limit = match args.get("limit") {
        Some(limit) => limit.as_u64().ok_or("limit must be an integer")? as usize,
        None => LOCAL_PAGE_DEFAULT,
    };
    if !(1..=LOCAL_PAGE_MAX).contains(&limit) {
        return Err(format!("limit must be between 1 and {LOCAL_PAGE_MAX}"));
    }
    if start > items.len() {
        return Err("cursor is outside the catalog".to_string());
    }
    let end = start.saturating_add(limit).min(items.len());
    let mut output = Vec::new();
    {
        let mut serializer = serde_json::Serializer::new(&mut output);
        let mut map = serializer.serialize_map(Some(usize::from(end < items.len()) + 1)).map_err(|error| error.to_string())?;
        map.serialize_entry(collection, &&items[start..end]).map_err(|error| error.to_string())?;
        if end < items.len() {
            map.serialize_entry("nextCursor", &end.to_string()).map_err(|error| error.to_string())?;
        }
        map.end().map_err(|error| error.to_string())?;
    }
    String::from_utf8(output).map_err(|error| error.to_string())
}
