//! 只缩短模型上下文投影，保留真实原文；失败不丢结果、不猜测工具执行成功。
use std::path::Path;

pub(crate) fn for_context(text: String, frozen_db_path: Option<&Path>, can_read_original: bool) -> String {
    if text.chars().count() <= 8000 { return text; }
    let store = frozen_db_path.and_then(Path::parent).map(|parent| parent.join("tool-results"));
    let saved = store.as_ref().ok_or_else(|| "缺少冻结存储位置".to_string())
        .and_then(|root| crate::content_blob_store::put(root, text.as_bytes(), "txt").map_err(|error| error.to_string()));
    match saved {
        Ok(blob) => {
            // 保存成功不等于模型能够读回；不能以一个未开放工具的指针替代真实结果。
            if !can_read_original {
                return format!("[工具结果已保存原文；SHA256={}，字节数={}。本轮没有可用的原文读取工具，以下保留完整结果，不代表执行成功。]\n{text}",
                    blob.sha256, text.len());
            }
            let read = serde_json::json!({"path":blob.path,"character_offset":6000,"max_chars":4000});
            format!("[工具结果已保存原文；此处仅为上下文节选，不代表执行成功。SHA256={}，字节数={}。继续读取：read_file({read})]\n{}",
                blob.sha256, text.len(), text.chars().take(6000).collect::<String>())
        },
        Err(reason) => format!("[工具原文保存失败，以下保留完整结果：{reason}]\n{text}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn 原文可分段还原且损坏对象不被覆盖() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("sessions.sqlite3");
        let text = "中文𠮷😀\r\n".repeat(2000);
        let projected = super::for_context(text.clone(), Some(&db), true);
        assert!(projected.chars().count() < 8000);
        let blob = crate::content_blob_store::put(&temp.path().join("tool-results"), text.as_bytes(), "txt").unwrap();
        assert_eq!(std::fs::read(&blob.path).unwrap(), text.as_bytes());
        let range = runtime::read_file_character_range(blob.path.to_str().unwrap(), 5999, 4000).unwrap();
        assert_eq!(range.file.content, text.chars().skip(5999).take(4000).collect::<String>());
        assert_eq!(range.version.sha256, blob.sha256);
        std::fs::write(&blob.path, "changed").unwrap();
        let failed = super::for_context(text.clone(), Some(&db), true);
        assert!(failed.ends_with(&text));
        assert_eq!(std::fs::read_to_string(&blob.path).unwrap(), "changed");
    }

    #[test]
    fn 没有原文读取工具时仍保存原文并完整投影() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("sessions.sqlite3");
        let text = "中文𠮷😀\r\n".repeat(2000);
        let projected = super::for_context(text.clone(), Some(&db), false);
        assert!(projected.ends_with(&text));
        assert!(!projected.contains("read_file("));
        let blob = crate::content_blob_store::put(&temp.path().join("tool-results"), text.as_bytes(), "txt").unwrap();
        assert_eq!(std::fs::read(blob.path).unwrap(), text.as_bytes());
        let short = "普通结果".to_string();
        assert_eq!(super::for_context(short.clone(), None, false), short);
        let failed = super::for_context(text.clone(), None, false);
        assert!(failed.contains("工具原文保存失败"));
        assert!(failed.ends_with(&text));
    }
}
