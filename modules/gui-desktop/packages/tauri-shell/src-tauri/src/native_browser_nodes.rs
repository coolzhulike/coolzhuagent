//! 文档与短期节点绑定只存于宿主内存；网页或模型得到的随机引用不是权限。
use std::{collections::VecDeque, sync::{Mutex, OnceLock}, time::{Duration, Instant}};
use native_browser_protocol::{NodeHandle, PanelResource};

// 引用只绑定观察，不授予输入资格。初始验证加规划可超过20秒；保留至默认CU总预算上限。
// 真实节点/文档/命中仍在预检与执行前重检，一次性执行票据继续只有2秒，动作后撤销旧引用。
const NODE_LEASE: Duration = Duration::from_secs(120);
pub(super) use super::native_browser_document::DocumentIdentity;
use super::native_browser_document::{DocumentScope, DocumentSnapshot};
fn random_id() -> Result<String, String> {
    let mut random = [0u8;16];
    getrandom::fill(&mut random).map_err(|_| "native_browser_node_unavailable")?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}
#[derive(Clone, Debug)]
pub(super) struct NodeBinding { pub node_id:String, pub index:usize, pub backend_node:i64, pub role:String, pub name:String, pub scope:DocumentScope }
#[derive(Clone, Debug)]
pub(super) struct NodeCandidate { pub index:usize, pub backend_node:i64, pub role:String, pub name:String, pub scope:DocumentScope }
struct CachedObservation { observation_id:String, expires:Instant, nodes:Vec<NodeBinding> }
#[derive(Default)]
struct NodeCache { resource:Option<PanelResource>, document:Option<DocumentSnapshot>, token:String, scope_ids:Vec<String>, observations:VecDeque<CachedObservation> }
impl NodeCache {
    fn register(&mut self, resource: &PanelResource, document: &DocumentSnapshot,
        observation_id: &str, candidates: &[NodeCandidate]) -> Result<(String,Vec<NodeHandle>),String> {
        if !native_browser_protocol::opaque_id(observation_id) || candidates.len() > 128
            || candidates.iter().any(|node| !document.scopes.contains(&node.scope)) {
            return Err("native_browser_node_unavailable".into());
        }
        if self.resource.as_ref() != Some(resource) || self.document.as_ref() != Some(document) {
            // 身份在同一快照内稳定，与节点索引无关；随机源失败不留下半套新绑定。
            let token=random_id()?;
            let scope_ids=document.scopes.iter().map(|_|random_id()).collect::<Result<Vec<_>,_>>()?;
            self.observations.clear(); self.token=token; self.scope_ids=scope_ids;
            self.resource = Some(resource.clone()); self.document = Some(document.clone());
        }
        if self.token.is_empty() { self.token = random_id()?; }
        let now = Instant::now(); self.observations.retain(|value| value.expires > now);
        // 允许动作前重新采样验证原引用；动作后必须显式撤销全部旧引用。
        while self.observations.len() >= 4 { self.observations.pop_front(); }
        if self.observations.iter().any(|value| value.observation_id == observation_id) { return Err("native_browser_observation_replayed".into()); }
        let nodes = candidates.iter().map(|candidate| Ok(NodeBinding {
            node_id:random_id()?,index:candidate.index,backend_node:candidate.backend_node,role:candidate.role.clone(),name:candidate.name.clone(),scope:candidate.scope.clone(),
        })).collect::<Result<Vec<_>,String>>()?;
        let handles = nodes.iter().map(|node| NodeHandle {index:node.index,node_id:node.node_id.clone(),in_viewport:None,document_viewport:None,
            document_scope_id:if node.role=="RootWebArea" {document.scopes.iter().position(|scope|scope==&node.scope)
                .and_then(|index|self.scope_ids.get(index)).cloned()} else {None}}).collect();
        self.observations.push_back(CachedObservation {observation_id:observation_id.into(),expires:now+NODE_LEASE,nodes});
        Ok((self.token.clone(),handles))
    }
    fn resolve(&self, resource: &PanelResource, document: &DocumentSnapshot, observation_id: &str,
        token: &str, node_id: &str) -> Result<NodeBinding,String> {
        if self.resource.as_ref() != Some(resource) || self.document.as_ref() != Some(document) || self.token != token { return Err("native_browser_document_changed".into()); }
        let observed = self.observations.iter().find(|value| value.observation_id == observation_id && value.expires > Instant::now()).ok_or("native_browser_node_expired")?;
        observed.nodes.iter().find(|node| node.node_id == node_id).cloned().ok_or_else(|| "native_browser_node_unknown".into())
    }
}
fn cache() -> &'static Mutex<NodeCache> {
    static CACHE:OnceLock<Mutex<NodeCache>> = OnceLock::new(); CACHE.get_or_init(|| Mutex::new(NodeCache::default()))
}
pub(super) fn register(resource: &PanelResource, document: &DocumentSnapshot, observation_id: &str,
    candidates: &[NodeCandidate]) -> Result<(String,Vec<NodeHandle>),String> {
    cache().lock().map_err(|_| "native_browser_node_unavailable")?.register(resource, document, observation_id, candidates)
}
pub(super) fn resolve(resource: &PanelResource, document: &DocumentSnapshot, observation_id: &str,
    token: &str, node_id: &str) -> Result<NodeBinding,String> {
    cache().lock().map_err(|_| "native_browser_node_unavailable")?.resolve(resource,document,observation_id,token,node_id)
}
pub(super) fn retire() -> Result<(),String> { cache().lock().map_err(|_| "native_browser_node_unavailable")?.observations.clear(); Ok(()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_identity_survives_ax_reordering_and_retires_with_resource_or_document() {
        let mut cache=NodeCache::default();
        let resource=PanelResource {workspace_path:"workspace".into(),room_id:"room-1".into(),label:"browser-panel-1".into(),generation:1,navigation_revision:1};
        let identity=DocumentIdentity {frame_id:"frame".into(),loader_id:"loader".into(),backend_root:1};
        let scope=DocumentScope::top(identity.clone());
        let document=DocumentSnapshot {top:identity,scopes:vec![scope.clone()],truncated:false};
        let mut candidate=NodeCandidate {index:12,backend_node:1,role:"RootWebArea".into(),name:"文档".into(),scope};
        let (token,first)=cache.register(&resource,&document,&"a".repeat(32),&[candidate.clone()]).unwrap();
        assert!(first[0].document_scope_id.as_deref().is_some_and(native_browser_protocol::opaque_id));
        cache.observations.clear(); // 动作撤销节点引用，不撤销同一文档身份。
        candidate.index=17;
        let (same_token,next)=cache.register(&resource,&document,&"b".repeat(32),&[candidate.clone()]).unwrap();
        assert_eq!(token,same_token); assert_eq!(first[0].document_scope_id,next[0].document_scope_id);
        assert_ne!(first[0].node_id,next[0].node_id);
        let mut replacement=document.clone();replacement.top.loader_id="replacement".into();
        replacement.scopes[0]=DocumentScope::top(replacement.top.clone());candidate.scope=replacement.scopes[0].clone();
        let (_,replaced)=cache.register(&resource,&replacement,&"c".repeat(32),&[candidate.clone()]).unwrap();
        assert_ne!(first[0].document_scope_id,replaced[0].document_scope_id);
        let mut other=resource.clone();other.room_id="room-2".into();
        let (_,moved)=cache.register(&other,&replacement,&"d".repeat(32),&[candidate]).unwrap();
        assert_ne!(replaced[0].document_scope_id,moved[0].document_scope_id);
    }
    #[test]
    fn references_need_same_document_scope_and_live_unconsumed_observation() {
        let mut cache = NodeCache::default();
        let resource = PanelResource {workspace_path:"workspace".into(),room_id:"room-1".into(),label:"browser-panel-1".into(),generation:1,navigation_revision:1};
        let document = DocumentIdentity {frame_id:"frame".into(),loader_id:"loader".into(),backend_root:1};
        let scope = DocumentScope::top(document.clone());
        let document = DocumentSnapshot {top:document,scopes:vec![scope.clone()],truncated:false};
        let observation = "00000000000000000000000000000001";
        let (token,handles) = cache.register(&resource,&document,observation,&[NodeCandidate {index:0,backend_node:9,role:"button".into(),name:"下一页".into(),scope}]).unwrap();
        let id = &handles[0].node_id;
        assert_eq!(cache.resolve(&resource,&document,observation,&token,id).unwrap().backend_node,9);
        // 模拟真实AK的验证与规划等待，不睡眠；普通等待不得让尚未执行的观察引用先过期。
        cache.observations[0].expires -= Duration::from_secs(25);
        assert_eq!(cache.resolve(&resource,&document,observation,&token,id).unwrap().backend_node,9);
        assert!(cache.resolve(&resource,&document,observation,&token,"00000000000000000000000000000000").is_err());
        let mut replacement = document.clone(); replacement.top.loader_id = "replacement".into();
        assert!(cache.resolve(&resource,&replacement,observation,&token,id).is_err());
        let mut other_room = resource.clone(); other_room.room_id = "room-2".into();
        assert!(cache.resolve(&other_room,&document,observation,&token,id).is_err());
        cache.observations[0].expires = Instant::now()-Duration::from_millis(1);
        assert!(cache.resolve(&resource,&document,observation,&token,id).is_err());
        cache.observations.clear(); assert!(cache.resolve(&resource,&document,observation,&token,id).is_err());
        assert!(DocumentIdentity::from_host(&serde_json::json!({}),&serde_json::json!({"root":{"backendNodeId":1,"nodeType":9}})).is_err());
    }
}
