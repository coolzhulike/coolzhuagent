use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::trace_id::{SpanId, TraceId};

thread_local! {
    static CONTEXT_STACK: RefCell<Vec<SpanContext>> = RefCell::new(Vec::new());
}

#[derive(Debug, Clone)]
pub struct SpanContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_id: Option<SpanId>,
}

impl Copy for SpanContext {}

#[derive(Debug, Clone)]
pub struct Span {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_id: Option<SpanId>,
    pub name: String,
    pub module: String,
    pub start_time: SystemTime,
    pub attributes: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
}

#[derive(Debug, Clone)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: SystemTime,
    pub attributes: HashMap<String, String>,
}

pub struct SpanGuard {
    span: Arc<Mutex<Span>>,
    closed: bool,
}

impl SpanGuard {
    pub fn record(&mut self, key: &str, value: impl Into<String>) {
        if let Ok(mut span) = self.span.lock() {
            span.attributes.insert(key.to_string(), value.into());
        }
    }

    pub fn event(&mut self, name: &str, attrs: HashMap<String, String>) {
        if let Ok(mut span) = self.span.lock() {
            span.events.push(SpanEvent {
                name: name.to_string(),
                timestamp: SystemTime::now(),
                attributes: attrs,
            });
        }
    }

    pub fn span_id(&self) -> SpanId {
        if let Ok(span) = self.span.lock() {
            span.span_id
        } else {
            SpanId::generate()
        }
    }

    pub fn trace_id(&self) -> TraceId {
        if let Ok(span) = self.span.lock() {
            span.trace_id
        } else {
            TraceId::generate()
        }
    }

    pub fn context(&self) -> SpanContext {
        if let Ok(span) = self.span.lock() {
            SpanContext {
                trace_id: span.trace_id,
                span_id: span.span_id,
                parent_id: span.parent_id,
            }
        } else {
            SpanContext {
                trace_id: TraceId::generate(),
                span_id: SpanId::generate(),
                parent_id: None,
            }
        }
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;

        let mut closed_span_id = None;
        if let Ok(span) = self.span.lock() {
            closed_span_id = Some(span.span_id);
            let end_time = SystemTime::now();
            let duration = end_time
                .duration_since(span.start_time)
                .unwrap_or(Duration::ZERO);

            let span_data = SpanClosed {
                trace_id: span.trace_id,
                span_id: span.span_id,
                parent_id: span.parent_id,
                name: span.name.clone(),
                module: span.module.clone(),
                start_time: span.start_time,
                end_time,
                duration_ms: duration.as_millis() as u64,
                attributes: span.attributes.clone(),
                events: span.events.clone(),
            };

            crate::emit_span_closed(&span_data);
        }

        if let Some(span_id) = closed_span_id {
            pop_context(span_id);
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpanClosed {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_id: Option<SpanId>,
    pub name: String,
    pub module: String,
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub duration_ms: u64,
    pub attributes: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
}

pub fn start_span(name: &str, module: &str) -> SpanGuard {
    let parent_ctx = current_context();

    let trace_id = parent_ctx
        .as_ref()
        .map(|c| c.trace_id)
        .unwrap_or_else(TraceId::generate);
    let parent_id = parent_ctx.as_ref().map(|c| c.span_id);

    let span = Span {
        trace_id,
        span_id: SpanId::generate(),
        parent_id,
        name: name.to_string(),
        module: module.to_string(),
        start_time: SystemTime::now(),
        attributes: HashMap::new(),
        events: Vec::new(),
    };

    let guard = SpanGuard {
        span: Arc::new(Mutex::new(span)),
        closed: false,
    };

    let new_ctx = guard.context();
    push_context(new_ctx);

    guard
}

pub fn start_span_with_parent(name: &str, module: &str, parent: &SpanGuard) -> SpanGuard {
    let parent_ctx = parent.context();

    let span = Span {
        trace_id: parent_ctx.trace_id,
        span_id: SpanId::generate(),
        parent_id: Some(parent_ctx.span_id),
        name: name.to_string(),
        module: module.to_string(),
        start_time: SystemTime::now(),
        attributes: HashMap::new(),
        events: Vec::new(),
    };

    let guard = SpanGuard {
        span: Arc::new(Mutex::new(span)),
        closed: false,
    };
    push_context(guard.context());
    guard
}

#[must_use]
pub fn current_context() -> Option<SpanContext> {
    CONTEXT_STACK.with(|stack| stack.borrow().last().copied())
}

#[must_use]
pub fn current_trace_id() -> Option<TraceId> {
    current_context().map(|c| c.trace_id)
}

#[must_use]
pub fn current_span_id() -> Option<SpanId> {
    current_context().map(|c| c.span_id)
}

pub fn enter_span(guard: &SpanGuard) {
    push_context(guard.context());
}

pub fn exit_span() {
    CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
}

fn push_context(context: SpanContext) {
    CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().push(context);
    });
}

fn pop_context(span_id: SpanId) {
    CONTEXT_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack
            .last()
            .is_some_and(|context| context.span_id == span_id)
        {
            stack.pop();
            return;
        }
        if let Some(index) = stack.iter().rposition(|context| context.span_id == span_id) {
            stack.remove(index);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn span_guard_records_attributes() {
        let mut guard = start_span("test_op", "test_module");
        guard.record("key1", "value1");
        guard.record("key2", "42");

        let span = guard.span.lock().unwrap();
        assert_eq!(span.attributes.get("key1"), Some(&"value1".to_string()));
        assert_eq!(span.attributes.get("key2"), Some(&"42".to_string()));
    }

    #[test]
    fn span_guard_records_events() {
        let mut guard = start_span("test_op", "test_module");
        guard.event("something_happened", HashMap::new());

        let span = guard.span.lock().unwrap();
        assert_eq!(span.events.len(), 1);
        assert_eq!(span.events[0].name, "something_happened");
    }

    #[test]
    fn span_context_propagation() {
        let guard = start_span("parent", "module");
        let parent_ctx = guard.context();

        let child_guard = start_span_with_parent("child", "module", &guard);
        let child_ctx = child_guard.context();

        assert_eq!(parent_ctx.trace_id, child_ctx.trace_id);
        assert_eq!(child_ctx.parent_id, Some(parent_ctx.span_id));
        assert_ne!(parent_ctx.span_id, child_ctx.span_id);
    }

    #[test]
    fn current_context_tracking() {
        let _guard = start_span("test", "module");
        let ctx = current_context();
        assert!(ctx.is_some());
    }

    #[test]
    fn dropping_child_span_restores_parent_context() {
        let parent = start_span("parent", "module");
        let parent_ctx = current_context().expect("parent context should be active");
        {
            let child = start_span("child", "module");
            let child_ctx = current_context().expect("child context should be active");
            assert_eq!(child_ctx.parent_id, Some(parent_ctx.span_id));
            assert_ne!(child_ctx.span_id, parent_ctx.span_id);
            drop(child);
        }

        assert_eq!(
            current_context().map(|ctx| ctx.span_id),
            Some(parent.span_id())
        );
        drop(parent);
        assert!(current_context().is_none());
    }

    #[test]
    fn span_context_is_thread_local() {
        let _guard = start_span("main-thread", "module");
        assert!(current_context().is_some());

        let child_context = thread::spawn(current_context)
            .join()
            .expect("thread should join");

        assert!(child_context.is_none());
    }

    #[test]
    fn span_duration_measured() {
        let guard = start_span("delayed_op", "test");
        thread::sleep(Duration::from_millis(10));
        drop(guard);
    }
}
