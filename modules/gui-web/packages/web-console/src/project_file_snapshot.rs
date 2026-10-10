//! 工程文件的有界只读快照；路径权限仍由工程 API 解析，编辑资格由调用方决定。
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::Path;

pub struct Snapshot {
    pub metadata: Metadata,
    pub bytes: Vec<u8>,
    pub complete: bool,
}

/// 元信息与读取复用同一文件句柄；增长中的文件也不能突破读取内存上限。
/// 字节摘要只对 complete 的实际读取结果计算，不把截断样本当完整文件版本。
pub fn read(path: &Path, max_bytes: u64) -> io::Result<Snapshot> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Ok(Snapshot {
            metadata,
            bytes: Vec::new(),
            complete: false,
        });
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes).min(64 * 1024) as usize);
    (&mut file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    let complete = bytes.len() as u64 <= max_bytes;
    if !complete {
        bytes = Vec::new();
    }
    Ok(Snapshot {
        metadata: file.metadata()?,
        bytes,
        complete,
    })
}
