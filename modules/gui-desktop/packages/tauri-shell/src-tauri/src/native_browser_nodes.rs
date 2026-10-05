//! 文档与短期节点绑定只存于宿主内存；网页或模型得到的随机引用不是权限。
use std::{collections::VecDeque, sync::{Mutex, OnceLock}, time::{Duration, Instant}};
use native_browser_protocol::{NodeHandle, PanelResource};

// 引用只绑定观察，不授予输入资格。初始验证加规划可超过20秒；保留至默认CU总预算上限。
// 真实节点/文档/命中仍在预检与执行前重检，一次性执行票据继续只有2秒，动作后撤销旧引用。
const NODE_LEASE: Duration = Duration::from_secs(120);
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DocumentIdentity { pub frame_id:String, pub loader_id:String, pub backend_root:i64 }
impl DocumentIdentity {
    pub(super) fn from_host(frame: &serde_json::Value, document: &serde_json::Value) -> Result<Self, String> {
        let text = |key| frame.pointer(key).and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control))
            .map(str::to_owned).ok_or_else(|| "native_browser_document_unavailable".to_string());
        let backend_root = document.pointer("/root/backendNodeId").and_then(serde_json::Value::as_i64)
            .filter(|id| *id > 0).ok_or("native_browser_document_unavailable")?;
        if document.pointer("/root/nodeType").and_then(serde_json::Value::as_i64) != Some(9) {
            return Err("native_browser_document_unavailable".into());
        }
        Ok(Self {frame_id:text("/frameTree/frame/id")?,loader_id:text("/frameTree/frame/loaderId")?,backend_root})
    }
}
fn random_id() -> Result<String, String> {
    let mut random = [0u8;16];
    getrandom::fill(&mut random).map_err(|_| "native_browser_node_unavailable")?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}
#[derive(Clone, Debug)]
pub(super) struct NodeBinding { pub node_id:String, pub index:usize, pub backend_node:i64, pub role:String, pub name:String }
struct CachedObservation { observation_id:String, expires:Instant, nodes:Vec<NodeBinding> }
#[derive(Default)]
struct NodeCache { resource:Option<PanelResource>, document:Option<DocumentIdentity>, token:String, observations:VecDeque<CachedObservation> }
impl NodeCache {
    fn register(&mut self, resource: &PanelResource, document: &DocumentIdentity,
        observation_id: &str, candidates: &[(usize,i64,String,String)]) -> Result<(String,Vec<NodeHandle>),String> {
        if !native_browser_protocol::opaque_id(observation_id) || candidates.len() > 128 { return Err("native_browser_node_unavailable".into()); }
        if self.resource.as_ref() != Some(resource) || self.document.as_ref() != Some(document) {
            self.observations.clear(); self.token.clear(); self.resource = Some(resource.clone()); self.document = Some(document.clone());
        }
        if self.token.is_empty() { self.token = random_id()?; }
        let now = Instant::now(); self.observations.retain(|value| value.expires > now);
        // 允许动作前重新采样验证原引用；动作后必须显式撤销全部旧引用。
        while self.observations.len() >= 4 { self.observations.pop_front(); }
        if self.observations.iter().any(|value| value.observation_id == observation_id) { return Err("native_browser_observation_replayed".into()); }
        let nodes = candidates.iter().map(|(index,backend_node,role,name)| Ok(NodeBinding {
            node_id:random_id()?,index:*index,backend_node:*backend_node,role:role.clone(),name:name.clone(),
        })).collect::<Result<Vec<_>,String>>()?;
        let handles = nodes.iter().map(|node| NodeHandle {index:node.index,node_id:node.node_id.clone(),in_viewport:None}).collect();
        self.observations.push_back(CachedObservation {observation_id:observation_id.into(),expires:now+NODE_LEASE,nodes});
        Ok((self.token.clone(),handles))
    }
    fn resolve(&self, resource: &PanelResource, document: &DocumentIdentity, observation_id: &str,
        token: &str, node_id: &str) -> Result<NodeBinding,String> {
        if self.resource.as_ref() != Some(resource) || self.document.as_ref() != Some(document) || self.token != token { return Err("native_browser_document_changed".into()); }
        let observed = self.observations.iter().find(|value| value.observation_id == observation_id && value.expires > Instant::now()).ok_or("native_browser_node_expired")?;
        observed.nodes.iter().find(|node| node.node_id == node_id).cloned().ok_or_else(|| "native_browser_node_unknown".into())
    }
}
fn cache() -> &'static Mutex<NodeCache> {
    static CACHE:OnceLock<Mutex<NodeCache>> = OnceLock::new(); CACHE.get_or_init(|| Mutex::new(NodeCache::default()))
}
pub(super) fn register(resource: &PanelResource, document: &DocumentIdentity, observation_id: &str,
    candidates: &[(usize,i64,String,String)]) -> Result<(String,Vec<NodeHandle>),String> {
    cache().lock().map_err(|_| "native_browser_node_unavailable")?.register(resource, document, observation_id, candidates)
}
pub(super) fn resolve(resource: &PanelResource, document: &DocumentIdentity, observation_id: &str,
    token: &str, node_id: &str) -> Result<NodeBinding,String> {
    cache().lock().map_err(|_| "native_browser_node_unavailable")?.resolve(resource,document,observation_id,token,node_id)
}
pub(super) fn retire() -> Result<(),String> { cache().lock().map_err(|_| "native_browser_node_unavailable")?.observations.clear(); Ok(()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_need_same_document_scope_and_live_unconsumed_observation() {
        let mut cache = NodeCache::default();
        let resource = PanelResource {workspace_path:"workspace".into(),room_id:"room-1".into(),label:"browser-panel-1".into(),generation:1,navigation_revision:1};
        let document = DocumentIdentity {frame_id:"frame".into(),loader_id:"loader".into(),backend_root:1};
        let observation = "00000000000000000000000000000001";
        let (token,handles) = cache.register(&resource,&document,observation,&[(0,9,"button".into(),"下一页".into())]).unwrap();
        let id = &handles[0].node_id;
        assert_eq!(cache.resolve(&resource,&document,observation,&token,id).unwrap().backend_node,9);
        // 模拟真实AK的验证与规划等待，不睡眠；普通等待不得让尚未执行的观察引用先过期。
        cache.observations[0].expires -= Duration::from_secs(25);
        assert_eq!(cache.resolve(&resource,&document,observation,&token,id).unwrap().backend_node,9);
        assert!(cache.resolve(&resource,&document,observation,&token,"00000000000000000000000000000000").is_err());
        assert!(cache.resolve(&resource,&DocumentIdentity {loader_id:"replacement".into(),..document.clone()},observation,&token,id).is_err());
        let mut other_room = resource.clone(); other_room.room_id = "room-2".into();
        assert!(cache.resolve(&other_room,&document,observation,&token,id).is_err());
        cache.observations[0].expires = Instant::now()-Duration::from_millis(1);
        assert!(cache.resolve(&resource,&document,observation,&token,id).is_err());
        cache.observations.clear(); assert!(cache.resolve(&resource,&document,observation,&token,id).is_err());
        assert!(DocumentIdentity::from_host(&serde_json::json!({}),&serde_json::json!({"root":{"backendNodeId":1,"nodeType":9}})).is_err());
    }
}
