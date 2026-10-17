//! 诊断输出前统一清除明确的凭据字段、令牌与认证URL，不修改权威业务事实。
const MASK: &str = "[REDACTED]";

pub(crate) fn sensitive_key(key: &str) -> bool {
    let key: String = key.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect();
    ["apikey", "apikeyref", "password", "passwd", "clientsecret", "accesstoken", "refreshtoken", "idtoken", "privatekey", "authorization", "cookie", "credentials"]
        .iter().any(|suffix| key.ends_with(suffix))
        || (key.ends_with("token") && !matches!(key.as_str(), "inputtoken" | "outputtoken" | "cachetoken" | "totaltoken" | "maxtoken"))
        || matches!(key.as_str(), "token" | "secret" | "credential")
}

pub(crate) fn field(key: &str, value: &str) -> String {
    if sensitive_key(key) && !value.is_empty() { MASK.into() } else { text(value) }
}

fn word(byte: u8) -> bool { byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') }
fn end_value(byte: u8) -> bool { byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\'' | b',' | b';' | b'&' | b')' | b'}' | b']' | b'<' | b'>') }

pub(crate) fn text(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        // ASCII边界确保切片不会截断中文UTF-8。
        if !word(bytes[index]) || (index > 0 && word(bytes[index - 1])) { index += 1; continue; }
        let start = index;
        while index < bytes.len() && word(bytes[index]) { index += 1; }
        let key = &value[start..index];
        if sensitive_key(key) {
            let mut cursor = index;
            while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || matches!(bytes[cursor], b'"' | b'\'')) { cursor += 1; }
            if cursor < bytes.len() && matches!(bytes[cursor], b'=' | b':') {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
                let quote = bytes.get(cursor).copied().filter(|b| matches!(b, b'"' | b'\''));
                if quote.is_some() { cursor += 1; }
                let begin = cursor;
                if value[begin..].starts_with(MASK) { cursor += MASK.len(); }
                let header = key.eq_ignore_ascii_case("authorization") || key.eq_ignore_ascii_case("cookie") || key.eq_ignore_ascii_case("proxy-authorization") || key.eq_ignore_ascii_case("set-cookie");
                while cursor < bytes.len() {
                    if let Some(quote) = quote {
                        if bytes[cursor] == quote && (cursor == 0 || bytes[cursor - 1] != b'\\') { break; }
                    } else if if header { matches!(bytes[cursor], b'\r' | b'\n') } else { end_value(bytes[cursor]) } { break; }
                    cursor += 1;
                }
                if begin < cursor { ranges.push((begin, cursor)); }
            }
        }
        // 错误文本中的独立Bearer令牌也须清除，而非只保护结构化Authorization字段。
        if key.eq_ignore_ascii_case("bearer") && bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            let mut cursor = index;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
            let begin = cursor;
            if value[begin..].starts_with(MASK) { cursor += MASK.len(); }
            while cursor < bytes.len() && !end_value(bytes[cursor]) { cursor += 1; }
            if begin < cursor { ranges.push((begin, cursor)); }
        }
        let tail = &value[start..];
        if ["sk-", "hf_", "ghp_", "gho_", "github_pat_", "dpapi:v1:"].iter().any(|prefix| tail.starts_with(prefix)) {
            let mut end = start;
            while end < bytes.len() && !end_value(bytes[end]) { end += 1; }
            ranges.push((start, end));
        }
        if tail.starts_with("eyJ") {
            let mut end = start;
            while end < bytes.len() && (word(bytes[end]) || bytes[end] == b'.') { end += 1; }
            let parts = value[start..end].split('.').collect::<Vec<_>>();
            if parts.len() == 3 && parts.iter().all(|part| part.len() >= 4) { ranges.push((start, end)); }
        }
        if tail.starts_with("http://") || tail.starts_with("https://") {
            let begin = start + if tail.starts_with("https://") { 8 } else { 7 };
            let mut end = begin;
            while end < bytes.len() && !end_value(bytes[end]) && !matches!(bytes[end], b'/' | b'?' | b'#') { end += 1; }
            if let Some(at) = value[begin..end].rfind('@') { ranges.push((begin, begin + at)); }
        }
    }
    if ranges.is_empty() { return value.to_string(); }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 { last.1 = last.1.max(end); continue; }
        }
        merged.push((start, end));
    }
    let mut result = String::new(); let mut cursor = 0;
    for (start, end) in merged { result.push_str(&value[cursor..start]); result.push_str(MASK); cursor = end; }
    result.push_str(&value[cursor..]); result
}

#[cfg(test)]
mod tests {
    #[test]
    fn structured_credentials_do_not_erase_usage_or_identity() {
        for key in ["api_key", "provider_api_key_ref", "access_token", "HF_TOKEN", "claim_token", "password", "Authorization", "set-cookie"] {
            assert_eq!(super::field(key, "test-secret"), "[REDACTED]");
        }
        for key in ["input_tokens", "output_tokens", "usage_known_mask", "run_id", "token_count"] {
            assert_eq!(super::field(key, "123"), "123");
        }
    }
    #[test]
    fn text_removes_json_query_cli_headers_and_protected_references() {
        let text = "中文错误 {\"api_key\":\"secret with spaces\"} --password='私密值' https://user:pass@example.test/x?access_token=abc&limit=3 Bearer opaque-token\nAuthorization: Bearer abc def\nCookie: session=secret; other=private\nref=dpapi:v1:encrypted sk-test-secret hf_test_secret github_pat_test_secret";
        let clean = super::text(text);
        for secret in ["secret with spaces", "私密值", "user:pass", "access_token=abc", "opaque-token", "abc def", "session=secret", "other=private", "encrypted", "sk-test-secret", "hf_test_secret", "github_pat_test_secret"] {
            assert!(!clean.contains(secret), "未清除测试凭据: {secret}");
        }
        assert!(clean.contains("中文错误")); assert!(clean.contains("limit=3"));
        assert_eq!(super::text(&clean), clean, "多输出边界重复脱敏不得改变结果");
    }
    #[test]
    fn safe_unicode_diagnostics_stay_unchanged() {
        let text = "房间run-123：input_tokens=0, output_tokens=未知；C:/用户/文件.txt";
        assert_eq!(super::text(text), text);
    }
    #[test]
    fn standalone_jwt_is_not_exposed_as_an_error_message() {
        assert_eq!(super::text("认证失败 eyJ0ZXN0.eyJ0ZXN0.c2lnbmF0dXJl，请重试"), "认证失败 [REDACTED]，请重试");
    }
}
