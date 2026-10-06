//! 单一文档内可操作节点的范围；开放 Shadow Root 不继承宿主控件的点击资格。
use serde_json::Value;
use std::collections::HashSet;

pub(super) const NODE_LIMIT: usize = 2048;

pub(super) fn open_shadow_roots(node: &Value) -> impl Iterator<Item = &Value> {
    node["shadowRoots"].as_array().into_iter().flatten().filter(|root|
        root["shadowRootType"].as_str() == Some("open") && root["nodeType"].as_i64() == Some(11))
}

pub(super) fn operable_document_nodes(tree: &Value) -> HashSet<i64> {
    tree.get("root").map(operable_root_nodes).unwrap_or_default()
}

pub(super) fn operable_root_nodes(root: &Value) -> HashSet<i64> {
    let mut found = HashSet::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if found.len() >= NODE_LIMIT { return HashSet::new(); }
        let Some(id) = node["backendNodeId"].as_i64().filter(|id| *id > 0) else { return HashSet::new(); };
        if !found.insert(id) { return HashSet::new(); }
        if let Some(children) = node["children"].as_array() { pending.extend(children); }
        // contentDocument、封闭/UA Shadow Root、伪元素均不提供顶层操作引用。
        pending.extend(open_shadow_roots(node));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_include_open_shadow_children_but_exclude_other_documents_and_closed_roots() {
        let tree = serde_json::json!({"root":{"backendNodeId":1,"children":[{"backendNodeId":2,
            "shadowRoots":[
                {"backendNodeId":3,"nodeType":11,"shadowRootType":"open","children":[{"backendNodeId":4}]},
                {"backendNodeId":5,"nodeType":11,"shadowRootType":"closed","children":[{"backendNodeId":6}]},
                {"backendNodeId":7,"nodeType":11,"shadowRootType":"user-agent","children":[{"backendNodeId":8}]},
                {"backendNodeId":9,"nodeType":1,"shadowRootType":"open"}],
            "contentDocument":{"backendNodeId":10,"children":[{"backendNodeId":11}]},
            "pseudoElements":[{"backendNodeId":12}] }]}});
        assert_eq!(operable_document_nodes(&tree), HashSet::from([1,2,3,4]));
    }

    #[test]
    fn malformed_or_over_budget_trees_do_not_grant_partial_refs() {
        let duplicate = serde_json::json!({"root":{"backendNodeId":1,"children":[{"backendNodeId":1}]}});
        assert!(operable_document_nodes(&duplicate).is_empty());
        let children: Vec<_> = (2..=NODE_LIMIT as i64+1).map(|id|serde_json::json!({"backendNodeId":id})).collect();
        let large = serde_json::json!({"root":{"backendNodeId":1,"children":children}});
        assert!(operable_document_nodes(&large).is_empty());
    }
}
