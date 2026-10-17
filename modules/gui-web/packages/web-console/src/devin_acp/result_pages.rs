//! 本轮工具回执的只读分页；不接受路径，不执行工具，不跨桥保留读取资格。
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::Mutex};

const PAGE_JSON_BYTES: usize = 6000;

#[derive(Clone)]
struct Receipt { tool: String, path: PathBuf, sha256: String, total_bytes: usize, covered: Vec<(usize, usize)> }

impl Receipt {
    fn cover(&mut self, start: usize, end: usize) {
        if start < end { self.covered.push((start, end)); }
        self.covered.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in self.covered.drain(..) {
            if let Some(last) = merged.last_mut().filter(|last| start <= last.1) { last.1 = last.1.max(end); }
            else { merged.push((start, end)); }
        }
        self.covered = merged;
    }
    fn contiguous_bytes(&self) -> usize {
        self.covered.first().filter(|range| range.0 == 0).map_or(0, |range| range.1)
    }
    fn gaps(&self) -> Vec<(usize, usize)> {
        let mut gaps = Vec::new(); let mut cursor = 0;
        for &(start, end) in &self.covered {
            if cursor < start { gaps.push((cursor, start)); }
            cursor = end;
        }
        if cursor < self.total_bytes { gaps.push((cursor, self.total_bytes)); }
        gaps
    }
}

#[derive(Default)]
pub(super) struct ResultPages { receipts: Mutex<BTreeMap<String, Receipt>> }

impl ResultPages {
    pub(super) fn present(&self, text: String, db: Option<&Path>, tool: &str) -> String {
        if text.len() <= PAGE_JSON_BYTES { return text; }
        let saved = (|| -> Result<String, String> {
            let root = db.and_then(Path::parent).ok_or("缺少冻结存储位置")?.join("tool-results");
            let blob = crate::content_blob_store::put(&root, text.as_bytes(), "txt").map_err(|e|e.to_string())?;
            let id = super::chat::digest(format!("{tool}:{}", blob.sha256).as_bytes());
            let mut receipt = self.receipts.lock().map_err(|_|"回执分页状态不可用")?.get(&id).cloned()
                .unwrap_or(Receipt { tool:tool.into(), path:blob.path, sha256:blob.sha256,
                    total_bytes: text.len(), covered: Vec::new() });
            let page = encode_page(&id, &receipt, &text, 0)?;
            receipt.cover(0, page["next_offset"].as_u64().ok_or("回执页缺少偏移")? as usize);
            // 同一工具的同一原文重发第一页不会撤销已返回的连续前缀。
            self.receipts.lock().map_err(|_|"回执分页状态不可用")?.entry(id).or_insert(receipt);
            Ok(page.to_string())
        })();
        saved.unwrap_or_else(|reason|format!("[工具回执分页准备失败：{reason}。以下保留完整结果；下游仍可能截短，不可据此猜测尾文。]\n{text}"))
    }

    pub(super) fn source_tool(&self, id: &str) -> Result<String,String> {
        Ok(self.receipt(id)?.tool)
    }

    fn receipt(&self, id: &str) -> Result<Receipt,String> {
        self.receipts.lock().map_err(|_|"回执分页状态不可用")?.get(id).cloned()
            .ok_or_else(||"回执不属于本轮已执行工具，不能读取。".into())
    }

    pub(super) fn read(&self, id: &str, offset: usize) -> Result<Value,String> {
        let receipt = self.receipt(id)?;
        if !std::fs::symlink_metadata(&receipt.path).map_err(|_|"回执原文不可用")?.file_type().is_file() {
            return Err("回执原文不能是链接或目录。".into());
        }
        let bytes = std::fs::read(&receipt.path).map_err(|_|"回执原文读取失败")?;
        // 校验实际读入的同一份字节；文件在读取期间被替换也不能返回其它内容。
        if super::chat::digest(&bytes) != receipt.sha256 { return Err("回执原文摘要不符，未返回内容。".into()); }
        let text = std::str::from_utf8(&bytes).map_err(|_|"回执原文不是UTF-8")?;
        let page = encode_page(id, &receipt, text, offset)?;
        let end = page["next_offset"].as_u64().ok_or("回执页缺少偏移")? as usize;
        let mut receipts = self.receipts.lock().map_err(|_|"回执分页状态不可用")?;
        let current = receipts.get_mut(id).ok_or("回执已不可用")?;
        current.cover(offset, end);
        Ok(page)
    }
}

