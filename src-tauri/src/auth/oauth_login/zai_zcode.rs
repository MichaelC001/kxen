//! 智谱 Z.AI（ZCode 桌面客户端契约，逆向，无官方承诺）的三阶段换票。契约来源交叉核实：
//! TriDefender/zcode-api src/auth/oauth.ts、Yeachan-Heo/gajae-code glm-zcode.ts、
//! smartlizi/zcode-account-switcher src/oauthCli.js、jlcodes99/cockpit-tools zcode_oauth.rs。
//! 1. broker：POST zcode.z.ai/api/v1/oauth/token {provider:"zai", code, redirect_uri, state}
//!    -> {code:0, data:{token:<ZCode JWT>, zai:{access_token:<上游 Z.AI token>}, user}}
//! 2. z/login：POST api.z.ai/api/auth/z/login {token:<上游 token>} -> {data:{access_token:<业务 token>}}
//! 3. 铸 key：业务 token 作 Bearer，getCustomerInfo 取默认 org/project，复用或创建名为 zcode-api-key
//!    的 API key，再 GET api_keys/copy/{id} 取 secretKey，拼 "{id}.{secretKey}"，落 CredentialKind::Api。

use crate::auth::credential::CredentialKind;
use serde_json::{Value, json};

const ZAI_LOGIN_URL: &str = "https://api.z.ai/api/auth/z/login";
const ZAI_API_BASE: &str = "https://api.z.ai";
/// ZCode 官方客户端自动铸 key 时用的固定名字（host bundle 常量）。
const API_KEY_NAME: &str = "zcode-api-key";

/// 三阶段端点；生产取常量，测试指向 mock。
struct Endpoints {
    broker: String,
    z_login: String,
    api_base: String,
}

/// code_flow 的 ZaiZcode 分支入口：broker_url 即 CodeSpec.token_url。
pub(super) async fn exchange(
    client: &reqwest::Client,
    broker_url: &str,
    code: &str,
    redirect_uri: &str,
    state: &str,
) -> Result<CredentialKind, String> {
    let endpoints = Endpoints { broker: broker_url.to_string(), z_login: ZAI_LOGIN_URL.to_string(), api_base: ZAI_API_BASE.to_string() };
    run(client, &endpoints, code, redirect_uri, state).await
}

