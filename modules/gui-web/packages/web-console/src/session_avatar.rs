//! 上传头像必须经过图像解码和尺寸限制；文件名只由内容哈希生成。
use super::*;

#[derive(Deserialize)]
pub(super) struct Upload { content: String }

pub(super) async fn upload(headers: axum::http::HeaderMap, Json(payload): Json<Upload>) -> ApiResult<Json<JsonValue>> {
    let _pin = session_config_service::configuration_request_pin(&headers)?;
    use base64::Engine;
    let encoded = payload.content.strip_prefix("data:image/png;base64,")
        .or_else(|| payload.content.strip_prefix("data:image/webp;base64,"))
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST,"头像仅支持 PNG 或 WebP。"))?;
    if encoded.len()>2_800_000 { return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE,"头像不能超过 2 MB。")); }
    let bytes=base64::engine::general_purpose::STANDARD.decode(encoded)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST,"图片编码无效。"))?;
    if bytes.len()>2*1024*1024 { return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE,"头像不能超过 2 MB。")); }
    let mut reader=image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(io_api_error)?;
    let mut limits=image::Limits::default(); limits.max_image_width=Some(4096); limits.max_image_height=Some(4096); limits.max_alloc=Some(64*1024*1024); reader.limits(limits);
    let image=reader.decode().map_err(|_| api_error(StatusCode::BAD_REQUEST,"图片损坏或尺寸超过 4096×4096。"))?.thumbnail(256,256);
    let mut output=std::io::Cursor::new(Vec::new());
    image.write_to(&mut output,image::ImageFormat::Png).map_err(|_|api_error(StatusCode::BAD_REQUEST,"无法转换头像。"))?;
    let bytes=output.into_inner();
    use sha2::Digest;
    let name=format!("{:x}.png",sha2::Sha256::digest(&bytes));
    let root=default_session_sqlite_path().parent().unwrap_or(Path::new(".")).join("avatars");
    std::fs::create_dir_all(&root).map_err(io_api_error)?;
    std::fs::write(root.join(&name),bytes).map_err(io_api_error)?;
    Ok(Json(json!({"avatar":format!("/api/sessions/avatars/{name}")})))
}

pub(super) async fn file(AxumPath(name): AxumPath<String>) -> ApiResult<impl IntoResponse> {
    if !name.strip_suffix(".png").is_some_and(|stem| stem.len()==64 && stem.bytes().all(|b|b.is_ascii_hexdigit())) {
        return Err(api_error(StatusCode::BAD_REQUEST,"头像名称无效。"));
    }
    let root=default_session_sqlite_path().parent().unwrap_or(Path::new(".")).join("avatars");
    let bytes=std::fs::read(root.join(name)).map_err(|_|api_error(StatusCode::NOT_FOUND,"头像不存在。"))?;
    Ok(([(header::CONTENT_TYPE,"image/png"),(header::CACHE_CONTROL,"private, max-age=86400")],bytes))
}
