//! 语义向量索引（记忆架构 A：语义检索的底座）。
//!
//! `SemanticIndex` 抽象「存 id→向量、按余弦相似度召回 top-k」。
//! P1 用 `BruteForceCosineIndex`（暴力余弦，零依赖、离线安全，适合数百~数千条）；
//! 规模上来（L4 归档）后可换 TurboVec 后端，接口不变（见 `CUR-MEM-ARCH-001`）。
//!
//! 约束：一个索引内所有向量必须来自同一 embedding 模型（维度一致、同语义空间）；
//! 维度不一致的项在检索时被跳过，换 embedder 需重建索引。

/// 语义向量索引抽象。
pub trait SemanticIndex {
    /// 插入或更新 `id` 的向量（同 id 覆盖，即 upsert）。
    fn add(&mut self, id: String, vector: Vec<f32>);

    /// 移除 `id`（不存在则无操作）。
    fn remove(&mut self, id: &str);

    /// 按余弦相似度返回与 `query` 最相近的至多 `k` 条 `(id, score)`，按 score 降序。
    /// 维度与 `query` 不一致的项会被跳过；`k == 0` 或空索引返回空。
    #[must_use]
    fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)>;

    /// 当前索引条目数。
    #[must_use]
    fn len(&self) -> usize;

    /// 索引是否为空。
    #[must_use]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 暴力余弦索引：内存线性扫描。向量在入库时 L2 归一化，检索即点积（= 余弦相似度）。
///
/// 适合小规模（L1/L2/L3 的数百~数千 bead）。规模膨胀（L4 归档）后换 TurboVec。
#[derive(Debug, Clone, Default)]
pub struct BruteForceCosineIndex {
    /// `(id, 归一化后的向量)`。
    entries: Vec<(String, Vec<f32>)>,
}

impl BruteForceCosineIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SemanticIndex for BruteForceCosineIndex {
    fn add(&mut self, id: String, vector: Vec<f32>) {
        let normalized = normalize(&vector);
        if let Some(slot) = self
            .entries
            .iter_mut()
            .find(|(existing, _)| existing == &id)
        {
            slot.1 = normalized;
        } else {
            self.entries.push((id, normalized));
        }
    }

    fn remove(&mut self, id: &str) {
        self.entries.retain(|(existing, _)| existing != id);
    }

    fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if k == 0 || self.entries.is_empty() {
            return Vec::new();
        }
        let normalized_query = normalize(query);
        let mut scored = self
            .entries
            .iter()
            .filter(|(_, vector)| vector.len() == normalized_query.len())
            .map(|(id, vector)| (id.clone(), dot(&normalized_query, vector)))
            .collect::<Vec<_>>();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// L2 归一化；零向量原样返回（避免除零）。
fn normalize(vector: &[f32]) -> Vec<f32> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm == 0.0 {
        return vector.to_vec();
    }
    vector.iter().map(|value| value / norm).collect()
}

/// 点积（两侧均已归一化时即余弦相似度）。调用方保证等长。
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// 默认嵌入维度（`hash_embed` 用）。
pub const DEFAULT_EMBED_DIM: usize = 256;

/// 离线确定性嵌入：把文本 token 哈希进 `dim` 维桶计数。
///
/// 纯 CPU、无网络、无依赖、可复现；作为「无 embedding 服务」时的默认/回退。
/// 注意：这是**词袋级 lexical 向量**，不具备同义词级语义；真正语义需接 `/v1/embeddings`
/// （见 `CUR-MEM-ARCH-001` P1.5：以同接口替换本嵌入器）。
#[must_use]
pub fn hash_embed(text: &str, dim: usize) -> Vec<f32> {
    let dim = dim.max(1);
    let mut vector = vec![0.0f32; dim];
    for token in lexical_tokens(text) {
        let bucket = (fnv1a(&token) as usize) % dim;
        vector[bucket] += 1.0;
    }
    vector
}

/// 余弦相似度；零向量或维度不一致返回 0。
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot(a, b) / (norm_a * norm_b)
    }
}

/// 词元化：ASCII 字母数字串（小写）+ CJK 单字 + CJK 相邻二元组。
/// 兼顾英文词与「无空格中文」的 lexical 特征。
fn lexical_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii = String::new();
    let mut prev_cjk: Option<char> = None;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            ascii.push(ch.to_ascii_lowercase());
            prev_cjk = None;
            continue;
        }
        if !ascii.is_empty() {
            tokens.push(std::mem::take(&mut ascii));
        }
        if is_cjk(ch) {
            tokens.push(ch.to_string());
            if let Some(prev) = prev_cjk {
                tokens.push(format!("{prev}{ch}"));
            }
            prev_cjk = Some(ch);
        } else {
            prev_cjk = None;
        }
    }
    if !ascii.is_empty() {
        tokens.push(ascii);
    }
    tokens
}

