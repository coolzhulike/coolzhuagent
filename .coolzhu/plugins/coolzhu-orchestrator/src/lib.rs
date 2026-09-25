//! Agent Orchestrator — multi-agent task distribution with priority, retry, and timeout.
//!
//! Manages a pool of agents, enqueues tasks with priority routing,
//! dispatches to the best-matching idle agent, and handles retries and timeouts.

use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub category: String,
    pub priority: Priority,
    pub status: TaskStatus,
    pub assigned_to: Option<String>,
    pub result: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout_ms: u64,
    started_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    current_task: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Busy,
    Offline,
}

#[derive(Debug, Clone)]
pub struct Orchestrator {
    tasks: VecDeque<Task>,
    agents: HashMap<String, Agent>,
    completed: Vec<Task>,
    counter: u64,
    pub max_concurrent: usize,
    pub default_timeout_ms: u64,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
            agents: HashMap::new(),
            completed: vec![],
            counter: 0,
            max_concurrent: 5,
            default_timeout_ms: 300_000,
        }
    }

    pub fn register_agent(&mut self, id: &str, name: &str, capabilities: Vec<String>) {
        self.agents.insert(
            id.into(),
            Agent {
                id: id.into(),
                name: name.into(),
                capabilities,
                status: AgentStatus::Idle,
                completed_tasks: 0,
                failed_tasks: 0,
                current_task: None,
            },
        );
    }

    pub fn enqueue(&mut self, desc: &str, category: &str, priority: Priority) -> String {
        let id = format!("task-{}", self.counter);
        self.counter += 1;
        self.tasks.push_back(Task {
            id: id.clone(),
            description: desc.into(),
            category: category.into(),
            priority,
            status: TaskStatus::Pending,
            assigned_to: None,
            result: None,
            retry_count: 0,
            max_retries: 3,
            timeout_ms: self.default_timeout_ms,
            started_at_ms: None,
        });
        id
    }

    pub fn find_agent(&self, category: &str) -> Option<&Agent> {
        self.agents
            .values()
            .filter(|a| a.status == AgentStatus::Idle)
            .filter(|a| {
                a.capabilities
                    .iter()
                    .any(|c| c == category || c == "general")
            })
            .max_by_key(|a| {
                let exact = a.capabilities.iter().any(|c| c == category);
                (exact as u32, a.completed_tasks)
            })
    }

    pub fn dispatch(&mut self) -> usize {
        let mut dispatched = 0;
        let mut indices: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status == TaskStatus::Pending)
            .map(|(i, _)| i)
            .collect();
        indices.sort_by_key(|&i| self.tasks[i].priority);

        let active = self
            .agents
            .values()
            .filter(|a| a.status == AgentStatus::Busy)
            .count();
        for idx in indices {
            if active + dispatched >= self.max_concurrent {
                break;
            }
            if let Some(agent_id) = self
                .find_agent(&self.tasks[idx].category)
                .map(|a| a.id.clone())
            {
                self.tasks[idx].status = TaskStatus::Running;
                self.tasks[idx].assigned_to = Some(agent_id.clone());
                self.tasks[idx].started_at_ms = Some(now_ms());
                if let Some(a) = self.agents.get_mut(&agent_id) {
                    a.status = AgentStatus::Busy;
                    a.current_task = Some(self.tasks[idx].id.clone());
                }
                dispatched += 1;
            }
        }
        dispatched
    }

    pub fn complete_task(&mut self, task_id: &str, result: &str, success: bool) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            if success {
                task.status = TaskStatus::Completed;
                task.result = Some(result.into());
            } else if task.retry_count < task.max_retries {
                task.retry_count += 1;
                task.status = TaskStatus::Pending;
                task.assigned_to = None;
                return; // keep in queue for retry
            } else {
                task.status =
                    TaskStatus::Failed(format!("max retries ({}) exceeded", task.max_retries));
            }
            if let Some(agent_id) = &task.assigned_to {
                if let Some(a) = self.agents.get_mut(agent_id) {
                    a.status = AgentStatus::Idle;
                    a.current_task = None;
                    if success {
                        a.completed_tasks += 1;
                    } else {
                        a.failed_tasks += 1;
                    }
                }
            }
            let id = task.id.clone();
            self.completed.push(task.clone());
            self.tasks.retain(|t| t.id != id);
        }
    }

    pub fn check_timeouts(&mut self) -> Vec<String> {
        let now = now_ms();
        let mut out = vec![];
        for task in self
            .tasks
            .iter_mut()
            .filter(|t| t.status == TaskStatus::Running)
        {
            if let Some(start) = task.started_at_ms {
                if now - start > task.timeout_ms {
                    task.status = TaskStatus::Timeout;
                    out.push(task.id.clone());
                    if let Some(aid) = &task.assigned_to {
                        if let Some(a) = self.agents.get_mut(aid) {
                            a.status = AgentStatus::Idle;
                            a.current_task = None;
                            a.failed_tasks += 1;
                        }
                    }
                }
            }
        }
        out
    }

    pub fn stats(&self) -> Stats {
        Stats {
            total: self.tasks.len() + self.completed.len(),
            pending: self
                .tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .count(),
            running: self
                .tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Running)
                .count(),
            done: self
                .completed
                .iter()
                .filter(|t| t.status == TaskStatus::Completed)
                .count(),
            failed: self
                .completed
                .iter()
                .filter(|t| matches!(t.status, TaskStatus::Failed(_)))
                .count(),
            agents: self.agents.len(),
            idle_agents: self
                .agents
                .values()
                .filter(|a| a.status == AgentStatus::Idle)
                .count(),
        }
    }
}

#[derive(Debug)]
pub struct Stats {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
    pub agents: usize,
    pub idle_agents: usize,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_dispatch_complete() {
        let mut o = Orchestrator::new();
        o.register_agent("a1", "W", vec!["code".into()]);
        let id = o.enqueue("fix", "code", Priority::High);
        o.dispatch();
        o.complete_task(&id, "done", true);
        assert_eq!(o.stats().done, 1);
    }

    #[test]
    fn retry_then_fail() {
        let mut o = Orchestrator::new();
        o.register_agent("a1", "W", vec!["g".into()]);
        let id = o.enqueue("x", "g", Priority::Normal);
        for _ in 0..=3 {
            o.dispatch();
            o.complete_task(&id, "err", false);
        }
        assert_eq!(o.stats().failed, 1);
    }

    #[test]
    fn timeout_frees_agent() {
        let mut o = Orchestrator::new();
        o.default_timeout_ms = 50;
        o.register_agent("a1", "W", vec!["g".into()]);
        o.enqueue("slow", "g", Priority::Normal);
        o.dispatch();
        for t in o.tasks.iter_mut() {
            t.started_at_ms = Some(now_ms() - 200);
        }
        o.check_timeouts();
        assert_eq!(o.stats().idle_agents, 1);
    }
}
