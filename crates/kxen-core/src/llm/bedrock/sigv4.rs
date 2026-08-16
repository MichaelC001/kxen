//! SigV4-HMAC-SHA256 签名（手写最小实现：HMAC 用 ring，SHA-256 用 sha2，均为既有依赖）。
//! 契约对照 AWS 官方文档「Signature Version 4 signing process」与 aws-sig-v4-test-suite 已知答案。

use sha2::Digest;

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default)]
    pub session_token: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0F) as usize] as char);
    }
    out
}

fn sha256_hex(data: &[u8]) -> String {
    hex(&sha2::Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key), data).as_ref().to_vec()
}

/// SigV4 URI 编码：只放行 unreserved（A-Z a-z 0-9 - _ . ~），其余逐字节大写百分号编码。
pub(crate) fn uri_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// 签名输入：调用方自备 canonical 化的 path（已 uri_encode）与头列表（必须含 host / x-amz-date）。
pub(crate) struct SignRequest<'a> {
    pub method: &'a str,
    pub path: &'a str,
    /// 原始 query 键值对（函数内编码 + 排序）
    pub query: &'a [(&'a str, &'a str)],
    /// 参与签名的头（name 小写与否均可，函数内归一）
    pub headers: &'a [(&'a str, &'a str)],
    pub payload: &'a [u8],
    /// "20150830T123600Z"
    pub amz_date: &'a str,
    /// "20150830"
    pub date_stamp: &'a str,
    pub region: &'a str,
    pub service: &'a str,
}

/// 返回 Authorization 头值。
pub(crate) fn sign(credentials: &Credentials, request: &SignRequest) -> String {
    let payload_hash = sha256_hex(request.payload);

    let mut query: Vec<String> = request.query.iter().map(|(k, v)| format!("{}={}", uri_encode(k), uri_encode(v))).collect();
    query.sort();
    let canonical_query = query.join("&");

    // 头归一：name 小写、value 压缩空白；同名合并（SigV4 规范逗号连接）
    let mut headers: Vec<(String, String)> = request
        .headers
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    headers.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let canonical_headers: String = headers.iter().map(|(name, value)| format!("{name}:{value}\n")).collect();
    let signed_headers: String = headers.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(";");

    let canonical_request =
        format!("{}\n{}\n{}\n{}\n{}\n{}", request.method, request.path, canonical_query, canonical_headers, signed_headers, payload_hash);
    let scope = format!("{}/{}/{}/aws4_request", request.date_stamp, request.region, request.service);
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{}\n{}\n{}", request.amz_date, scope, sha256_hex(canonical_request.as_bytes()));

    let k_date = hmac_sha256(format!("AWS4{}", credentials.secret_access_key).as_bytes(), request.date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, request.region.as_bytes());
    let k_service = hmac_sha256(&k_region, request.service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        credentials.access_key_id, scope, signed_headers, signature
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AWS 官方测试套件 get-vanilla-query-order-key-case 已知答案。
    /// <https://docs.aws.amazon.com/general/latest/gr/sigv4-signed-request-examples.html>
    #[test]
    fn aws_documented_get_vector_matches() {
        let credentials = Credentials {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: None,
            region: None,
        };
        let request = SignRequest {
            method: "GET",
            path: "/",
            query: &[("Action", "ListUsers"), ("Version", "2010-05-08")],
            headers: &[
                ("content-type", "application/x-www-form-urlencoded; charset=utf-8"),
                ("host", "iam.amazonaws.com"),
                ("x-amz-date", "20150830T123600Z"),
            ],
            payload: b"",
            amz_date: "20150830T123600Z",
            date_stamp: "20150830",
            region: "us-east-1",
            service: "iam",
        };
        assert_eq!(
            sign(&credentials, &request),
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, \
             SignedHeaders=content-type;host;x-amz-date, \
             Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn uri_encode_only_keeps_unreserved() {
        assert_eq!(uri_encode("anthropic.claude-sonnet-4-5-20250929-v1:0"), "anthropic.claude-sonnet-4-5-20250929-v1%3A0");
        assert_eq!(uri_encode("a b+c/d"), "a%20b%2Bc%2Fd");
    }

    #[test]
    fn query_pairs_are_encoded_and_sorted() {
        let credentials = Credentials { access_key_id: "AK".into(), secret_access_key: "SK".into(), session_token: None, region: None };
        // 同一请求 query 乱序输入必须得到同一签名（排序在函数内）
        let base = SignRequest {
            method: "POST",
            path: "/model/m/converse-stream",
            query: &[("b", "2"), ("a", "1 1")],
            headers: &[("host", "h"), ("x-amz-date", "20260101T000000Z")],
            payload: b"{}",
            amz_date: "20260101T000000Z",
            date_stamp: "20260101",
            region: "us-east-1",
            service: "bedrock",
        };
        let first = sign(&credentials, &base);
        let shuffled = SignRequest { query: &[("a", "1 1"), ("b", "2")], ..base };
        assert_eq!(first, sign(&credentials, &shuffled));
    }
}
