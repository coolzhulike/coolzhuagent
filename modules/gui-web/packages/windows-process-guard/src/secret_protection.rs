//! Windows 当前用户 DPAPI：避免凭据明文落盘，不声称防御同一登录用户。
use std::io;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
};

const MAX_SECRET_BYTES: usize = 1024 * 1024;
const DOMAIN: &[u8] = b"coolzhuagent.credentials.v1";

pub fn protect_user_secret(bytes: &[u8]) -> io::Result<Vec<u8>> { transform(bytes, true) }
pub fn unprotect_user_secret(bytes: &[u8]) -> io::Result<Vec<u8>> { transform(bytes, false) }

fn transform(bytes: &[u8], protect: bool) -> io::Result<Vec<u8>> {
    if bytes.len() > MAX_SECRET_BYTES { return Err(io::Error::new(io::ErrorKind::InvalidInput, "凭据数据超过上限")); }
    let input = CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr().cast_mut() };
    let entropy = CRYPT_INTEGER_BLOB { cbData: DOMAIN.len() as u32, pbData: DOMAIN.as_ptr().cast_mut() };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        if protect {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        }
    };
    if ok == 0 { return Err(io::Error::last_os_error()); }
    // API 成功时分配的缓冲区归 LocalFree；解密后的 OS 缓冲先清零。
    let result = if output.cbData as usize > MAX_SECRET_BYTES || output.pbData.is_null() {
        Err(io::Error::new(io::ErrorKind::InvalidData, "系统凭据返回的数据大小无效"))
    } else {
        Ok(unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec())
    };
    unsafe {
        if !output.pbData.is_null() {
            if !protect { std::ptr::write_bytes(output.pbData, 0, output.cbData as usize); }
            LocalFree(output.pbData.cast());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn real_dpapi_roundtrip_and_corruption_rejection() {
        let source = b"test-only-not-a-real-token";
        let mut protected = super::protect_user_secret(source).unwrap();
        assert!(!protected.windows(source.len()).any(|bytes| bytes == source));
        assert_eq!(super::unprotect_user_secret(&protected).unwrap(), source);
        let last = protected.len() - 1; protected[last] ^= 1;
        assert!(super::unprotect_user_secret(&protected).is_err());
    }
}