fn encode_page(id: &str, receipt: &Receipt, text: &str, offset: usize) -> Result<Value,String> {
    if offset > text.len() || !text.is_char_boundary(offset) { return Err("回执偏移不是有效UTF-8字节边界。".into()); }
    let mut end = offset.saturating_add(4096).min(text.len());
    loop {
        while !text.is_char_boundary(end) { end -= 1; }
        let mut coverage = receipt.clone(); coverage.cover(offset, end);
        let gaps = coverage.gaps(); let complete = gaps.is_empty();
        let page = json!({"result_id":id,"source_tool":receipt.tool,"sha256":receipt.sha256,
            "offset":offset,"next_offset":end,"total_bytes":text.len(),"complete":complete,"page_at_end":end==text.len(),
            "contiguous_bytes":coverage.contiguous_bytes(),"covered_bytes":coverage.covered.iter().map(|range|range.1-range.0).sum::<usize>(),
            "missing_ranges":gaps.iter().take(8).collect::<Vec<_>>(),"missing_range_count":gaps.len(),
            "resume_offset":if complete {Value::Null}else{json!(coverage.contiguous_bytes())},
            "next_tool":if complete{Value::Null}else{json!("tool_result_read")},
            "message":"这是原工具回执的只读分页，不是新执行。可定位所需范围；page_at_end仅表示此页到达尾部。若任务要求全文，按resume_offset补齐缺口至complete=true；未返回正文不能猜测，不重提原工具。",
            "result_text":&text[offset..end]});
        // 以编码后的页大小限额，控制字符/反斜线不会令包装再次超限。
        if page.to_string().len() <= PAGE_JSON_BYTES { return Ok(page); }
        if end == offset { return Err("分页元数据超过协议预算。".into()); }
        end = offset + (end-offset)/2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 分页保全多字节与转义正文且不接受其它桥的回执() {
        let dir=tempfile::tempdir().unwrap();let db=dir.path().join("sessions.sqlite3");
        let text="中文𠮷😀\\\"\n\u{0001}".repeat(1600);
        let pages=ResultPages::default();let first=pages.present(text.clone(),Some(&db),"actual_tool");
        let mut page:Value=serde_json::from_str(&first).unwrap();let id=page["result_id"].as_str().unwrap().to_owned();
        assert_eq!(pages.source_tool(&id).unwrap(),"actual_tool");
        assert!(ResultPages::default().read(&id,0).is_err());
        assert!(pages.read(&id,1).is_err());
        assert!(pages.read("../other",0).is_err());
        let mut restored=String::new();
        loop {
            assert!(page.to_string().len()<=PAGE_JSON_BYTES);
            restored.push_str(page["result_text"].as_str().unwrap());
            if page["complete"]==true {break;}
            page=pages.read(&id,page["next_offset"].as_u64().unwrap() as usize).unwrap();
        }
        assert_eq!(restored,text);
        let receipt=pages.receipt(&id).unwrap();std::fs::write(receipt.path,"changed").unwrap();
        assert!(pages.read(&id,0).is_err());
    }

    #[test]
    fn 定位尾部保留中段缺口且重读不倒退覆盖进度() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let text = "a".repeat(80_000);
        let pages = ResultPages::default();
        let first: Value = serde_json::from_str(&pages.present(text.clone(), Some(&db), "net_fetch")).unwrap();
        let id = first["result_id"].as_str().unwrap();
        assert_eq!(first["complete"], false);
        let second = pages.read(id, 4096).unwrap();
        assert_eq!(second["next_offset"], 8192);
        let positioned = pages.read(id, 69632).unwrap();
        assert_eq!(positioned["complete"], false);
        assert_eq!(positioned["resume_offset"], 8192);
        let tail = pages.read(id, text.len()-100).unwrap();
        assert_eq!(tail["page_at_end"], true);
        assert_eq!(tail["complete"], false);
        pages.read(id, 0).unwrap();
        assert_eq!(pages.receipt(id).unwrap().contiguous_bytes(), 8192);
        pages.present(text.clone(), Some(&db), "net_fetch");
        assert_eq!(pages.receipt(id).unwrap().contiguous_bytes(), 8192);
        let mut offset = 8192;
        while offset < text.len() {
            let page = pages.read(id, offset).unwrap();
            offset = page["next_offset"].as_u64().unwrap() as usize;
        }
        assert_eq!(pages.receipt(id).unwrap().contiguous_bytes(), text.len());
        assert_eq!(pages.read(id, 0).unwrap()["complete"], true);
    }

    #[test]
    fn 无法保存时明确保留完整结果而短回执不创建对象() {
        let pages=ResultPages::default();
        assert_eq!(pages.present("short".into(),None,"tool"),"short");
        let text="实际资料".repeat(2500);let failed=pages.present(text.clone(),None,"tool");
        assert!(failed.contains("分页准备失败") && failed.ends_with(&text));
        assert!(pages.receipts.lock().unwrap().is_empty());
    }
}
