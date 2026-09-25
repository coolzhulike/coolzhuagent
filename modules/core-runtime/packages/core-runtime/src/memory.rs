use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryLayer {
    L0,
    L1,
    L2,
    L3,
    L4,
}

impl MemoryLayer {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::L3 => "L3",
            Self::L4 => "L4",
        }
    }

    #[must_use]
    pub const fn prompt_rank(self) -> u8 {
        match self {
            Self::L1 => 0,
            Self::L2 => 1,
            Self::L3 => 2,
            Self::L0 => 3,
            Self::L4 => 4,
        }
    }
}

#[must_use]
pub fn memory_layer_for_kind(kind: Option<&str>) -> MemoryLayer {
    match kind.unwrap_or("note").trim().to_ascii_lowercase().as_str() {
        "person" | "profile" | "preference" => MemoryLayer::L1,
        "task" | "decision" | "tool" | "tool-summary" | "vision" | "computer-use" | "chat-room"
        | "compaction" => MemoryLayer::L2,
        "knowledge" | "code" | "fact" | "semantic" | "experience" | "lesson" | "pattern" => {
            MemoryLayer::L3
        }
        "archive" | "raw" | "attachment" | "chat" | "conversation" | "message" | "transient" => {
            MemoryLayer::L4
        }
        _ => MemoryLayer::L1,
    }
}

#[must_use]
pub fn normalize_memory_layer(layer: Option<&str>, kind: Option<&str>) -> MemoryLayer {
    match layer
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "L0" => MemoryLayer::L0,
        "L1" => MemoryLayer::L1,
        "L2" => MemoryLayer::L2,
        "L3" => MemoryLayer::L3,
        "L4" => MemoryLayer::L4,
        _ => memory_layer_for_kind(kind),
    }
}

