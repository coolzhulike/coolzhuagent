//! 配置发布只负责文件提交；调用方在成功后更新内存状态。
use std::io::Write;
use std::path::Path;

pub(crate) fn publish(path: &Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("配置文件缺少父目录")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("配置目录不可写：{e}"))?;
    // 同目录临时文件使 persist 使用同卷原子替换。失败时旧文件保持不变。
    let mut draft = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("无法创建配置草稿：{e}"))?;
    draft.write_all(content.as_bytes()).map_err(|e| format!("写入配置失败：{e}"))?;
    draft.as_file().sync_all().map_err(|e| format!("配置同步失败：{e}"))?;
    draft.persist(path).map_err(|e| format!("配置未发布：{}", e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn failed_publication_keeps_previous_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coolzhu.toml");
        super::publish(&path, "name='旧配置'\n").unwrap();
        super::publish(&path, "name='新配置'\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "name='新配置'\n");
        let blocked = dir.path().join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("keep.txt"), "保留").unwrap();
        assert!(super::publish(&blocked, "cannot replace directory").is_err());
        assert_eq!(std::fs::read_to_string(blocked.join("keep.txt")).unwrap(), "保留");
    }
}
