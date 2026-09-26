//! 会话与工作区配置共用的受保护凭据引用。磁盘只存带版本的密文，运行时按需解密。

use base64::{engine::general_purpose::STANDARD, Engine as _};

pub const PREFIX: &str = "dpapi:v1:";

pub fn protect(value: &str) -> Result<String, String> {
    if value.starts_with(PREFIX) || value.is_empty() {
        return Ok(value.to_string());
    }
    #[cfg(windows)]
    {
        let encrypted = windows_process_guard::protect_user_secret(value.as_bytes())
            .map_err(|_| "系统无法保护凭据，原值未写入配置".to_string())?;
        Ok(format!("{PREFIX}{}", STANDARD.encode(encrypted)))
    }
    #[cfg(not(windows))]
    {
        // 本项目的正式发行目标为 Windows。其他平台维持旧行为，避免伪称文件权限等于加密。
        Ok(value.to_string())
    }
}

pub fn reveal(value: &str) -> Result<String, String> {
    let Some(encoded) = value.strip_prefix(PREFIX) else {
        return Ok(value.to_string());
    };
    let bytes = STANDARD.decode(encoded).map_err(|_| "凭据密文格式损坏，请重新输入".to_string())?;
    #[cfg(windows)]
    {
        let plaintext = windows_process_guard::unprotect_user_secret(&bytes)
            .map_err(|_| "当前 Windows 用户无法解锁凭据，请重新输入".to_string())?;
        String::from_utf8(plaintext).map_err(|_| "凭据解码失败，请重新输入".to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        Err("此系统无法解锁 Windows 凭据，请重新输入".to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn protected_reference_roundtrip_and_corruption() {
        let secret = "sk-test-only-0123456789-no-real-key";
        let encoded = super::protect(secret).expect("protect");
        #[cfg(windows)]
        {
            assert!(encoded.starts_with(super::PREFIX));
            assert!(!encoded.contains(secret));
            assert_eq!(super::reveal(&encoded).unwrap(), secret);
            assert!(super::reveal("dpapi:v1:broken").is_err());
            assert_eq!(super::protect(&encoded).unwrap(), encoded);
        }
        #[cfg(not(windows))]
        assert_eq!(encoded, secret);
    }
}