pub trait MemoryBeadView {
    fn layer(&self) -> &str;
    fn kind(&self) -> &str;
    fn summary(&self) -> &str;
    fn source(&self) -> &str;
    fn pinned(&self) -> bool;
    fn confidence(&self) -> f32;
    fn created_at(&self) -> u64;
    /// F 时效：记忆有效期（unix ms）；`None` = 永久（默认，现有实现零变化）。
    fn valid_until(&self) -> Option<u64> {
        None
    }
    /// B 衰减：最近被召回时间（unix ms）；默认 = 创建时间（现有实现零变化）。
    fn last_accessed_at(&self) -> u64 {
        self.created_at()
    }
    /// B 强化：被召回命中次数；默认 0（现有实现零变化）。
    fn access_count(&self) -> u32 {
        0
    }
    /// C 冲突消解：实体归并键（如 "user.theme"）；`None` = 不参与实体级 upsert（默认）。
    fn entity_key(&self) -> Option<&str> {
        None
    }
    /// C 冲突消解：状态 active/superseded/expired；默认 "active"（现有实现零变化）。
    fn status(&self) -> &str {
        "active"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBeadsSummary {
    pub total: usize,
    pub explicit: usize,
    pub defaulted: bool,
    pub pinned: usize,
    pub prompt_candidates: usize,
    pub by_layer: BTreeMap<String, usize>,
    pub by_kind: BTreeMap<String, usize>,
    pub max_beads: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBeadQueryOptions {
    pub q: Option<String>,
    pub layer: Option<String>,
    pub kind: Option<String>,
    pub prompt_only: bool,
    pub limit: usize,
}

impl MemoryBeadQueryOptions {
    #[must_use]
    pub fn bounded_limit(&self, default_limit: usize, max_limit: usize) -> usize {
        let limit = if self.limit == 0 {
            default_limit
        } else {
            self.limit
        };
        limit.clamp(1, max_limit.max(1))
    }
}

#[must_use]
pub fn memory_layer_rank(layer: &str) -> u8 {
    normalize_memory_layer(Some(layer), None).prompt_rank()
}

#[must_use]
pub fn memory_bead_signature(layer: &str, kind: &str, source: &str, summary: &str) -> String {
    let normalized_summary = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        layer.trim().to_ascii_uppercase(),
        kind.trim().to_ascii_lowercase(),
        source.trim().to_ascii_lowercase(),
        normalized_summary.to_ascii_lowercase()
    )
}

#[must_use]
pub fn summarize_memory_beads<T>(
    beads: &[T],
    explicit_count: usize,
    defaulted: bool,
    max_beads: usize,
) -> MemoryBeadsSummary
where
    T: MemoryBeadView,
{
    let mut by_layer = BTreeMap::new();
    let mut by_kind = BTreeMap::new();
    for bead in beads {
        *by_layer.entry(bead.layer().to_string()).or_insert(0) += 1;
        *by_kind.entry(bead.kind().to_string()).or_insert(0) += 1;
    }

    MemoryBeadsSummary {
        total: beads.len(),
        explicit: explicit_count,
        defaulted,
        pinned: beads.iter().filter(|bead| bead.pinned()).count(),
        prompt_candidates: beads
            .iter()
            .filter(|bead| !bead.layer().eq_ignore_ascii_case("L4"))
            .count(),
        by_layer,
        by_kind,
        max_beads,
    }
}

fn compare_memory_beads_for_prompt<T>(a: &T, b: &T) -> std::cmp::Ordering
where
    T: MemoryBeadView,
{
    b.pinned()
        .cmp(&a.pinned())
        .then_with(|| memory_layer_rank(a.layer()).cmp(&memory_layer_rank(b.layer())))
        .then_with(|| b.confidence().total_cmp(&a.confidence()))
        .then_with(|| b.created_at().cmp(&a.created_at()))
}

/// C 冲突消解：bead 是否处于 active 状态（大小写不敏感）。
#[must_use]
pub fn is_memory_bead_active<T: MemoryBeadView>(bead: &T) -> bool {
    bead.status().eq_ignore_ascii_case("active")
}

/// C 冲突消解：只保留 active bead（召回默认排除 superseded/expired-status）。
#[must_use]
pub fn filter_active<T: MemoryBeadView + Clone>(beads: &[T]) -> Vec<T> {
    beads
        .iter()
        .filter(|bead| is_memory_bead_active(*bead))
        .cloned()
        .collect()
}

/// C 冲突消解：在现有 active bead 中找与 `new_bead` 同 `entity_key` 的项（应被取代），返回其下标。
/// `new_bead` 无 entity_key → `None`（视为新增，不取代）。规则优先（同 entity_key 即更新）。
#[must_use]
pub fn find_supersede_target<T: MemoryBeadView>(new_bead: &T, existing: &[T]) -> Option<usize> {
    let key = new_bead
        .entity_key()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    existing.iter().position(|bead| {
        is_memory_bead_active(bead)
            && bead
                .entity_key()
                .map(str::trim)
                .is_some_and(|existing_key| existing_key.eq_ignore_ascii_case(key))
    })
}

/// G 写入策略：写入决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryWriteDecision {
    /// 低价值或精确重复 → 不写。
    Skip,
    /// 新增。
    New,
    /// 取代 `existing[idx]`（实体级更新；旧 bead 应置 superseded）。
    Supersede(usize),
}

/// G 写入策略：摘要是否低价值（有意义字符过少 → 视为噪声）。
#[must_use]
pub fn is_low_value_memory(summary: &str) -> bool {
    summary.chars().filter(|c| c.is_alphanumeric()).count() < 2
}

/// G 写入策略：综合「价值过滤 + 实体级取代（C）+ 精确签名去重」给出写入决策。
/// 顺序：低价值 → Skip；同 entity_key 命中 → Supersede；与 active 项精确签名重复 → Skip；否则 New。
#[must_use]
pub fn decide_memory_write<T: MemoryBeadView>(new_bead: &T, existing: &[T]) -> MemoryWriteDecision {
    if is_low_value_memory(new_bead.summary()) {
        return MemoryWriteDecision::Skip;
    }
    if let Some(index) = find_supersede_target(new_bead, existing) {
        return MemoryWriteDecision::Supersede(index);
    }
    let new_signature = memory_bead_signature(
        new_bead.layer(),
        new_bead.kind(),
        new_bead.source(),
        new_bead.summary(),
    );
    let duplicate = existing.iter().any(|bead| {
        is_memory_bead_active(bead)
            && memory_bead_signature(bead.layer(), bead.kind(), bead.source(), bead.summary())
                == new_signature
    });
    if duplicate {
        MemoryWriteDecision::Skip
    } else {
        MemoryWriteDecision::New
    }
}

/// D 巩固：找出与 `target` 语义近重复（hash 嵌入余弦 ≥ `threshold`）的 active 候选下标。
/// 复用于巩固聚类、C 的语义候选补充、E 的相似边构建。
#[must_use]
pub fn find_near_duplicates<T: MemoryBeadView>(
    target: &T,
    candidates: &[T],
    threshold: f32,
) -> Vec<usize> {
    let target_vector =
        crate::semantic::hash_embed(target.summary(), crate::semantic::DEFAULT_EMBED_DIM);
    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| is_memory_bead_active(*candidate))
        .filter(|(_, candidate)| {
            let candidate_vector = crate::semantic::hash_embed(
                candidate.summary(),
                crate::semantic::DEFAULT_EMBED_DIM,
            );
            crate::semantic::cosine_similarity(&target_vector, &candidate_vector) >= threshold
        })
        .map(|(index, _)| index)
        .collect()
}

/// D 巩固晋升：对外部向量做贪心单遍聚簇——以每个未分配点为种子，拉入所有 cosine ≥ `threshold`
/// 的未分配点成一簇；仅返回 size ≥ 2 的近义簇（簇内 index 升序，按种子先后排列）。
/// 向量与 index 一一对应（调用方负责对齐 bead）。空输入或无近义对 → 返回空。
#[must_use]
pub fn cluster_by_similarity(vectors: &[Vec<f32>], threshold: f32) -> Vec<Vec<usize>> {
    let mut assigned = vec![false; vectors.len()];
    let mut clusters = Vec::new();
    for seed in 0..vectors.len() {
        if assigned[seed] {
            continue;
        }
        let mut cluster = vec![seed];
        assigned[seed] = true;
        for other in (seed + 1)..vectors.len() {
            if assigned[other] {
                continue;
            }
            if crate::semantic::cosine_similarity(&vectors[seed], &vectors[other]) >= threshold {
                assigned[other] = true;
                cluster.push(other);
            }
        }
        if cluster.len() >= 2 {
            clusters.push(cluster);
        }
    }
    clusters
}

/// E 关联图：一条相似边（无向，`from < to` 去重）。
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEdge {
    pub from: usize,
    pub to: usize,
    pub weight: f32,
}

