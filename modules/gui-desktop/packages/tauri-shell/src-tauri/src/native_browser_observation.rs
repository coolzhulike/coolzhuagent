//! 原生网页观察：固定宿主只读查询、实际文档身份与有限AX；不接受任意脚本、不发送输入。
use native_browser_protocol::{ObservedNode, PageObservation, PanelResource};
use tauri::{AppHandle, Manager};
use super::{native_browser_devtools::{self,ReadMethod},
    native_browser_dom::operable_root_nodes,
    native_browser_document::{self,DocumentScope,DocumentSnapshot,DocumentSet},native_browser_nodes::NodeCandidate};

pub(super) fn bounded_text(value: Option<&serde_json::Value>, limit: usize) -> String {
    value.and_then(|value| value.get("value")).and_then(serde_json::Value::as_str)
        .unwrap_or_default().chars().filter(|c| !c.is_control()).take(limit).collect()
}
fn focused_editor(node: &serde_json::Value, role: &str) -> bool {
    matches!(role, "textbox" | "searchbox" | "combobox" | "button" | "link" | "checkbox" | "radio")
        && node.get("properties").and_then(serde_json::Value::as_array).is_some_and(|properties|
            properties.iter().any(|property| property["name"] == "focused"
                && property.pointer("/value/value").and_then(serde_json::Value::as_bool) == Some(true)))
}
#[cfg(test)]
fn project_tree(value: &serde_json::Value) -> Result<PageObservation,String> {
    let nodes = value.get("nodes").and_then(serde_json::Value::as_array).ok_or("native_observation_invalid")?;
    let mut result = PageObservation {url:String::new(),title:String::new(),nodes:Vec::new(),truncated:false,
        document_token:None,node_handles:Vec::new(),viewport:None,focused_node_index:None,loading:false,navigation_target:None};
    for node in nodes {
        if node.get("ignored").and_then(serde_json::Value::as_bool) != Some(false) { continue; }
        let role = bounded_text(node.get("role"),64);
        let name = bounded_text(node.get("name"),256);
        if role == "RootWebArea" && result.title.is_empty() { result.title = name.clone(); }
        if role.is_empty() && name.is_empty() { continue; }
        if result.nodes.len() == 128 { result.truncated = true; break; }
        // 不提取value/properties；name仍是可能含私密内容的不可信页面文字。
        result.nodes.push(ObservedNode {role,name});
    }
    Ok(result)
}
/// 同进程子文档由FrameTree与pierced DOM共同证明；导航穿过读取窗口时不能拼接两个文档。
pub(super) async fn snapshot(app:&AppHandle,resource:&PanelResource) -> Result<(DocumentSnapshot,DocumentSet),String> {
    let frames = native_browser_devtools::read(app,resource,ReadMethod::FrameTree).await?;
    let mut dom = DocumentSet::top(native_browser_devtools::read(app,resource,ReadMethod::DocumentNodes).await?);
    dom.sessions=super::native_browser_sessions::documents(app,resource,&frames,&dom.top).await;
    let first = native_browser_document::snapshot_from_documents(&frames,&dom)?;
    let fresh = native_browser_devtools::read(app,resource,ReadMethod::FrameTree).await?;
    for document in &mut dom.sessions {
        document.frames=native_browser_devtools::read_session(app,resource,Some(&document.session),ReadMethod::FrameTree).await?;
    }
    if native_browser_document::snapshot_from_documents(&fresh,&dom)? != first {
        return Err("native_browser_document_changed".into());
    }
    Ok((first,dom))
}

fn append_frame(page:&mut PageObservation,tree:&serde_json::Value,scope:&DocumentScope,root:&serde_json::Value,
    candidates:&mut Vec<NodeCandidate>,focused:&mut Vec<usize>) -> Result<(),String> {
    page.truncated |= super::native_browser_dom::root_is_partial(root);
    let operable = operable_root_nodes(root);
    for node in tree["nodes"].as_array().ok_or("native_observation_invalid")? {
        if node["ignored"].as_bool() != Some(false) { continue; }
        let role = bounded_text(node.get("role"),64);
        let name = bounded_text(node.get("name"),256);
        if role.is_empty() && name.is_empty() { continue; }
        if page.nodes.len() == 128 { page.truncated = true; break; }
        if scope.is_top() && role == "RootWebArea" && page.title.is_empty() { page.title = name.clone(); }
        let index = page.nodes.len();
        // 子文档RootWebArea仅供自身滚动；导航仍由执行预检限制为顶层。
        let supported = ["RootWebArea","button","link","textbox","searchbox","checkbox","radio","combobox"].contains(&role.as_str());
        if supported && node["frameId"].as_str().is_none_or(|frame| frame == scope.document().frame_id) {
            if let Some(backend_node) = node["backendDOMNodeId"].as_i64().filter(|id| operable.contains(id)) {
                candidates.push(NodeCandidate {index,backend_node,role:role.clone(),name:name.clone(),scope:scope.clone()});
                if focused_editor(node,&role) { focused.push(index); }
            }
        }
        page.nodes.push(ObservedNode {role,name});
    }
    Ok(())
}

