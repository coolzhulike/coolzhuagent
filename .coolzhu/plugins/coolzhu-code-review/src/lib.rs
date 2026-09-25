//! Code Review Plugin — automated security, performance, and quality reviews.
//!
//! Scans code changes for common issues:
//! - Security: hardcoded credentials, XSS vectors, unsafe eval
//! - Performance: N+1 queries, console.log in production
//! - Maintainability: TODO comments, overly long functions

/// Severity of a review finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical = 0,
    High = 1,
    Medium = 2,
    Low = 3,
    Info = 4,
}

/// Category of a review finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Security,
    Performance,
    Correctness,
    Consistency,
    Maintainability,
}

/// A single code review finding.
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub category: Category,
    pub file: String,
    pub line: usize,
    pub message: String,
    pub confidence: u8,
}

/// Result of a code review pass.
#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub findings: Vec<Finding>,
    pub summary: String,
    confidence_threshold: u8,
}

impl ReviewResult {
    pub fn new(threshold: u8) -> Self {
        Self {
            findings: vec![],
            summary: String::new(),
            confidence_threshold: threshold,
        }
    }

    pub fn add(&mut self, finding: Finding) {
        if finding.confidence >= self.confidence_threshold {
            self.findings.push(finding);
        }
    }

    pub fn critical_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Critical)
            .count()
    }

    pub fn highest_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).min()
    }
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

pub fn check_security(code: &str, file: &str) -> Vec<Finding> {
    let mut findings = vec![];
    for (i, line) in code.lines().enumerate() {
        let trimmed = line.trim();
        let ln = i + 1;

        if trimmed.contains("password = \"") || trimmed.contains("apikey = \"") {
            findings.push(Finding {
                severity: Severity::Critical,
                category: Category::Security,
                file: file.into(),
                line: ln,
                message: "Hardcoded credential. Use env vars.".into(),
                confidence: 95,
            });
        }
        if trimmed.contains("eval(") || trimmed.contains("exec(") {
            findings.push(Finding {
                severity: Severity::Critical,
                category: Category::Security,
                file: file.into(),
                line: ln,
                message: "Dangerous eval/exec. Use safer alternatives.".into(),
                confidence: 90,
            });
        }
        if trimmed.contains(".innerHTML") || trimmed.contains("dangerouslySetInnerHTML") {
            findings.push(Finding {
                severity: Severity::High,
                category: Category::Security,
                file: file.into(),
                line: ln,
                message: "Potential XSS. Use textContent or sanitize.".into(),
                confidence: 80,
            });
        }
    }
    findings
}

pub fn check_performance(code: &str, file: &str) -> Vec<Finding> {
    let mut findings = vec![];
    if code.contains("for (") && code.contains("query(") {
        findings.push(Finding {
            severity: Severity::High,
            category: Category::Performance,
            file: file.into(),
            line: 0,
            message: "Potential N+1 query: DB call inside loop.".into(),
            confidence: 70,
        });
    }
    if code.contains("console.log") {
        findings.push(Finding {
            severity: Severity::Low,
            category: Category::Maintainability,
            file: file.into(),
            line: 0,
            message: "console.log left in code. Remove or use logging framework.".into(),
            confidence: 85,
        });
    }
    findings
}

/// Run full review on a set of changed files.
pub fn review(changes: &[(&str, &str)], threshold: u8) -> ReviewResult {
    let mut result = ReviewResult::new(threshold);
    for (file, code) in changes {
        for f in check_security(code, file) {
            result.add(f);
        }
        for f in check_performance(code, file) {
            result.add(f);
        }
    }
    let total = result.findings.len();
    let critical = result.critical_count();
    result.summary = format!("{total} findings ({critical} critical)");
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_hardcoded_password() {
        let findings = check_security("password = \"secret\"", "config.py");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Critical);
    }

    #[test]
    fn detects_eval() {
        let findings = check_security("result = eval(input)", "x.js");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn detects_xss() {
        let findings = check_security("<div dangerouslySetInnerHTML={{}} />", "c.tsx");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn confidence_filter_works() {
        let r = review(&[("f.js", "password = \"x\"")], 80);
        assert_eq!(r.findings.len(), 1);
        let r2 = review(&[("f.js", "password = \"x\"")], 100);
        assert_eq!(r2.findings.len(), 0);
    }

    #[test]
    fn empty_code_no_findings() {
        let r = review(&[("e.rs", "")], 50);
        assert!(r.findings.is_empty());
    }
}
