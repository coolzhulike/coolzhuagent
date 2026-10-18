//! 证据读取只接受运行/步骤身份，不接受用户提供的磁盘路径。
use super::*;
#[derive(Deserialize)]
pub(super) struct EvidenceQuery {
    call_id: String,
    step: usize,
    phase: String,
}
pub(super) async fn get(Query(input): Query<EvidenceQuery>) -> ApiResult<impl IntoResponse> {
    let _pin =
        workspace_activity::pin_workspace().map_err(|e| api_error(StatusCode::CONFLICT, &e))?;
    let column = match input.phase.as_str() {
        "before" => "before_evidence_ref",
        "after" => "after_evidence_ref",
        _ => return Err(api_error(StatusCode::BAD_REQUEST, "证据阶段无效")),
    };
    let db = default_session_sqlite_path();
    let c = open_session_connection(&db).map_err(sqlite_api_error)?;
    let evidence: Option<String> = c
        .query_row(
            &format!("SELECT {column} FROM computer_use_steps WHERE run_id=?1 AND step_index=?2"),
            params![input.call_id, input.step],
            |r| r.get(0),
        )
        .optional()
        .map_err(sqlite_api_error)?
        .flatten();
    let path = evidence
        .as_deref()
        .and_then(|v| runtime_retention::evidence_paths(v).into_iter().next())
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "此步骤没有图片证据，或记录已超过保留窗口",
            )
        })?;
    let path = path
        .canonicalize()
        .map_err(|_| api_error(StatusCode::NOT_FOUND, "证据文件已不可用"))?;
    let parent = db.parent().unwrap_or(Path::new("."));
    let allowed = [
        parent.join("desktop-capture"),
        parent.join("computer-use"),
        parent.join("screenshots"),
        vision::default_latest_desktop_capture_dir(),
    ];
    if !allowed
        .into_iter()
        .filter_map(|p| p.canonicalize().ok())
        .any(|root| path.starts_with(root))
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "记录中的文件不属于截图证据目录",
        ));
    }
    if std::fs::metadata(&path).map_err(io_api_error)?.len() > 32 * 1024 * 1024 {
        return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE, "证据图片过大"));
    }
    let bytes = std::fs::read(path).map_err(io_api_error)?;
    let mime = match image::guess_format(&bytes).ok() {
        Some(image::ImageFormat::Png) => "image/png",
        Some(image::ImageFormat::Jpeg) => "image/jpeg",
        Some(image::ImageFormat::WebP) => "image/webp",
        _ => return Err(api_error(StatusCode::BAD_REQUEST, "证据不是支持的图片")),
    };
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, mime),
            (axum::http::header::CACHE_CONTROL, "private, no-store"),
        ],
        bytes,
    ))
}