pub(super) async fn observe(app: &AppHandle, expected: &PanelResource, observation_id: &str) -> Result<PageObservation,String> {
    // 短暂让已经接近完成的导航落地；慢请求返回宿主控制状态，不等待CDP超时。
    for _ in 0..10 {
        let (control,loading)=super::browser_panel::control_snapshot(app).ok_or("native_browser_resource_changed")?;
        if &control.resource!=expected {return Err("native_browser_resource_changed".into());}
        if !loading {break;}
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    if let Some(page)=loading_observation(app,expected,observation_id)? {return Ok(page);}
    let result=observe_document(app,expected,observation_id).await;
    if result.is_err() {
        // 页面在读文档过程中开始导航，返回明确loading事实；没有旧页面节点可复用。
        if let Some(page)=loading_observation(app,expected,observation_id)? {return Ok(page);}
    }
    result
}

fn loading_observation(app:&AppHandle,expected:&PanelResource,observation_id:&str)->Result<Option<PageObservation>,String> {
    let (control,loading)=super::browser_panel::control_snapshot(app).ok_or("native_browser_resource_changed")?;
    if &control.resource!=expected {return Err("native_browser_resource_changed".into());}
    if !loading {return Ok(None);}
    let navigation_target=super::native_browser_navigation::register(control.clone(),observation_id)?;
    let page=PageObservation {url:control.url,title:String::new(),nodes:Vec::new(),truncated:false,
        viewport:None,document_token:None,node_handles:Vec::new(),focused_node_index:None,loading:true,navigation_target:Some(navigation_target)};
    if page.valid_shape() {Ok(Some(page))} else {Err("native_observation_invalid".into())}
}

async fn observe_document(app: &AppHandle, expected: &PanelResource, observation_id: &str) -> Result<PageObservation,String> {
    if super::browser_panel::input_resource(app).as_ref() != Some(expected) { return Err("native_browser_resource_changed".into()); }
    let url = app.get_webview(&expected.label).ok_or("native_browser_unavailable")?
        .url().map_err(|_| "native_browser_unavailable")?.to_string();
    let (stamp,dom) = snapshot(app,expected).await?;
    let mut page = PageObservation {url:String::new(),title:String::new(),nodes:Vec::new(),truncated:stamp.truncated,
        document_token:None,node_handles:Vec::new(),viewport:None,focused_node_index:None,loading:false,navigation_target:None};
    let mut candidates = Vec::new();
    let mut focused_editors = Vec::new();
    let frame_started = std::time::Instant::now();
    for scope in &stamp.scopes {
        let tree = if scope.is_top() {
            native_browser_devtools::read_session(app,expected,scope.session(),ReadMethod::Accessibility(scope.document().frame_id.clone())).await?
        } else {
            let remaining = std::time::Duration::from_millis(500).saturating_sub(frame_started.elapsed());
            match tokio::time::timeout(remaining,native_browser_devtools::read_session(app,expected,scope.session(),
                ReadMethod::Accessibility(scope.document().frame_id.clone()))).await {
                Ok(Ok(tree)) => tree,
                _ => { page.truncated = true; continue; },
            }
        };
        let root = native_browser_document::root_for_scope(&dom,scope)?;
        append_frame(&mut page,&tree,scope,root,&mut candidates,&mut focused_editors)?;
    }
    let metrics = native_browser_devtools::read(app,expected,ReadMethod::LayoutMetrics).await?;
    let viewport = &metrics["cssVisualViewport"];
    let number = |key| viewport[key].as_f64().ok_or("native_browser_viewport_invalid");
    let viewport = native_browser_protocol::PageViewport {
        page_x:number("pageX")?, page_y:number("pageY")?,
        width:number("clientWidth")?, height:number("clientHeight")?,
    };
    if !viewport.valid_shape() { return Err("native_browser_viewport_invalid".into()); }
    page.viewport = Some(viewport);
    // 只读几何提示与输入预检复用坐标语义。总采样余量耗尽时保留未知节点，不能过滤离屏目标。
    // 子页面复用预检的逐层坐标换算和owner只读命中；不自动滚动、不派发输入。
    // 观察结果只是提示，不能替代随后执行前的文档/布局/命中复核。
    let started = std::time::Instant::now();
    let mut visibility = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        let remaining = std::time::Duration::from_millis(500).saturating_sub(started.elapsed());
        // LayoutMetrics属于CDP target根；同进程嵌套文档不能冒用其父视口。
        let target_root = candidate.scope.is_top() || candidate.scope.chain.iter().rev().nth(1)
            .is_some_and(|parent| parent.session.as_deref() != candidate.scope.session());
        let document_viewport = if candidate.role == "RootWebArea" && !remaining.is_zero() && !target_root {
            // 同进程子文档读取自己的scrollingElement。每次CDP读有2秒上限；
            // 允许最后一次采样完成对象释放后才结束，而非500ms强丢future遗留远端对象。
            // 后续候选仍按总采样余量跳过；此事实不授予输入，也不替代最终目标验收。
            super::native_browser_scroll::read_viewport(app,expected,&candidate.scope,candidate.backend_node).await.ok()
        } else if candidate.role == "RootWebArea" && target_root && !remaining.is_zero() {
            let read = async {
                let raw = if candidate.scope.is_top() {metrics.clone()} else {
                    super::native_browser_frame_geometry::local_metrics(native_browser_devtools::read_session(
                        app,expected,candidate.scope.session(),ReadMethod::LayoutMetrics).await?)?
                };
                let v=&raw["cssVisualViewport"];
                let viewport=native_browser_protocol::PageViewport {
                    page_x:v["pageX"].as_f64().ok_or("native_browser_viewport_invalid")?,
                    page_y:v["pageY"].as_f64().ok_or("native_browser_viewport_invalid")?,
                    width:v["clientWidth"].as_f64().ok_or("native_browser_viewport_invalid")?,
                    height:v["clientHeight"].as_f64().ok_or("native_browser_viewport_invalid")?,
                };
                if viewport.valid_shape() {Ok(viewport)} else {Err("native_browser_viewport_invalid".to_string())}
            };
            tokio::time::timeout(remaining,read).await.ok().and_then(Result::ok)
        } else {None};
        let remaining = std::time::Duration::from_millis(500).saturating_sub(started.elapsed());
        let hint = if candidate.role == "RootWebArea" || remaining.is_zero() { None } else {
            let locate = async {
                let geometry = native_browser_devtools::read_session(app,expected,candidate.scope.session(),ReadMethod::BoxModel(candidate.backend_node)).await?;
                if candidate.scope.session().is_some() {
                    super::native_browser_frame_geometry::locate(app,expected,&candidate.scope,&geometry,&metrics).await.map(|_| ())
                } else {
                    super::native_browser_target::point(&geometry,&metrics).map(|_| ())
                }
            };
            match tokio::time::timeout(remaining,locate).await {
                Ok(Ok(())) => Some(true),
                Ok(Err(code)) if code == "native_browser_target_outside_viewport" => Some(false),
                _ => None,
            }
        };
        visibility.push((hint,document_viewport));
    }
    if snapshot(app,expected).await?.0 != stamp || super::browser_panel::input_resource(app).as_ref() != Some(expected)
        || app.get_webview(&expected.label).and_then(|view| view.url().ok()).as_ref().map(|url| url.as_str()) != Some(url.as_str()) {
        return Err("native_browser_document_changed".into());
    }
    let (token,mut handles) = super::native_browser_nodes::register(expected,&stamp,observation_id,&candidates)?;
    for (handle,(hint,viewport)) in handles.iter_mut().zip(visibility) {
        handle.in_viewport = hint; handle.document_viewport = viewport;
    }
    page.url = url; page.document_token = Some(token); page.node_handles = handles;
    page.focused_node_index = (focused_editors.len() == 1).then(|| focused_editors[0]);
    if page.valid_shape() { Ok(page) } else { Err("native_observation_invalid".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ax_projection_omits_private_values_and_preserves_top_title() {
        let raw = serde_json::json!({"nodes":[
            {"ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"主页面"}},
            {"ignored":false,"role":{"value":"textbox"},"name":{"value":"口令"},"value":{"value":"SECRET"}},
            {"ignored":true,"name":{"value":"HIDDEN"}},
            {"ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"子页面"}}]});
        let mut page = project_tree(&raw).unwrap(); page.url = "https://example.invalid/".into();
        let text = serde_json::to_string(&page).unwrap();
        assert!(page.valid_shape()); assert_eq!(page.title,"主页面");
        assert!(!text.contains("SECRET") && !text.contains("HIDDEN"));
        let focus = serde_json::json!({"properties":[{"name":"focused","value":{"value":true}},
            {"name":"value","value":{"value":"SECRET"}}]});
        assert!(focused_editor(&focus,"textbox"));
        assert!(focused_editor(&focus,"button"));
        assert!(!focused_editor(&serde_json::json!({"properties":[{"name":"focused","value":{"value":"true"}}]}),"textbox"));
        page.document_token = Some("a".repeat(32));
        page.node_handles.push(native_browser_protocol::NodeHandle {index:1,node_id:"b".repeat(32),in_viewport:None,document_viewport:None});
        page.focused_node_index = Some(1);
        assert!(page.valid_shape());
        page.focused_node_index = Some(0);
        assert!(!page.valid_shape());
        page.focused_node_index = None;
        page.node_handles.push(native_browser_protocol::NodeHandle {index:999,node_id:"0".repeat(32),in_viewport:None,document_viewport:None});
        assert!(!page.valid_shape());
        let dom = serde_json::json!({"root":{"backendNodeId":1,"children":[{"backendNodeId":2,
            "shadowRoots":[{"backendNodeId":3}],"contentDocument":{"backendNodeId":4}}]}});
        let ids = super::super::native_browser_dom::operable_document_nodes(&dom);
        assert!(ids.contains(&1) && ids.contains(&2) && !ids.contains(&3) && !ids.contains(&4));
    }
}
