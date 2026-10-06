//! 宿主读取的文档归属；Frame、owner和所属CDP会话共同区分子文档。
use serde_json::Value;
use super::native_browser_dom::{open_shadow_roots, NODE_LIMIT};

const FRAME_LIMIT: usize = 8;
const FRAME_DEPTH: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DocumentIdentity {
    pub frame_id: String,
    pub loader_id: String,
    pub backend_root: i64,
}
impl DocumentIdentity {
    fn from_frame(frame: &Value, root: &Value) -> Result<Self, String> {
        let text = |key: &str| frame[key].as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
            .map(str::to_owned).ok_or_else(|| "native_browser_document_unavailable".to_string());
        let backend_root = root["backendNodeId"].as_i64().filter(|id| *id > 0)
            .ok_or("native_browser_document_unavailable")?;
        if root["nodeType"].as_i64() != Some(9) { return Err("native_browser_document_unavailable".into()); }
        Ok(Self { frame_id: text("id")?, loader_id: text("loaderId")?, backend_root })
    }
    pub(super) fn from_host(frame: &Value, document: &Value) -> Result<Self, String> {
        Self::from_frame(&frame["frameTree"]["frame"], &document["root"])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrameBinding { pub document: DocumentIdentity, pub owner: Option<i64>, pub session:Option<String> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DocumentScope { pub chain: Vec<FrameBinding> }
impl DocumentScope {
    pub(super) fn top(document: DocumentIdentity) -> Self {
        Self { chain: vec![FrameBinding { document, owner: None, session:None }] }
    }
    pub(super) fn document(&self) -> &DocumentIdentity { &self.chain[self.chain.len() - 1].document }
    pub(super) fn is_top(&self) -> bool { self.chain.len() == 1 }
    pub(super) fn session(&self)->Option<&str> {self.chain.last().and_then(|b|b.session.as_deref())}
}

/// 保留各session的真实DOM，不将跨进程根拼接成虚构的contentDocument。
pub(super) struct DocumentSet {pub top:Value,pub sessions:Vec<super::native_browser_sessions::SessionDocument>}
impl DocumentSet {
    pub(super) fn top(top:Value)->Self {Self {top,sessions:Vec::new()}}
    fn dom(&self,session:Option<&str>)->Result<&Value,String> {
        match session {None=>Ok(&self.top),Some(session)=>self.sessions.iter().find(|d|d.session==session)
            .map(|d|&d.dom).ok_or_else(||"native_browser_document_changed".into())}
    }
}
impl std::ops::Deref for DocumentSet {type Target=Value;fn deref(&self)->&Value {&self.top}}

/// 整页观察令牌覆盖所有已绑定文档，子Frame单独导航也不能沿用旧令牌。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DocumentSnapshot {
    pub top: DocumentIdentity,
    pub scopes: Vec<DocumentScope>,
    pub truncated: bool,
}
#[cfg(test)]
pub(super) fn snapshot_from_host(frame_tree:&Value,dom:&Value)->Result<DocumentSnapshot,String> {
    snapshot_from_documents(frame_tree,&DocumentSet::top(dom.clone()))
}
pub(super) fn snapshot_from_documents(frame_tree: &Value, dom: &DocumentSet) -> Result<DocumentSnapshot, String> {
    let top = DocumentIdentity::from_host(frame_tree, dom)?;
    let mut pending = vec![(&frame_tree["frameTree"], &dom["root"], DocumentScope::top(top.clone()))];
    let mut found = Vec::new();
    let mut frames = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    let mut truncated = false;
    while let Some((tree, root, scope)) = pending.pop() {
        if found.len() >= FRAME_LIMIT { truncated = true; break; }
        if !frames.insert(scope.document().frame_id.clone()) { return Err("native_browser_document_unavailable".into()); }
        let children = tree["childFrames"].as_array().map(Vec::as_slice).unwrap_or_default();
        let mut represented = std::collections::HashSet::new();
        let mut nodes = vec![root];
        while let Some(node) = nodes.pop() {
            if visited.len() >= NODE_LIMIT { truncated = true; break; }
            let Some(id) = node["backendNodeId"].as_i64().filter(|id| *id > 0) else {
                return Err("native_browser_document_unavailable".into());
            };
            if !visited.insert((scope.session().map(str::to_owned),id)) { return Err("native_browser_document_unavailable".into()); }
            if let Some(list) = node["children"].as_array() { nodes.extend(list); }
            nodes.extend(open_shadow_roots(node));
            if !matches!(node["nodeName"].as_str(), Some("IFRAME" | "FRAME")) { continue; }
            let Some(child_id) = node["frameId"].as_str() else { continue; };
            let matching: Vec<_> = children.iter().filter(|c| c["frame"]["id"].as_str() == Some(child_id)).collect();
            let external=dom.sessions.iter().filter(|d|
                d.frames["frameTree"]["frame"]["id"].as_str()==Some(child_id)
                && d.frames["frameTree"]["frame"]["parentId"].as_str()==Some(scope.document().frame_id.as_str())).collect::<Vec<_>>();
            let binding=if let Some(child_root)=node.get("contentDocument") {
                matching.first().filter(|_|matching.len()==1).map(|child|(*child,child_root,scope.session().map(str::to_owned)))
            } else {
                external.first().filter(|_|external.len()==1).map(|child|(&child.frames["frameTree"],&child.dom["root"],Some(child.session.clone())))
            };
            let Some((child,child_root,session))=binding else {truncated=true;continue;};
            if scope.chain.len() >= FRAME_DEPTH
                || child["frame"]["parentId"].as_str() != Some(scope.document().frame_id.as_str()) {
                truncated = true; continue;
            }
            let Ok(document) = DocumentIdentity::from_frame(&child["frame"], child_root) else { truncated = true; continue; };
            let mut chain = scope.chain.clone();
            chain.push(FrameBinding { document, owner: Some(id), session });
            represented.insert(child_id);
            pending.push((child, child_root, DocumentScope { chain }));
        }
        truncated |= children.iter().any(|child| child["frame"]["id"].as_str().is_none_or(|id| !represented.contains(id)));
        found.push(scope);
        if visited.len() >= NODE_LIMIT { truncated |= !pending.is_empty(); break; }
    }
    // 顶层优先，其余按Frame身份稳定排序；DOM/AX遍历顺序变化不能随机改变令牌。
    found.sort_by(|a, b| a.chain.len().cmp(&b.chain.len())
        .then(a.document().frame_id.cmp(&b.document().frame_id)));
    Ok(DocumentSnapshot { top, scopes: found, truncated })
}

/// 在新DOM快照中沿原宿主owner链找到目标根；不把contentDocument当普通children。
pub(super) fn root_for_scope<'a>(dom: &'a DocumentSet, scope: &DocumentScope) -> Result<&'a Value, String> {
    let mut root = dom.get("root").ok_or("native_browser_document_changed")?;
    let mut budget = NODE_LIMIT;
    let mut session=None;
    for binding in &scope.chain {
        if let Some(owner) = binding.owner {
            let mut pending = vec![root];
            let mut found = None;
            while let Some(node) = pending.pop() {
                if budget == 0 { return Err("native_browser_document_changed".into()); }
                budget -= 1;
                if node["backendNodeId"].as_i64() == Some(owner) { found = Some(node); break; }
                if let Some(children) = node["children"].as_array() { pending.extend(children); }
                pending.extend(open_shadow_roots(node));
            }
            let owner_node = found.ok_or("native_browser_document_changed")?;
            if owner_node["frameId"].as_str() != Some(binding.document.frame_id.as_str()) {
                return Err("native_browser_document_changed".into());
            }
            root = if binding.session.as_deref()==session {
                owner_node.get("contentDocument").ok_or("native_browser_document_changed")?
            } else {dom.dom(binding.session.as_deref())?.get("root").ok_or("native_browser_document_changed")?};
        }
        session=binding.session.as_deref();
        if root["nodeType"].as_i64() != Some(9) || root["backendNodeId"].as_i64() != Some(binding.document.backend_root) {
            return Err("native_browser_document_changed".into());
        }
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_frame_requires_actual_parent_and_qualifies_backend_ids_by_session() {
        let tree=serde_json::json!({"frameTree":{"frame":{"id":"top","loaderId":"a"}}});
        let mut dom=DocumentSet {
            top:serde_json::json!({"root":{"backendNodeId":1,"nodeType":9,"children":[
                {"backendNodeId":2,"nodeName":"IFRAME","frameId":"child"}]}}),
            sessions:vec![super::super::native_browser_sessions::SessionDocument {
                session:"private-session".into(),
                frames:serde_json::json!({"frameTree":{"frame":{"id":"child","parentId":"top","loaderId":"b"}}}),
                // 不同进程的backendNodeId可与父进程碰撞，不能因此丢弃子根或借用父控件。
                dom:serde_json::json!({"root":{"backendNodeId":1,"nodeType":9,"children":[
                    {"backendNodeId":2,"nodeName":"BUTTON"}]}}),
            }],
        };
        let snapshot=snapshot_from_documents(&tree,&dom).unwrap();
        assert_eq!(snapshot.scopes.len(),2);
        assert!(!snapshot.truncated);
        assert_eq!(snapshot.scopes[1].session(),Some("private-session"));
        assert_eq!(root_for_scope(&dom,&snapshot.scopes[1]).unwrap(),&dom.sessions[0].dom["root"]);
        dom.sessions[0].frames["frameTree"]["frame"]["loaderId"]="new-loader".into();
        assert_ne!(snapshot_from_documents(&tree,&dom).unwrap(),snapshot);
        dom.sessions[0].frames["frameTree"]["frame"]["parentId"]="other-parent".into();
        let mismatch=snapshot_from_documents(&tree,&dom).unwrap();
        assert_eq!(mismatch.scopes.len(),1);
        assert!(mismatch.truncated);
        dom.top["root"]["children"][0]["backendNodeId"]=3.into();
        assert!(root_for_scope(&dom,&snapshot.scopes[1]).is_err());
    }
    #[test]
    fn child_navigation_and_owner_replacement_change_document_scope() {
        let tree = serde_json::json!({"frameTree":{"frame":{"id":"top","loaderId":"a"},
            "childFrames":[{"frame":{"id":"child","parentId":"top","loaderId":"b"}}]}});
        let mut dom = serde_json::json!({"root":{"backendNodeId":1,"nodeType":9,"children":[
            {"backendNodeId":2,"nodeName":"IFRAME","frameId":"child","contentDocument":
                {"backendNodeId":3,"nodeType":9,"children":[{"backendNodeId":4,"nodeName":"BUTTON"}]}}]}});
        let snapshot = snapshot_from_host(&tree, &dom).unwrap();
        assert_eq!(snapshot.scopes.len(), 2);
        assert!(!snapshot.truncated);
        assert_eq!(root_for_scope(&DocumentSet::top(dom.clone()), &snapshot.scopes[1]).unwrap()["backendNodeId"], 3);
        let mut navigated = tree.clone(); navigated["frameTree"]["childFrames"][0]["frame"]["loaderId"] = "new".into();
        assert_ne!(snapshot_from_host(&navigated, &dom).unwrap(), snapshot);
        dom["root"]["children"][0]["backendNodeId"] = 5.into();
        assert!(root_for_scope(&DocumentSet::top(dom.clone()), &snapshot.scopes[1]).is_err());
        assert_ne!(snapshot_from_host(&tree, &dom).unwrap(), snapshot);
        dom["root"]["children"][0].as_object_mut().unwrap().remove("contentDocument");
        let unsupported = snapshot_from_host(&tree, &dom).unwrap();
        assert_eq!(unsupported.scopes.len(), 1);
        assert!(unsupported.truncated);
    }
}
