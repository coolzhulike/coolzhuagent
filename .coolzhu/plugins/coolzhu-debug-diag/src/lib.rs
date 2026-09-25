//! Debug Diagnostics Plugin — crash analysis, leak detection, profiling.
//! C8: Stack trace parsing, memory leak detection, performance profiling.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct CrashReport {
    pub signal: String,
    pub message: String,
    pub stacktrace: Vec<StackFrame>,
}

#[derive(Debug, Clone)]
pub struct LeakCandidate {
    pub location: String,
    pub allocation_size: u64,
    pub allocation_count: u64,
}

#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    pub total_allocated: u64,
    pub total_freed: u64,
    pub leak_candidates: Vec<LeakCandidate>,
}

#[derive(Debug, Clone)]
pub struct PerformanceProfile {
    pub name: String,
    pub duration_ms: u64,
    pub call_count: u64,
    pub children: Vec<PerformanceProfile>,
}

#[derive(Debug, Clone)]
pub struct DiagnosticReport {
    pub crash: Option<CrashReport>,
    pub memory: Option<MemorySnapshot>,
    pub profiles: Vec<PerformanceProfile>,
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Stack trace parsing
// ---------------------------------------------------------------------------

pub fn parse_stacktrace(text: &str) -> Vec<StackFrame> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed.find(" at ").map(|at_pos| {
                let func = trimmed[..at_pos].trim().to_string();
                let loc = trimmed[at_pos + 4..].trim();
                let (file, line_num) = loc
                    .rfind(':')
                    .map(|c| {
                        (
                            loc[..c].to_string(),
                            loc[c + 1..].parse::<u32>().unwrap_or(0),
                        )
                    })
                    .unwrap_or((loc.to_string(), 0));
                StackFrame {
                    function: func,
                    file,
                    line: line_num,
                }
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Crash analysis
// ---------------------------------------------------------------------------

pub fn analyze_crash(stacktrace_text: &str, signal: &str) -> CrashReport {
    CrashReport {
        signal: signal.into(),
        message: format!("Process terminated with signal: {signal}"),
        stacktrace: parse_stacktrace(stacktrace_text),
    }
}

// ---------------------------------------------------------------------------
// Leak detection
// ---------------------------------------------------------------------------

pub fn detect_leaks(allocations: &[(u64, u64, &str)]) -> Vec<LeakCandidate> {
    let mut by_loc: HashMap<&str, (u64, u64)> = HashMap::new();
    for (alloc, freed, loc) in allocations {
        let e = by_loc.entry(loc).or_insert((0, 0));
        e.0 += alloc;
        e.1 += freed;
    }
    by_loc
        .into_iter()
        .filter(|(_, (a, f))| a > f)
        .map(|(loc, (a, f))| LeakCandidate {
            location: loc.into(),
            allocation_size: a - f,
            allocation_count: 1,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Performance profiling
// ---------------------------------------------------------------------------

pub fn flat_to_profiles(flat: &[(&str, u64, u64)]) -> Vec<PerformanceProfile> {
    flat.iter()
        .map(|(name, dur, count)| PerformanceProfile {
            name: name.to_string(),
            duration_ms: *dur,
            call_count: *count,
            children: vec![],
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Report generation
// ---------------------------------------------------------------------------

pub fn generate_report(
    crash_text: Option<&str>,
    allocations: Option<&[(u64, u64, &str)]>,
    profiles: &[(&str, u64, u64)],
) -> DiagnosticReport {
    let crash = crash_text.map(|t| analyze_crash(t, "SIGSEGV"));
    let memory = allocations.map(|a| {
        let leaks = detect_leaks(a);
        MemorySnapshot {
            total_allocated: a.iter().map(|(al, _, _)| al).sum(),
            total_freed: a.iter().map(|(_, fr, _)| fr).sum(),
            leak_candidates: leaks,
        }
    });
    let profs = flat_to_profiles(profiles);
    let mut summary = String::new();
    if crash.is_some() {
        summary.push_str("CRASH DETECTED. ");
    }
    if let Some(ref m) = memory {
        if !m.leak_candidates.is_empty() {
            summary.push_str(&format!("{} potential leaks. ", m.leak_candidates.len()));
        }
    }
    if !profs.is_empty() {
        let total: u64 = profs.iter().map(|p| p.duration_ms).sum();
        summary.push_str(&format!("{total}ms in {} spans.", profs.len()));
    }
    if summary.is_empty() {
        summary = "No issues detected.".into();
    }
    DiagnosticReport {
        crash,
        memory,
        profiles: profs,
        summary,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stacktrace_works() {
        let frames = parse_stacktrace("main at src/main.rs:42\nfoo at src/lib.rs:10");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].line, 42);
    }

    #[test]
    fn detect_leak_works() {
        let allocs = &[(1024, 0, "src/leak.rs:1"), (512, 512, "src/ok.rs:1")];
        let leaks = detect_leaks(allocs);
        assert_eq!(leaks.len(), 1);
        assert_eq!(leaks[0].allocation_size, 1024);
    }

    #[test]
    fn report_with_crash() {
        let report = generate_report(Some("fn at src/a.rs:1"), None, &[]);
        assert!(report.summary.contains("CRASH"));
    }

    #[test]
    fn report_clean() {
        assert_eq!(
            generate_report(None, None, &[]).summary,
            "No issues detected."
        );
    }
}