async fn run(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    code: &str,
    redirect_uri: &str,
    state: &str,
) -> Result<CredentialKind, String> {
    let broker = post_json(
        client,
        &endpoints.broker,
        &json!({ "provider": "zai", "code": code, "redirect_uri": redirect_uri, "state": state }),
        None,
        "Z.AI broker",
    )
    .await?;
    if let Some(error) = envelope_error(&broker) {
        return Err(format!("Z.AI broker 换票失败：{error}"));
    }
    let upstream = broker
        .pointer("/data/zai/access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or("Z.AI broker 响应缺少 data.zai.access_token")?;

    let login = post_json(client, &endpoints.z_login, &json!({ "token": upstream }), None, "Z.AI z/login").await?;
    if let Some(error) = envelope_error(&login) {
        return Err(format!("Z.AI z/login 失败：{error}"));
    }
    let business = login
        .pointer("/data/access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or("Z.AI z/login 响应缺少 data.access_token")?;

    provision_api_key(client, endpoints, business).await
}

/// 阶段 3：业务 token 下复用或创建 durable API key，返回 "{id}.{secretKey}"。
async fn provision_api_key(client: &reqwest::Client, endpoints: &Endpoints, business: &str) -> Result<CredentialKind, String> {
    let info =
        get_json(client, &format!("{}/api/biz/customer/getCustomerInfo", endpoints.api_base), business, "Z.AI getCustomerInfo").await?;
    let (organization, project) = pick_default_org_project(&info)?;
    let keys_url = format!("{}/api/biz/v1/organization/{organization}/projects/{project}/api_keys", endpoints.api_base);

    let list = get_json(client, &keys_url, business, "Z.AI api_keys.list").await?;
    let existing = list
        .get("data")
        .and_then(Value::as_array)
        .and_then(|entries| entries.iter().find(|entry| entry.get("name").and_then(Value::as_str) == Some(API_KEY_NAME)))
        .and_then(api_key_id);
    let id = match existing {
        Some(id) => id,
        None => {
            let created = post_json(client, &keys_url, &json!({ "name": API_KEY_NAME }), Some(business), "Z.AI api_keys.create").await?;
            let entry = created.get("data").filter(|data| data.is_object()).unwrap_or(&created);
            api_key_id(entry).ok_or("Z.AI api_keys.create 响应缺少 apiKey id")?
        }
    };

    let copy = get_json(client, &format!("{keys_url}/copy/{id}"), business, "Z.AI api_keys.copy").await?;
    let data = copy.get("data").filter(|data| data.is_object()).unwrap_or(&copy);
    let secret = data
        .get("secretKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("Z.AI api_keys.copy 响应缺少 secretKey")?;
    Ok(CredentialKind::Api { key: format!("{id}.{secret}"), region: None })
}

/// z.ai 系信封：code 缺失/null 视为成功；数字 0/200、字符串 "0"/"200" 成功；
/// success=false 强制失败。失败原因取 msg。
fn envelope_error(value: &Value) -> Option<String> {
    let code_ok = match value.get("code") {
        None | Some(Value::Null) => true,
        Some(Value::Number(code)) => code.as_i64().is_some_and(|code| code == 0 || code == 200),
        Some(Value::String(code)) => matches!(code.trim(), "0" | "200"),
        _ => false,
    };
    if code_ok && value.get("success").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    Some(value.get("msg").and_then(Value::as_str).unwrap_or("未知错误").to_string())
}

/// getCustomerInfo 取默认（缺省取首个）organization 与 project。
fn pick_default_org_project(payload: &Value) -> Result<(String, String), String> {
    let root = payload.get("data").filter(|data| data.is_object()).unwrap_or(payload);
    let organizations = root.get("organizations").and_then(Value::as_array).ok_or("Z.AI getCustomerInfo 响应缺少 organizations")?;
    let organization = organizations
        .iter()
        .find(|org| org.get("isDefault").and_then(Value::as_bool) == Some(true))
        .or_else(|| organizations.first())
        .ok_or("Z.AI getCustomerInfo organizations 为空")?;
    let organization_id = organization
        .get("organizationId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or("Z.AI getCustomerInfo 缺少 organizationId")?;
    let projects = organization.get("projects").and_then(Value::as_array).ok_or("Z.AI getCustomerInfo 缺少 projects")?;
    let project = projects
        .iter()
        .find(|proj| proj.get("isDefault").and_then(Value::as_bool) == Some(true))
        .or_else(|| projects.first())
        .ok_or("Z.AI getCustomerInfo projects 为空")?;
    let project_id =
        project.get("projectId").and_then(Value::as_str).filter(|id| !id.is_empty()).ok_or("Z.AI getCustomerInfo 缺少 projectId")?;
    Ok((organization_id.to_string(), project_id.to_string()))
}

/// api_keys 条目的 id 字段：官方响应为 apiKey，兼容 id 别名。
fn api_key_id(entry: &Value) -> Option<String> {
    entry
        .get("apiKey")
        .and_then(Value::as_str)
        .or_else(|| entry.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(String::from)
}

async fn post_json(client: &reqwest::Client, url: &str, body: &Value, bearer: Option<&str>, label: &str) -> Result<Value, String> {
    let mut request = client.post(url).json(body);
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    read_json(request.send().await.map_err(|error| format!("{label} 请求失败: {error}"))?, label).await
}

async fn get_json(client: &reqwest::Client, url: &str, bearer: &str, label: &str) -> Result<Value, String> {
    let response = client.get(url).bearer_auth(bearer).send().await.map_err(|error| format!("{label} 请求失败: {error}"))?;
    read_json(response, label).await
}

async fn read_json(response: reqwest::Response, label: &str) -> Result<Value, String> {
    let status = response.status();
    let value = crate::net_response::json::<Value>(response, crate::net_response::JSON_BODY_LIMIT, label).await?;
    if !status.is_success() {
        let detail = value.get("msg").or_else(|| value.get("error")).or_else(|| value.get("message")).and_then(Value::as_str).unwrap_or("");
        return Err(format!("{label} 失败：http {status} {detail}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
