//! 独立进程Frame的私有CDP读取会话。归属由真实owner、Frame与父Frame共同证明。
use super::{
    native_browser_devtools::{self, ReadMethod},
    native_browser_dom::{open_shadow_roots, NODE_LIMIT},
};
use native_browser_protocol::PanelResource;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tauri::AppHandle;

#[derive(Clone)]
pub(super) struct SessionDocument {
    pub session: String,
    pub frames: Value,
    pub dom: Value,
}
#[derive(Default)]
struct SessionCache {
    resource: Option<PanelResource>,
    targets: HashMap<String, String>,
}
fn cache() -> &'static tokio::sync::Mutex<SessionCache> {
    static CACHE: OnceLock<tokio::sync::Mutex<SessionCache>> = OnceLock::new();
    CACHE.get_or_init(|| tokio::sync::Mutex::new(SessionCache::default()))
}
fn identity(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
}
fn missing_frames(root: &Value, parent: &str) -> Vec<(String, String)> {
    let mut pending = vec![(root, parent)];
    let mut result = Vec::new();
    let mut budget = NODE_LIMIT;
    while let Some((node, parent)) = pending.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        if let Some(children) = node["children"].as_array() {
            pending.extend(children.iter().map(|c| (c, parent)));
        }
        pending.extend(open_shadow_roots(node).map(|r| (r, parent)));
        if !matches!(node["nodeName"].as_str(), Some("IFRAME" | "FRAME")) {
            continue;
        }
        let Some(child) = identity(&node["frameId"]) else {
            continue;
        };
        if let Some(document) = node.get("contentDocument") {
            pending.push((document, child));
        } else {
            result.push((child.to_owned(), parent.to_owned()));
        }
    }
    result
}

/// 只在顶层文档确实缺少子内容时附着iframe；不附着worker、浏览器或其它父页面。
pub(super) async fn documents(
    app: &AppHandle,
    resource: &PanelResource,
    frames: &Value,
    dom: &Value,
) -> Vec<SessionDocument> {
    let Some(top) = identity(&frames["frameTree"]["frame"]["id"]) else {
        return Vec::new();
    };
    let missing = missing_frames(&dom["root"], top);
    if missing.is_empty() {
        return Vec::new();
    }
    // 不在附着中途取消后丢弃session回执；在完整方法之间核采样预算，单方法仍受原CDP超时约束。
    read_documents(app, resource, missing)
        .await
        .unwrap_or_default()
}

async fn read_documents(
    app: &AppHandle,
    resource: &PanelResource,
    mut missing: Vec<(String, String)>,
) -> Result<Vec<SessionDocument>, String> {
    let started = Instant::now();
    let mut cache = cache().lock().await;
    if started.elapsed() >= Duration::from_millis(500) {
        return Ok(Vec::new());
    }
    if cache.resource.as_ref() != Some(resource) {
        cache.targets.clear();
        cache.resource = Some(resource.clone());
    }
    let raw = native_browser_devtools::read(app, resource, ReadMethod::FrameTargets).await?;
    let targets = raw["targetInfos"]
        .as_array()
        .ok_or("native_browser_frame_targets_unavailable")?;
    cache.targets.retain(|id, _| {
        targets
            .iter()
            .any(|target| target["targetId"].as_str() == Some(id.as_str()))
    });
    let mut documents = Vec::new();
    let mut visited = std::collections::HashSet::new();
    loop {
        let mut progress = false;
        for target in targets {
            if documents.len() >= 7
                || visited.len() >= 8
                || started.elapsed() >= Duration::from_millis(500)
            {
                return Ok(documents);
            }
            if target["type"] != "iframe" {
                continue;
            }
            let (Some(id), Some(parent)) = (
                identity(&target["targetId"]),
                identity(&target["parentFrameId"]),
            ) else {
                continue;
            };
            if !missing.iter().any(|(_, p)| p == parent) || !visited.insert(id.to_owned()) {
                continue;
            }
            let session = if let Some(session) = cache.targets.get(id) {
                session.clone()
            } else {
                let attached = native_browser_devtools::read(
                    app,
                    resource,
                    ReadMethod::AttachFrame(id.to_owned()),
                )
                .await?;
                let session = identity(&attached["sessionId"])
                    .ok_or("native_browser_frame_session_unavailable")?
                    .to_owned();
                cache.targets.insert(id.to_owned(), session.clone());
                session
            };
            let frame = match native_browser_devtools::read_session(
                app,
                resource,
                Some(&session),
                ReadMethod::FrameTree,
            )
            .await
            {
                Ok(frame) => frame,
                Err(error) => {
                    cache.targets.remove(id);
                    return Err(error);
                }
            };
            let actual = &frame["frameTree"]["frame"];
            // targetId/URL/标题不作为Frame身份；附着后读取自身根Frame并核真实owner归属。
            if actual["parentId"].as_str() != Some(parent)
                || !missing
                    .iter()
                    .any(|(child, p)| actual["id"].as_str() == Some(child.as_str()) && p == parent)
            {
                cache.targets.remove(id);
                let _ =
                    native_browser_devtools::read(app, resource, ReadMethod::DetachFrame(session))
                        .await;
                continue;
            }
            let document = match native_browser_devtools::read_session(
                app,
                resource,
                Some(&session),
                ReadMethod::DocumentNodes,
            )
            .await
            {
                Ok(document) => document,
                Err(error) => {
                    cache.targets.remove(id);
                    return Err(error);
                }
            };
            missing.extend(missing_frames(
                &document["root"],
                actual["id"].as_str().unwrap(),
            ));
            documents.push(SessionDocument {
                session,
                frames: frame,
                dom: document,
            });
            progress = true;
        }
        if !progress {
            break;
        }
    }
    Ok(documents)
}
