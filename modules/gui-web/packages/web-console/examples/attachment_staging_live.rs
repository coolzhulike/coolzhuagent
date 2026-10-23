// 使用生产存储模块，避免重复编译巨型控制台测试入口。
#[path = "../src/content_blob_store.rs"]
mod content_blob_store;
fn main() {}
