//! 只读预览的传输边界：字节范围与 UTF-8 分页；路径权限由调用方解析。
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Response, StatusCode};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

fn byte_range(value: &str, len: u64) -> Option<(u64, u64)> {
    let (left, right) = value.strip_prefix("bytes=")?.split_once('-')?;
    if len == 0 || right.contains(',') {
        return None;
    }
    if left.is_empty() {
        let count = right.parse::<u64>().ok()?;
        return (count > 0).then_some((len.saturating_sub(count), len - 1));
    }
    let start = left.parse::<u64>().ok()?;
    let end = if right.is_empty() {
        len - 1
    } else {
        right.parse::<u64>().ok()?.min(len - 1)
    };
    (start < len && start <= end).then_some((start, end))
}

/// 流式读取同一文件句柄，视频拖动进度条时只发送所需的单一区间。
pub async fn serve_file(path: &Path, mime: &'static str, headers: &HeaderMap) -> Response<Body> {
    serve_verified_file(path, mime, headers, None).await
}

/// 摘要核对与范围读取复用同一文件句柄，避免校验后重新打开另一个对象。
pub async fn serve_verified_file(path: &Path, mime: &'static str, headers: &HeaderMap, digest: Option<&str>) -> Response<Body> {
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return super::plain_response(StatusCode::NOT_FOUND, "preview file not found");
    };
    let Ok(meta) = file.metadata().await else {
        return super::plain_response(StatusCode::NOT_FOUND, "preview file unavailable");
    };
    if !meta.is_file() {
        return super::plain_response(StatusCode::NOT_FOUND, "preview file not found");
    }
    let len = meta.len();
    if let Some(expected) = digest {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            match file.read(&mut chunk).await {
                Ok(0) => break,
                Ok(read) => hasher.update(&chunk[..read]),
                Err(_) => return super::plain_response(StatusCode::CONFLICT, "attachment integrity unavailable"),
            }
        }
        if format!("{:x}", hasher.finalize()) != expected {
            return super::plain_response(StatusCode::CONFLICT, "attachment integrity mismatch");
        }
    }
    let requested = headers.get(header::RANGE);
    let range = requested
        .and_then(|v| v.to_str().ok())
        .and_then(|v| byte_range(v, len));
    if requested.is_some() && range.is_none() {
        let mut response =
            super::plain_response(StatusCode::RANGE_NOT_SATISFIABLE, "invalid byte range");
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes */{len}")).unwrap(),
        );
        return response;
    }
    let (start, count) = range.map(|(s, e)| (s, e - s + 1)).unwrap_or((0, len));
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return super::plain_response(StatusCode::INTERNAL_SERVER_ERROR, "preview seek failed");
    }
    let stream: std::pin::Pin<
        Box<dyn futures_core::Stream<Item = Result<Vec<u8>, std::io::Error>> + Send>,
    > = Box::pin(async_stream::try_stream! {
        let mut remaining = count;
        while remaining > 0 {
            let mut chunk = vec![0u8; remaining.min(64 * 1024) as usize];
            let read = file.read(&mut chunk).await?;
            if read == 0 { Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "preview file changed"))?; }
            chunk.truncate(read);
            remaining -= read as u64;
            yield chunk;
        }
    });
    let body = Body::from_stream(stream);
    let mut response = Response::new(body);
    *response.status_mut() = if range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let out = response.headers_mut();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    out.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&count.to_string()).unwrap(),
    );
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    out.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // 即使用户单独打开 HTML/SVG 附件，也不能继承控制台的脚本或网络权限。
    out.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'; style-src 'unsafe-inline'"),
    );
    if let Some((start, end)) = range {
        out.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{len}")).unwrap(),
        );
    }
    response
}

pub fn text_page(
    bytes: &[u8],
    offset: u64,
    limit: usize,
) -> Result<(String, usize, usize), &'static str> {
    let text = std::str::from_utf8(bytes).map_err(|_| "project file is not valid UTF-8")?;
    let start = (offset.min(bytes.len() as u64) as usize).max(
        if offset == 0 && bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            3
        } else {
            0
        },
    );
    if !text.is_char_boundary(start) {
        return Err("offset must be a UTF-8 character boundary");
    }
    let mut end = start.saturating_add(limit.max(1)).min(bytes.len());
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    // 小于单个字符的页长仍须向前推进，最多多读三字节。
    if end == start && start < bytes.len() {
        end += text[start..].chars().next().unwrap().len_utf8();
    }
    Ok((text[start..end].to_string(), start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chinese_pages_are_lossless_and_always_advance() {
        let input = "中文🙂abc竹林";
        for limit in 1..8 {
            let mut offset = 0;
            let mut result = String::new();
            while offset < input.len() {
                let (page, _, end) = text_page(input.as_bytes(), offset as u64, limit).unwrap();
                assert!(end > offset);
                result.push_str(&page);
                offset = end;
            }
            assert_eq!(input, result);
        }
        assert!(text_page(input.as_bytes(), 1, 20).is_err());
    }
    #[tokio::test]
    async fn video_range_returns_real_slice_and_rejects_invalid_ranges() {
        use http_body_util::BodyExt;
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"0123456789").unwrap();
        let mut headers = HeaderMap::new();
        for (request, expected) in [("bytes=3-5", "345"), ("bytes=-2", "89"), ("bytes=8-", "89")] {
            headers.insert(header::RANGE, HeaderValue::from_static(request));
            let response = serve_file(file.path(), "video/mp4", &headers).await;
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(
                response
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes()
                    .as_ref(),
                expected.as_bytes()
            );
        }
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=99-100"));
        assert_eq!(
            serve_file(file.path(), "video/mp4", &headers)
                .await
                .status(),
            StatusCode::RANGE_NOT_SATISFIABLE
        );
    }
    #[tokio::test]
    async fn verified_range_reads_expected_bytes_and_rejects_changed_object() {
        use http_body_util::BodyExt;
        use sha2::{Digest, Sha256};
        let file = tempfile::NamedTempFile::new().unwrap();
        let original = b"0123456789";
        std::fs::write(file.path(), original).unwrap();
        let digest = format!("{:x}", Sha256::digest(original));
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=3-5"));
        let response = serve_verified_file(file.path(), "video/mp4", &headers, Some(&digest)).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 3-5/10");
        assert_eq!(response.into_body().collect().await.unwrap().to_bytes().as_ref(), b"345");
        // 长度不变也必须校验全对象，不能只按文件名或被请求的范围信任附件。
        std::fs::write(file.path(), b"X123456789").unwrap();
        let response = serve_verified_file(file.path(), "video/mp4", &headers, Some(&digest)).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(response.into_body().collect().await.unwrap().to_bytes().as_ref(), b"attachment integrity mismatch");
        assert_eq!(std::fs::read(file.path()).unwrap(), b"X123456789");
    }

}
