//! 每个 CU job 的视觉起点只用于验收；不保存旧 UIA 引用，不参与输入授权。
use computer_use::Observation;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Start {
    image: Arc<str>,
    generation: u64,
    geometry: Value,
}

#[derive(Default)]
pub(crate) struct RunBaseline(Mutex<Option<Start>>);

pub(crate) struct Comparison {
    pub images: Vec<String>,
    pub labels: Vec<&'static str>,
    pub start_generation: u64,
    pub comparable: bool,
}

fn image(observation: &Observation) -> Option<&str> {
    observation.state.pointer("/image/data_url").and_then(Value::as_str)
        .filter(|value| value.starts_with("data:image/png;base64,"))
}

fn geometry(observation: &Observation) -> Value {
    json!({"window": observation.state.pointer("/window/reference"),
        "process_id": observation.state.pointer("/window/process_id"),
        "screen_rect": observation.state.pointer("/image/screen_rect"),
        "canvas_rect": observation.state.get("canvas_rect"),
        "width": observation.state.pointer("/image/width"),
        "height": observation.state.pointer("/image/height"),
        "dpi": observation.state.pointer("/image/dpi")})
}

impl RunBaseline {
    pub fn capture_start(&self, observation: &Observation) {
        let Some(image) = image(observation) else { return; };
        let mut stored = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        stored.get_or_insert_with(|| Start { image: Arc::from(image),
            generation: observation.generation, geometry: geometry(observation) });
    }

    pub fn comparison(&self, previous: &Observation, current: &Observation) -> Option<Comparison> {
        let previous_image = image(previous)?;
        let current_image = image(current)?;
        let start = {
            let mut stored = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            stored.get_or_insert_with(|| Start { image: Arc::from(previous_image),
                generation: previous.generation, geometry: geometry(previous) }).clone()
        };
        let comparable = start.geometry == geometry(current);
        let mut images = vec![start.image.to_string()];
        let mut labels = vec!["run_start"];
        if start.generation == previous.generation {
            labels[0] = "run_start = previous_step";
        } else {
            images.push(previous_image.to_string());
            labels.push("previous_step");
        }
        if previous.generation == current.generation {
            if start.generation == previous.generation {
                labels[0] = "run_start = previous_step = current";
            } else {
                *labels.last_mut()? = "previous_step = current";
            }
        } else {
            images.push(current_image.to_string());
            labels.push("current");
        }
        Some(Comparison { images, labels, start_generation: start.generation, comparable })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observed(generation: u64, window: &str) -> Observation {
        Observation { generation, surface: computer_use::ComputerUseSurface::Desktop,
            surface_identity: window.into(), evidence: vec![],
            state: json!({"window":{"reference":window,"process_id":1},
                "image":{"data_url":format!("data:image/png;base64,frame-{generation}"),
                    "screen_rect":[10,20,600,400],"width":600,"height":400},
                "canvas_rect":[10,40,600,380],"elements":[{"reference":"旧引用不进入基线"}]}) }
    }
    #[test]
    fn successive_steps_keep_the_start_and_label_previous_separately() {
        let baseline = RunBaseline::default();
        baseline.capture_start(&observed(1,"one"));
        baseline.capture_start(&observed(2,"one"));
        let initial = baseline.comparison(&observed(1,"one"), &observed(1,"one")).unwrap();
        assert_eq!(initial.labels, ["run_start = previous_step = current"]);
        let first = baseline.comparison(&observed(1,"one"), &observed(2,"one")).unwrap();
        assert_eq!(first.labels, ["run_start = previous_step","current"]);
        let later = baseline.comparison(&observed(2,"one"), &observed(3,"one")).unwrap();
        assert_eq!(later.labels, ["run_start","previous_step","current"]);
        assert_eq!(later.images[0], initial.images[0]);
        assert_eq!(later.start_generation, 1);
        let unchanged = baseline.comparison(&observed(3,"one"), &observed(3,"one")).unwrap();
        assert_eq!(unchanged.labels, ["run_start","previous_step = current"]);
    }
    #[test]
    fn window_change_does_not_replace_start_or_leak_it_to_another_job() {
        let baseline = RunBaseline::default();
        baseline.comparison(&observed(1,"one"), &observed(2,"one")).unwrap();
        let changed = baseline.comparison(&observed(2,"one"), &observed(3,"two")).unwrap();
        assert!(!changed.comparable);
        assert_eq!(changed.start_generation, 1);
        let new_job = RunBaseline::default().comparison(&observed(9,"two"), &observed(9,"two")).unwrap();
        assert_eq!(new_job.start_generation, 9);
        assert!(new_job.comparable);
    }
}
