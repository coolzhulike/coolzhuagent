//! 加载期间的宿主导航引用；不创建文档或AX节点，不参与网页点击预检。
use std::{collections::VecDeque, sync::{Mutex, OnceLock}, time::{Duration, Instant}};
use native_browser_protocol::{PanelClickTarget, PanelNavigationTarget, PanelResource};
use tauri::AppHandle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ControlResource {
    pub resource: PanelResource,
    pub url: String,
    pub popup_sequence: u64,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct VerifiedNavigation(pub ControlResource);
struct Reference { control: ControlResource, target: PanelNavigationTarget, expires: Instant }
fn cache() -> &'static Mutex<VecDeque<Reference>> {
    static CACHE: OnceLock<Mutex<VecDeque<Reference>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}
fn random_id() -> Result<String, String> {
    let mut bytes=[0u8;16]; getrandom::fill(&mut bytes).map_err(|_|"native_browser_navigation_unavailable")?;
    Ok(bytes.iter().map(|b|format!("{b:02x}")).collect())
}
pub(super) fn register(control: ControlResource, observation_id: &str) -> Result<PanelNavigationTarget,String> {
    let target=PanelNavigationTarget {observation_id:observation_id.into(),token:random_id()?,id:random_id()?};
    if !target.valid_shape() {return Err("native_browser_navigation_invalid".into());}
    let mut cache=cache().lock().map_err(|_|"native_browser_navigation_unavailable")?;
    cache.retain(|r|r.expires>Instant::now() && r.control==control);
    while cache.len()>=4 {cache.pop_front();}
    cache.push_back(Reference {control,target:target.clone(),expires:Instant::now()+Duration::from_secs(120)});
    Ok(target)
}
pub(super) fn verify(app:&AppHandle,resource:&PanelResource,target:&PanelClickTarget) -> Result<VerifiedNavigation,String> {
    let (current,_) = super::browser_panel::control_snapshot(app).ok_or("native_browser_resource_changed")?;
    if &current.resource!=resource {return Err("native_browser_resource_changed".into());}
    let cache=cache().lock().map_err(|_|"native_browser_navigation_unavailable")?;
    let found=cache.iter().any(|r|r.expires>Instant::now() && r.control==current && r.target.binding()==*target);
    if !found {return Err("native_browser_navigation_reference_stale".into());}
    Ok(VerifiedNavigation(current))
}
pub(super) fn contains(target:&PanelClickTarget) -> bool {
    cache().lock().is_ok_and(|cache|cache.iter().any(|r|r.target.binding()==*target))
}
pub(super) fn retire() {
    if let Ok(mut cache)=cache().lock() {cache.clear();}
}