/// E 关联图：为 beads 构建语义相似边（cosine ≥ `threshold`，`from < to` 去重，仅 active 参与）。
#[must_use]
pub fn build_similarity_edges<T: MemoryBeadView>(beads: &[T], threshold: f32) -> Vec<MemoryEdge> {
    let vectors: Vec<Option<Vec<f32>>> = beads
        .iter()
        .map(|bead| {
            is_memory_bead_active(bead).then(|| {
                crate::semantic::hash_embed(bead.summary(), crate::semantic::DEFAULT_EMBED_DIM)
            })
        })
        .collect();
    let mut edges = Vec::new();
    for i in 0..beads.len() {
        for j in (i + 1)..beads.len() {
            if let (Some(vector_i), Some(vector_j)) = (&vectors[i], &vectors[j]) {
                let weight = crate::semantic::cosine_similarity(vector_i, vector_j);
                if weight >= threshold {
                    edges.push(MemoryEdge {
                        from: i,
                        to: j,
                        weight,
                    });
                }
            }
        }
    }
    edges
}

/// H eval：用 hash 嵌入按 `query` 对 beads 排序，返回 top-k 下标（cosine 降序）。
/// 召回质量评估的 lexical 基线（真实 embedder 可替换 `hash_embed`）。
#[must_use]
pub fn rank_beads_by_query<T: MemoryBeadView>(beads: &[T], query: &str, k: usize) -> Vec<usize> {
    if k == 0 {
        return Vec::new();
    }
    let query_vector = crate::semantic::hash_embed(query, crate::semantic::DEFAULT_EMBED_DIM);
    let mut scored: Vec<(usize, f32)> = beads
        .iter()
        .enumerate()
        .map(|(index, bead)| {
            let vector =
                crate::semantic::hash_embed(bead.summary(), crate::semantic::DEFAULT_EMBED_DIM);
            (
                index,
                crate::semantic::cosine_similarity(&query_vector, &vector),
            )
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(index, _)| index).collect()
}

/// H eval：recall@k = `expected` 中落入 `predicted` 前 k 的比例（`expected` 空 → 1.0）。
#[must_use]
pub fn recall_at_k(predicted: &[usize], expected: &[usize], k: usize) -> f32 {
    if expected.is_empty() {
        return 1.0;
    }
    let top: std::collections::HashSet<usize> = predicted.iter().take(k).copied().collect();
    let hit = expected.iter().filter(|id| top.contains(id)).count();
    #[allow(clippy::cast_precision_loss)]
    {
        hit as f32 / expected.len() as f32
    }
}

/// B 衰减/强化：综合有效召回分。
/// `base = confidence`；非 pinned 按 `(now - last_accessed)/half_life` 指数衰减（pinned 不衰减）；
/// 叠加 `ln(1 + access_count)` 强化项。用于排序/裁剪让「常用且新鲜」的记忆上浮。
#[must_use]
pub fn effective_recall_score<T: MemoryBeadView>(bead: &T, now: u64, half_life_ms: u64) -> f32 {
    let base = bead.confidence().clamp(0.0, 1.0);
    let decay = if bead.pinned() {
        1.0
    } else {
        let age = now.saturating_sub(bead.last_accessed_at()) as f32;
        let half_life = half_life_ms.max(1) as f32;
        0.5_f32.powf(age / half_life)
    };
    let reinforce = (bead.access_count() as f32).ln_1p() * 0.1;
    base * decay + reinforce
}

/// F 时效：bead 是否已过期（`valid_until` 早于 `now`）。`None` 永不过期。
#[must_use]
pub fn is_memory_bead_expired<T: MemoryBeadView>(bead: &T, now: u64) -> bool {
    bead.valid_until().is_some_and(|until| now > until)
}

/// F 时效：剔除已过期 bead，保留永久与未来有效期。
#[must_use]
pub fn filter_unexpired<T: MemoryBeadView + Clone>(beads: &[T], now: u64) -> Vec<T> {
    beads
        .iter()
        .filter(|bead| !is_memory_bead_expired(*bead, now))
        .cloned()
        .collect()
}

/// F 时效（规则）：按 kind 推导有效期——瞬时类（chat/会话/原始）给 7 天 TTL，知识/事实/偏好等永久（None）。
/// 让旧的瞬时记忆自动过期、不污染召回；无需存储，召回时按 kind + created_at 计算。
#[must_use]
pub fn rule_based_valid_until(kind: &str, created_at: u64) -> Option<u64> {
    const DAY_MS: u64 = 24 * 60 * 60 * 1000;
    match kind.trim().to_ascii_lowercase().as_str() {
        "chat" | "conversation" | "message" | "transient" | "raw" => {
            Some(created_at.saturating_add(7 * DAY_MS))
        }
        _ => None,
    }
}

pub fn select_prompt_memory_beads<T>(beads: &[T], limit: usize) -> Vec<T>
where
    T: MemoryBeadView + Clone,
{
    let mut selected = beads
        .iter()
        .filter(|bead| !bead.layer().eq_ignore_ascii_case("L4"))
        .cloned()
        .collect::<Vec<_>>();
    selected.sort_by(compare_memory_beads_for_prompt);
    selected.truncate(limit);
    selected
}

pub fn query_memory_beads<T>(beads: &[T], options: &MemoryBeadQueryOptions) -> Vec<T>
where
    T: MemoryBeadView + Clone,
{
    let mut selected = beads
        .iter()
        .filter(|bead| {
            let bead = *bead;
            memory_bead_matches_query(bead, options.layer.as_deref(), options.q.as_deref())
                && options
                    .kind
                    .as_deref()
                    .is_none_or(|kind| bead.kind().eq_ignore_ascii_case(kind.trim()))
                && (!options.prompt_only || !bead.layer().eq_ignore_ascii_case("L4"))
        })
        .cloned()
        .collect::<Vec<_>>();
    selected.sort_by(compare_memory_beads_for_prompt);
    // 语义回退：精确 AND 关键词无召回时，用词重叠相似度（>0.5）召回语义相近 bead，避免相关记忆漏召。
    // 精确有结果时不触发，保持原有精确行为（不破坏现有调用方/测试）。
    if selected.is_empty() {
        if let Some(query) = options.q.as_deref().filter(|q| !q.trim().is_empty()) {
            let mut scored: Vec<(f32, T)> = beads
                .iter()
                .filter(|bead| {
                    let bead = *bead;
                    options
                        .layer
                        .as_deref()
                        .is_none_or(|l| bead.layer().eq_ignore_ascii_case(l.trim()))
                        && options
                            .kind
                            .as_deref()
                            .is_none_or(|kind| bead.kind().eq_ignore_ascii_case(kind.trim()))
                        && (!options.prompt_only || !bead.layer().eq_ignore_ascii_case("L4"))
                })
                .filter_map(|bead| {
                    // 精确 AND 无召回时的相似度回退：词重叠（lexical）∪ hash 嵌入余弦（语义底座）。
                    // hash 嵌入是词袋级 lexical；真正同义词级语义需接 /v1/embeddings
                    // （CUR-MEM-ARCH-001 P1.5：同接口替换嵌入器）。
                    let lexical = memory_bead_relevance_score(bead, Some(query));
                    let haystack = format!(
                        "{} {} {} {}",
                        bead.layer(),
                        bead.kind(),
                        bead.summary(),
                        bead.source()
                    );
                    let semantic = crate::semantic::cosine_similarity(
                        &crate::semantic::hash_embed(query, crate::semantic::DEFAULT_EMBED_DIM),
                        &crate::semantic::hash_embed(&haystack, crate::semantic::DEFAULT_EMBED_DIM),
                    );
                    let score = lexical.max(semantic);
                    (lexical > 0.5 || semantic > 0.30).then(|| (score, bead.clone()))
                })
                .collect();
            scored.sort_by(|a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| compare_memory_beads_for_prompt(&a.1, &b.1))
            });
            selected = scored.into_iter().map(|(_, bead)| bead).collect();
        }
    }
    selected.truncate(options.bounded_limit(8, usize::MAX));
    selected
}