fn is_cjk(ch: char) -> bool {
    matches!(ch, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7A3}')
}

/// FNV-1a 32-bit：确定性哈希（不用 `DefaultHasher`，避免随机 seed 影响可复现性）。
fn fnv1a(text: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// 向量 → 字节（小端 f32），用于 sqlite BLOB 持久化（`memory_vectors`，P1.5）。
#[must_use]
pub fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// 字节 → 向量；长度非 4 的倍数视为损坏，返回空。
#[must_use]
pub fn decode_vector(bytes: &[u8]) -> Vec<f32> {
    if bytes.len() % 4 != 0 {
        return Vec::new();
    }
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{cosine_similarity, hash_embed, BruteForceCosineIndex, SemanticIndex};

    fn index_with(items: &[(&str, &[f32])]) -> BruteForceCosineIndex {
        let mut index = BruteForceCosineIndex::new();
        for (id, vector) in items {
            index.add((*id).to_string(), vector.to_vec());
        }
        index
    }

    #[test]
    fn search_returns_most_similar_first() {
        let index = index_with(&[("a", &[1.0, 0.0]), ("b", &[0.0, 1.0]), ("c", &[0.9, 0.1])]);
        let hits = index.search(&[1.0, 0.0], 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, "a");
        assert_eq!(hits[1].0, "c");
        assert!(hits[0].1 >= hits[1].1);
    }

    #[test]
    fn cosine_ignores_magnitude() {
        let index = index_with(&[("a", &[2.0, 0.0])]);
        let hits = index.search(&[10.0, 0.0], 1);
        assert_eq!(hits[0].0, "a");
        assert!((hits[0].1 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn add_same_id_upserts() {
        let mut index = index_with(&[("a", &[1.0, 0.0])]);
        index.add("a".to_string(), vec![0.0, 1.0]);
        assert_eq!(index.len(), 1);
        let hits = index.search(&[0.0, 1.0], 1);
        assert_eq!(hits[0].0, "a");
        assert!((hits[0].1 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn remove_drops_entry() {
        let mut index = index_with(&[("a", &[1.0, 0.0]), ("b", &[0.0, 1.0])]);
        index.remove("a");
        assert_eq!(index.len(), 1);
        let hits = index.search(&[1.0, 0.0], 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "b");
    }

    #[test]
    fn k_truncates_and_zero_k_empty() {
        let index = index_with(&[("a", &[1.0, 0.0]), ("b", &[0.9, 0.1]), ("c", &[0.8, 0.2])]);
        assert_eq!(index.search(&[1.0, 0.0], 2).len(), 2);
        assert!(index.search(&[1.0, 0.0], 0).is_empty());
    }

    #[test]
    fn empty_index_returns_empty() {
        let index = BruteForceCosineIndex::new();
        assert!(index.is_empty());
        assert!(index.search(&[1.0, 0.0], 5).is_empty());
    }

    #[test]
    fn mismatched_dimension_skipped() {
        let mut index = index_with(&[("a", &[1.0, 0.0])]);
        index.add("b".to_string(), vec![1.0, 0.0, 0.0]);
        let hits = index.search(&[1.0, 0.0], 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "a");
    }

    #[test]
    fn zero_vector_does_not_panic() {
        let index = index_with(&[("a", &[0.0, 0.0])]);
        let hits = index.search(&[0.0, 0.0], 1);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].1.abs() < 1e-6);
    }

    #[test]
    fn hash_embed_is_deterministic() {
        assert_eq!(hash_embed("create file", 64), hash_embed("create file", 64));
    }

    #[test]
    fn hash_embed_shared_tokens_more_similar() {
        let query = hash_embed("create a test file", 256);
        let related = hash_embed("create test file please", 256);
        let unrelated = hash_embed("weather is nice today", 256);
        assert!(cosine_similarity(&query, &related) > cosine_similarity(&query, &unrelated));
    }

    #[test]
    fn hash_embed_chinese_bigrams() {
        let a = hash_embed("用户偏好中文回复", 256);
        let b = hash_embed("用户偏好中文", 256);
        let c = hash_embed("部署本地模型", 256);
        assert!(cosine_similarity(&a, &b) > cosine_similarity(&a, &c));
    }

    #[test]
    fn cosine_handles_zero_and_mismatch() {
        assert!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]).abs() < 1e-6);
        assert!(cosine_similarity(&[1.0], &[1.0, 2.0]).abs() < 1e-6);
    }

    #[test]
    fn vector_bytes_roundtrip() {
        let vector = vec![0.1_f32, -2.5, 3.0, 0.0, 100.25];
        assert_eq!(super::decode_vector(&super::encode_vector(&vector)), vector);
    }

    #[test]
    fn decode_rejects_corrupt_length() {
        assert!(super::decode_vector(&[1, 2, 3]).is_empty());
    }
}
