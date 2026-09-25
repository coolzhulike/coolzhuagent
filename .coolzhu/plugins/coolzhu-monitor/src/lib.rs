//! Agent Monitor Plugin — real-time agent monitoring with alerts.
//! B4: Dashboard computation, token usage tracking, alert rules.

#[derive(Debug, Clone)]
pub struct AgentMetrics {
    pub agent_id: String,
    pub agent_name: String,
    pub status: String,
    pub current_task: Option<String>,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub tokens_used: u64,
    pub tokens_limit: u64,
    pub uptime_seconds: u64,
    pub last_active_seconds_ago: u64,
}

#[derive(Debug, Clone)]
pub struct DashboardState {
    pub agents: Vec<AgentMetrics>,
    pub total_tokens: u64,
    pub active_sessions: usize,
    pub error_rate: f32,
}

#[derive(Debug, Clone)]
pub struct AlertRule {
    pub metric: String,
    pub threshold: f32,
    pub operator: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub rule_name: String,
    pub agent_id: Option<String>,
    pub message: String,
    pub severity: String,
}

pub fn compute_dashboard(metrics: &[AgentMetrics]) -> DashboardState {
    let total_tokens: u64 = metrics.iter().map(|m| m.tokens_used).sum();
    let active = metrics.iter().filter(|m| m.status == "busy").count();
    let failed: usize = metrics.iter().map(|m| m.tasks_failed).sum();
    let completed: usize = metrics.iter().map(|m| m.tasks_completed).sum();
    let total = failed + completed;
    let error_rate = if total > 0 {
        failed as f32 / total as f32
    } else {
        0.0
    };
    DashboardState {
        agents: metrics.to_vec(),
        total_tokens,
        active_sessions: active,
        error_rate,
    }
}

pub fn evaluate_alerts(metrics: &[AgentMetrics], rules: &[AlertRule]) -> Vec<Alert> {
    let mut alerts = vec![];
    for rule in rules {
        for agent in metrics {
            let value = match rule.metric.as_str() {
                "tokens_used_pct" => {
                    agent.tokens_used as f32 / agent.tokens_limit.max(1) as f32 * 100.0
                }
                "error_rate" => {
                    let t = agent.tasks_completed + agent.tasks_failed;
                    if t > 0 {
                        agent.tasks_failed as f32 / t as f32
                    } else {
                        0.0
                    }
                }
                "idle_minutes" => agent.last_active_seconds_ago as f32 / 60.0,
                _ => 0.0,
            };
            let ok = match rule.operator.as_str() {
                ">" => value > rule.threshold,
                _ => value < rule.threshold,
            };
            if ok {
                alerts.push(Alert {
                    rule_name: format!("{}-{}", rule.metric, rule.operator),
                    agent_id: Some(agent.agent_id.clone()),
                    message: rule
                        .message
                        .replace("{agent}", &agent.agent_name)
                        .replace("{value}", &format!("{value:.1}")),
                    severity: (if rule.metric == "tokens_used_pct" {
                        "warning"
                    } else {
                        "info"
                    })
                    .into(),
                });
            }
        }
    }
    alerts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, busy: bool, done: usize, fail: usize, tokens: u64) -> AgentMetrics {
        AgentMetrics {
            agent_id: id.into(),
            agent_name: id.into(),
            status: if busy { "busy" } else { "idle" }.into(),
            current_task: None,
            tasks_completed: done,
            tasks_failed: fail,
            tokens_used: tokens,
            tokens_limit: 10000,
            uptime_seconds: 0,
            last_active_seconds_ago: 0,
        }
    }

    #[test]
    fn dashboard_aggregates() {
        let state = compute_dashboard(&[
            agent("a1", true, 10, 2, 5000),
            agent("a2", false, 5, 0, 2000),
        ]);
        assert_eq!(state.active_sessions, 1);
        assert_eq!(state.total_tokens, 7000);
    }

    #[test]
    fn alert_on_high_tokens() {
        let rules = &[AlertRule {
            metric: "tokens_used_pct".into(),
            threshold: 80.0,
            operator: ">".into(),
            message: "{agent} at {value}%".into(),
        }];
        assert!(!evaluate_alerts(&[agent("x", true, 0, 0, 9500)], rules).is_empty());
        assert!(evaluate_alerts(&[agent("x", true, 0, 0, 100)], rules).is_empty());
    }
}