pub fn prune_memory_beads_to<T>(beads: &mut Vec<T>, max_beads: usize)
where
    T: MemoryBeadView,
{
    if beads.len() <= max_beads {
        return;
    }
    beads.sort_by(compare_memory_beads_for_prompt);
    beads.truncate(max_beads);
}

#[must_use]
pub fn render_prompt_memory_context<T>(beads: &[T]) -> String
where
    T: MemoryBeadView,
{
    if beads.is_empty() {
        return "- 暂无".to_string();
    }
    beads
        .iter()
        .map(|bead| format!("- [{}] {}: {}", bead.layer(), bead.kind(), bead.summary()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[must_use]
pub fn memory_bead_matches_query<T: MemoryBeadView>(
    bead: &T,
    layer: Option<&str>,
    query: Option<&str>,
) -> bool {
    if layer.is_some_and(|layer| !bead.layer().eq_ignore_ascii_case(layer.trim())) {
        return false;
    }
    let terms = query
        .unwrap_or_default()
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {}",
        bead.layer(),
        bead.kind(),
        bead.summary(),
        bead.source()
    )
    .to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

/// 语义相关度评分（轻量，无 embedding）：query 词在 bead haystack 的命中比例（0.0-1.0）。
/// 用于关键词精确 AND 召回为空时的相似度回退召回（见 query_memory_beads）。
#[must_use]
pub fn memory_bead_relevance_score<T: MemoryBeadView>(bead: &T, query: Option<&str>) -> f32 {
    let terms = query
        .unwrap_or_default()
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return 0.0;
    }
    let haystack = format!(
        "{} {} {} {}",
        bead.layer(),
        bead.kind(),
        bead.summary(),
        bead.source()
    )
    .to_ascii_lowercase();
    let hits = terms.iter().filter(|term| haystack.contains(*term)).count();
    hits as f32 / terms.len() as f32
}

#[cfg(test)]
mod tests {
    use super::{
        build_similarity_edges, cluster_by_similarity, decide_memory_write, effective_recall_score,
        filter_active, filter_unexpired, find_near_duplicates, find_supersede_target,
        is_low_value_memory, is_memory_bead_active, is_memory_bead_expired,
        memory_bead_matches_query, memory_bead_signature, memory_layer_for_kind,
        normalize_memory_layer, prune_memory_beads_to, query_memory_beads, rank_beads_by_query,
        recall_at_k, render_prompt_memory_context, select_prompt_memory_beads,
        summarize_memory_beads, MemoryBeadQueryOptions, MemoryBeadView, MemoryEdge, MemoryLayer,
        MemoryWriteDecision,
    };

    #[derive(Clone)]
    struct WriteBead {
        summary: String,
        entity_key: Option<String>,
        status: String,
    }

    impl WriteBead {
        fn new(summary: &str, entity_key: Option<&str>, status: &str) -> Self {
            Self {
                summary: summary.to_string(),
                entity_key: entity_key.map(str::to_string),
                status: status.to_string(),
            }
        }
    }

    impl MemoryBeadView for WriteBead {
        fn layer(&self) -> &str {
            "L3"
        }
        fn kind(&self) -> &str {
            "fact"
        }
        fn summary(&self) -> &str {
            &self.summary
        }
        fn source(&self) -> &str {
            "test"
        }
        fn pinned(&self) -> bool {
            false
        }
        fn confidence(&self) -> f32 {
            0.5
        }
        fn created_at(&self) -> u64 {
            0
        }
        fn entity_key(&self) -> Option<&str> {
            self.entity_key.as_deref()
        }
        fn status(&self) -> &str {
            &self.status
        }
    }

    #[test]
    fn low_value_memory_detected() {
        assert!(is_low_value_memory(""));
        assert!(is_low_value_memory("   "));
        assert!(is_low_value_memory("."));
        assert!(is_low_value_memory("x"));
        assert!(!is_low_value_memory("ok fine"));
        assert!(!is_low_value_memory("用户偏好深色主题"));
    }

    #[test]
    fn write_decision_skips_low_value() {
        let existing: Vec<WriteBead> = vec![];
        assert_eq!(
            decide_memory_write(&WriteBead::new("", None, "active"), &existing),
            MemoryWriteDecision::Skip
        );
    }

    #[test]
    fn write_decision_supersedes_same_entity() {
        let existing = vec![WriteBead::new(
            "old theme light",
            Some("user.theme"),
            "active",
        )];
        assert_eq!(
            decide_memory_write(
                &WriteBead::new("new theme dark", Some("user.theme"), "active"),
                &existing
            ),
            MemoryWriteDecision::Supersede(0)
        );
    }

    #[test]
    fn write_decision_skips_exact_signature_duplicate() {
        let existing = vec![WriteBead::new("deploy guide details", None, "active")];
        assert_eq!(
            decide_memory_write(
                &WriteBead::new("deploy guide details", None, "active"),
                &existing
            ),
            MemoryWriteDecision::Skip
        );
    }

    #[test]
    fn write_decision_new_for_distinct() {
        let existing = vec![WriteBead::new("unrelated existing fact", None, "active")];
        assert_eq!(
            decide_memory_write(
                &WriteBead::new("brand new distinct fact", None, "active"),
                &existing
            ),
            MemoryWriteDecision::New
        );
    }

    #[test]
    fn near_duplicates_found_above_threshold() {
        let target = WriteBead::new("deploy local model guide", None, "active");
        let candidates = vec![
            WriteBead::new("deploy local model setup", None, "active"),
            WriteBead::new("weather today is sunny", None, "active"),
        ];
        assert_eq!(find_near_duplicates(&target, &candidates, 0.3), vec![0]);
    }

    #[test]
    fn near_duplicates_empty_with_high_threshold() {
        let target = WriteBead::new("deploy local model guide", None, "active");
        let candidates = vec![WriteBead::new("deploy local model setup", None, "active")];
        assert!(find_near_duplicates(&target, &candidates, 0.99).is_empty());
    }

    #[test]
    fn near_duplicates_ignores_non_active() {
        let target = WriteBead::new("deploy local model guide", None, "active");
        let candidates = vec![WriteBead::new(
            "deploy local model setup",
            None,
            "superseded",
        )];
        assert!(find_near_duplicates(&target, &candidates, 0.3).is_empty());
    }

    #[test]
    fn cluster_by_similarity_empty_input_returns_empty() {
        assert!(cluster_by_similarity(&[], 0.5).is_empty());
    }

    #[test]
    fn cluster_by_similarity_groups_close_and_drops_isolated() {
        // 0、1 相同方向（cos=1）→ 同簇；2 正交（cos=0）→ 孤立、不入簇（size<2 丢弃）。
        let vectors = vec![vec![1.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]];
        assert_eq!(cluster_by_similarity(&vectors, 0.9), vec![vec![0, 1]]);
    }

    #[test]
    fn cluster_by_similarity_no_pairs_returns_empty() {
        // 两两正交，无任何 ≥阈值 对 → 无近义簇。
        let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        assert!(cluster_by_similarity(&vectors, 0.9).is_empty());
    }

    #[test]
    fn similarity_edges_connect_similar_active_beads() {
        let beads = vec![
            WriteBead::new("deploy local model guide", None, "active"),
            WriteBead::new("deploy local model setup", None, "active"),
            WriteBead::new("weather is sunny today", None, "active"),
        ];
        let edges = build_similarity_edges(&beads, 0.3);
        assert_eq!(edges.len(), 1);
        assert_eq!((edges[0].from, edges[0].to), (0, 1));
        assert!(edges[0].weight >= 0.3);
    }

    #[test]
    fn similarity_edges_empty_with_high_threshold() {
        let beads = vec![
            WriteBead::new("deploy local model guide", None, "active"),
            WriteBead::new("deploy local model setup", None, "active"),
        ];
        assert!(build_similarity_edges(&beads, 0.99).is_empty());
    }

    #[test]
    fn similarity_edges_skip_non_active() {
        let beads = vec![
            WriteBead::new("deploy local model guide", None, "active"),
            WriteBead::new("deploy local model setup", None, "superseded"),
        ];
        assert!(build_similarity_edges(&beads, 0.3).is_empty());
    }

    #[test]
    fn recall_at_k_metric() {
        assert!((recall_at_k(&[0, 1, 2], &[1], 3) - 1.0).abs() < 1e-6);
        assert!(recall_at_k(&[0, 1, 2], &[5], 3).abs() < 1e-6);
        assert!((recall_at_k(&[0, 1, 2], &[0, 9], 3) - 0.5).abs() < 1e-6);
        assert!((recall_at_k(&[0, 1, 2], &[], 3) - 1.0).abs() < 1e-6); // 空 expected → 1.0
        assert!(recall_at_k(&[0, 1, 2], &[0], 0).abs() < 1e-6); // k=0 → top 空 → 0
    }

    #[test]
    fn rank_beads_query_ranks_relevant_first() {
        let beads = vec![
            WriteBead::new("weather sunny today", None, "active"),
            WriteBead::new("deploy local model ollama", None, "active"),
        ];
        let ranked = rank_beads_by_query(&beads, "deploy local model", 2);
        assert_eq!(ranked[0], 1);
    }

    /// H eval：固定样本上的 recall@3 验收基线（hash lexical 召回，清晰词重叠应满分）。
    #[test]
    fn eval_recall_at_k_on_fixed_sample() {
        let beads = vec![
            WriteBead::new("deploy local model with ollama", None, "active"), // 0
            WriteBead::new("user prefers dark theme ui", None, "active"),     // 1
            WriteBead::new("rust async tokio task spawn", None, "active"),    // 2
            WriteBead::new("local model embedding bge service", None, "active"), // 3
        ];
        let cases: [(&str, Vec<usize>); 3] = [
            ("deploy local model", vec![0]),
            ("dark theme ui", vec![1]),
            ("async tokio task", vec![2]),
        ];
        let mut total = 0.0_f32;
        for (query, expected) in &cases {
            let predicted = rank_beads_by_query(&beads, query, 3);
            total += recall_at_k(&predicted, expected, 3);
        }
        #[allow(clippy::cast_precision_loss)]
        let mean_recall = total / cases.len() as f32;
        assert!(mean_recall >= 0.99, "mean recall@3 = {mean_recall}");
    }

    #[derive(Clone)]
    struct ConflictBead {
        entity_key: Option<String>,
        status: String,
    }

    impl ConflictBead {
        fn new(entity_key: Option<&str>, status: &str) -> Self {
            Self {
                entity_key: entity_key.map(str::to_string),
                status: status.to_string(),
            }
        }
    }

    impl MemoryBeadView for ConflictBead {
        fn layer(&self) -> &str {
            "L1"
        }
        fn kind(&self) -> &str {
            "preference"
        }
        fn summary(&self) -> &str {
            "x"
        }
        fn source(&self) -> &str {
            "test"
        }
        fn pinned(&self) -> bool {
            false
        }
        fn confidence(&self) -> f32 {
            0.5
        }
        fn created_at(&self) -> u64 {
            0
        }
        fn entity_key(&self) -> Option<&str> {
            self.entity_key.as_deref()
        }
        fn status(&self) -> &str {
            &self.status
        }
    }

    #[test]
    fn default_bead_is_active() {
        let bead = TestBead {
            layer: "L1",
            kind: "person",
            summary: "x",
            pinned: false,
            confidence: 0.5,
            created_at: 1,
        };
        assert!(is_memory_bead_active(&bead)); // 默认 status="active"
        assert!(!is_memory_bead_active(&ConflictBead::new(
            None,
            "superseded"
        )));
    }

    #[test]
    fn filter_active_keeps_only_active() {
        let beads = vec![
            ConflictBead::new(Some("a"), "active"),
            ConflictBead::new(Some("b"), "superseded"),
            ConflictBead::new(Some("c"), "active"),
        ];
        assert_eq!(filter_active(&beads).len(), 2);
    }

    #[test]
    fn supersede_target_matches_same_entity_key() {
        let existing = vec![
            ConflictBead::new(Some("user.theme"), "active"),
            ConflictBead::new(Some("user.lang"), "active"),
        ];
        let new_bead = ConflictBead::new(Some("user.theme"), "active");
        assert_eq!(find_supersede_target(&new_bead, &existing), Some(0));
    }

    #[test]
    fn supersede_target_none_without_entity_key() {
        let existing = vec![ConflictBead::new(Some("user.theme"), "active")];
        assert_eq!(
            find_supersede_target(&ConflictBead::new(None, "active"), &existing),
            None
        );
    }

    #[test]
    fn supersede_target_skips_non_active_and_unmatched() {
        let existing = vec![
            ConflictBead::new(Some("user.theme"), "superseded"), // 同 key 但非 active → 跳过
            ConflictBead::new(Some("user.lang"), "active"),      // 不同 key
        ];
        assert_eq!(
            find_supersede_target(&ConflictBead::new(Some("user.theme"), "active"), &existing),
            None
        );
    }

    #[derive(Clone)]
    struct ScoreBead {
        confidence: f32,
        pinned: bool,
        last_accessed: u64,
        access_count: u32,
    }

    impl MemoryBeadView for ScoreBead {
        fn layer(&self) -> &str {
            "L3"
        }
        fn kind(&self) -> &str {
            "fact"
        }
        fn summary(&self) -> &str {
            "x"
        }
        fn source(&self) -> &str {
            "test"
        }
        fn pinned(&self) -> bool {
            self.pinned
        }
        fn confidence(&self) -> f32 {
            self.confidence
        }
        fn created_at(&self) -> u64 {
            0
        }
        fn last_accessed_at(&self) -> u64 {
            self.last_accessed
        }
        fn access_count(&self) -> u32 {
            self.access_count
        }
    }

    const HL: u64 = 1000; // 半衰期 ms（测试用）

    #[test]
    fn score_increases_with_confidence() {
        let high = ScoreBead {
            confidence: 0.9,
            pinned: false,
            last_accessed: 100,
            access_count: 0,
        };
        let low = ScoreBead {
            confidence: 0.3,
            pinned: false,
            last_accessed: 100,
            access_count: 0,
        };
        assert!(effective_recall_score(&high, 100, HL) > effective_recall_score(&low, 100, HL));
    }

    #[test]
    fn score_decays_with_age() {
        let recent = ScoreBead {
            confidence: 0.8,
            pinned: false,
            last_accessed: 100,
            access_count: 0,
        };
        let old = ScoreBead {
            confidence: 0.8,
            pinned: false,
            last_accessed: 100,
            access_count: 0,
        };
        // recent: age=0；old: age=2 个半衰期
        let now_recent = 100;
        let now_old = 100 + 2 * HL;
        assert!(
            effective_recall_score(&recent, now_recent, HL)
                > effective_recall_score(&old, now_old, HL)
        );
    }

    #[test]
    fn score_increases_with_access_count() {
        let hot = ScoreBead {
            confidence: 0.5,
            pinned: false,
            last_accessed: 100,
            access_count: 20,
        };
        let cold = ScoreBead {
            confidence: 0.5,
            pinned: false,
            last_accessed: 100,
            access_count: 0,
        };
        assert!(effective_recall_score(&hot, 100, HL) > effective_recall_score(&cold, 100, HL));
    }

    #[test]
    fn pinned_does_not_decay() {
        let pinned = ScoreBead {
            confidence: 0.5,
            pinned: true,
            last_accessed: 0,
            access_count: 0,
        };
        let unpinned = ScoreBead {
            confidence: 0.5,
            pinned: false,
            last_accessed: 0,
            access_count: 0,
        };
        let now = 10 * HL; // 很久之后
        assert!(
            effective_recall_score(&pinned, now, HL) > effective_recall_score(&unpinned, now, HL)
        );
    }

    #[test]
    fn default_bead_score_is_finite() {
        let bead = TestBead {
            layer: "L3",
            kind: "fact",
            summary: "x",
            pinned: false,
            confidence: 0.5,
            created_at: 0,
        };
        // 默认 last_accessed_at=created_at=0、access_count=0
        let score = effective_recall_score(&bead, 0, HL);
        assert!(score.is_finite());
    }

    #[derive(Clone)]
    struct ExpiringBead {
        valid_until: Option<u64>,
    }

    impl MemoryBeadView for ExpiringBead {
        fn layer(&self) -> &str {
            "L3"
        }
        fn kind(&self) -> &str {
            "fact"
        }
        fn summary(&self) -> &str {
            "fact"
        }
        fn source(&self) -> &str {
            "test"
        }
        fn pinned(&self) -> bool {
            false
        }
        fn confidence(&self) -> f32 {
            0.5
        }
        fn created_at(&self) -> u64 {
            0
        }
        fn valid_until(&self) -> Option<u64> {
            self.valid_until
        }
    }

    #[test]
    fn expired_bead_detected_by_now() {
        // 默认/无有效期：永不过期
        assert!(!is_memory_bead_expired(
            &ExpiringBead { valid_until: None },
            100
        ));
        // 未来有效期：未过期
        assert!(!is_memory_bead_expired(
            &ExpiringBead {
                valid_until: Some(100)
            },
            50
        ));
        // 过去有效期：过期
        assert!(is_memory_bead_expired(
            &ExpiringBead {
                valid_until: Some(50)
            },
            100
        ));
        // 边界 now == valid_until：未过期（> 才算过期）
        assert!(!is_memory_bead_expired(
            &ExpiringBead {
                valid_until: Some(100)
            },
            100
        ));
    }

    #[test]
    fn filter_unexpired_excludes_only_expired() {
        let beads = vec![
            ExpiringBead { valid_until: None },
            ExpiringBead {
                valid_until: Some(200),
            },
            ExpiringBead {
                valid_until: Some(50),
            },
        ];
        let kept = filter_unexpired(&beads, 100);
        assert_eq!(kept.len(), 2);
    }

    /// 现有 TestBead 不实现 valid_until → 用 trait 默认 None → 永不过期（验证不破坏现有行为）。
    #[test]
    fn default_bead_never_expires() {
        let bead = TestBead {
            layer: "L3",
            kind: "fact",
            summary: "x",
            pinned: false,
            confidence: 0.5,
            created_at: 1,
        };
        assert!(!is_memory_bead_expired(&bead, u64::MAX));
    }

    #[test]
    fn rule_based_valid_until_by_kind() {
        const DAY: u64 = 24 * 60 * 60 * 1000;
        assert_eq!(super::rule_based_valid_until("chat", 0), Some(7 * DAY));
        assert_eq!(
            super::rule_based_valid_until("conversation", 1000),
            Some(1000 + 7 * DAY)
        );
        assert_eq!(super::rule_based_valid_until("fact", 0), None);
        assert_eq!(super::rule_based_valid_until("knowledge", 0), None);
        assert_eq!(super::rule_based_valid_until("preference", 0), None);
    }

    #[derive(Clone)]
    struct TestBead {
        layer: &'static str,
        kind: &'static str,
        summary: &'static str,
        pinned: bool,
        confidence: f32,
        created_at: u64,
    }

    impl MemoryBeadView for TestBead {
        fn layer(&self) -> &str {
            self.layer
        }

        fn kind(&self) -> &str {
            self.kind
        }

        fn summary(&self) -> &str {
            self.summary
        }

        fn source(&self) -> &str {
            "test"
        }

        fn pinned(&self) -> bool {
            self.pinned
        }

        fn confidence(&self) -> f32 {
            self.confidence
        }

        fn created_at(&self) -> u64 {
            self.created_at
        }
    }

    #[test]
    fn maps_memory_kind_to_layer() {
        assert_eq!(memory_layer_for_kind(Some("person")), MemoryLayer::L1);
        assert_eq!(memory_layer_for_kind(Some("tool")), MemoryLayer::L2);
        assert_eq!(memory_layer_for_kind(Some("experience")), MemoryLayer::L3);
        assert_eq!(memory_layer_for_kind(Some("semantic")), MemoryLayer::L3);
        assert_eq!(memory_layer_for_kind(Some("chat")), MemoryLayer::L4);
        assert_eq!(memory_layer_for_kind(Some("attachment")), MemoryLayer::L4);
        assert_eq!(
            normalize_memory_layer(Some("l2"), Some("person")),
            MemoryLayer::L2
        );
    }

    #[test]
    fn prompt_selection_skips_archive_and_prefers_pinned() {
        let beads = vec![
            TestBead {
                layer: "L4",
                kind: "archive",
                summary: "raw",
                pinned: true,
                confidence: 1.0,
                created_at: 5,
            },
            TestBead {
                layer: "L2",
                kind: "task",
                summary: "task",
                pinned: false,
                confidence: 0.8,
                created_at: 4,
            },
            TestBead {
                layer: "L1",
                kind: "person",
                summary: "profile",
                pinned: true,
                confidence: 0.5,
                created_at: 3,
            },
        ];
        let selected = select_prompt_memory_beads(&beads, 8);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].summary, "profile");
    }

    #[test]
    fn signature_normalizes_whitespace_and_case() {
        assert_eq!(
            memory_bead_signature("l2", "Tool", "Chat", "safe   click target"),
            memory_bead_signature("L2", "tool", "chat", "safe click target")
        );
    }

    #[test]
    fn summary_counts_layers_and_prompt_candidates() {
        let beads = vec![
            TestBead {
                layer: "L1",
                kind: "person",
                summary: "profile",
                pinned: true,
                confidence: 1.0,
                created_at: 1,
            },
            TestBead {
                layer: "L4",
                kind: "archive",
                summary: "raw",
                pinned: false,
                confidence: 0.1,
                created_at: 2,
            },
        ];
        let summary = summarize_memory_beads(&beads, 2, false, 256);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.pinned, 1);
        assert_eq!(summary.prompt_candidates, 1);
        assert_eq!(summary.by_layer.get("L1"), Some(&1));
        assert_eq!(summary.by_kind.get("archive"), Some(&1));
    }

    #[test]
    fn query_filters_kind_and_prompt_candidates() {
        let beads = vec![
            TestBead {
                layer: "L2",
                kind: "tool",
                summary: "safe click target",
                pinned: false,
                confidence: 0.8,
                created_at: 1,
            },
            TestBead {
                layer: "L4",
                kind: "archive",
                summary: "safe raw log",
                pinned: true,
                confidence: 1.0,
                created_at: 2,
            },
        ];
        let selected = query_memory_beads(
            &beads,
            &MemoryBeadQueryOptions {
                q: Some("safe".to_string()),
                kind: Some("tool".to_string()),
                prompt_only: true,
                limit: 8,
                ..MemoryBeadQueryOptions::default()
            },
        );
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].kind, "tool");
    }

    #[test]
    fn prompt_context_renders_compact_bullets() {
        let beads = vec![TestBead {
            layer: "L2",
            kind: "decision",
            summary: "use shared runtime rules",
            pinned: true,
            confidence: 1.0,
            created_at: 1,
        }];
        let context = render_prompt_memory_context(&beads);
        assert_eq!(context, "- [L2] decision: use shared runtime rules");
    }

    #[test]
    fn prune_keeps_high_value_beads() {
        let mut beads = vec![
            TestBead {
                layer: "L3",
                kind: "task",
                summary: "old low value",
                pinned: false,
                confidence: 0.2,
                created_at: 1,
            },
            TestBead {
                layer: "L1",
                kind: "person",
                summary: "pinned profile",
                pinned: true,
                confidence: 0.3,
                created_at: 2,
            },
            TestBead {
                layer: "L3",
                kind: "knowledge",
                summary: "recent fact",
                pinned: false,
                confidence: 0.9,
                created_at: 3,
            },
        ];
        prune_memory_beads_to(&mut beads, 2);
        assert_eq!(beads.len(), 2);
        assert!(beads.iter().any(|bead| bead.summary == "pinned profile"));
        assert!(beads.iter().any(|bead| bead.summary == "recent fact"));
    }

    #[test]
    fn query_matches_layer_and_terms() {
        let bead = TestBead {
            layer: "L2",
            kind: "tool",
            summary: "safe click target",
            pinned: false,
            confidence: 0.8,
            created_at: 1,
        };
        assert!(memory_bead_matches_query(
            &bead,
            Some("L2"),
            Some("safe target")
        ));
        assert!(!memory_bead_matches_query(&bead, Some("L1"), Some("safe")));
        assert!(!memory_bead_matches_query(
            &bead,
            Some("L2"),
            Some("missing")
        ));
    }

    #[test]
    fn semantic_fallback_recalls_partial_overlap() {
        // 关键词精确 AND（deploy local model offline）失败（缺 deploy/offline），
        // 词重叠仅 0.5（不过 >0.5），但 hash 嵌入余弦 > 0.30 → 经语义回退召回。
        let beads = vec![TestBead {
            layer: "L3",
            kind: "knowledge",
            summary: "local model deployment guide",
            pinned: false,
            confidence: 0.8,
            created_at: 1,
        }];
        let selected = query_memory_beads(
            &beads,
            &MemoryBeadQueryOptions {
                q: Some("deploy local model offline".to_string()),
                limit: 8,
                ..MemoryBeadQueryOptions::default()
            },
        );
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].summary, "local model deployment guide");
    }
}
