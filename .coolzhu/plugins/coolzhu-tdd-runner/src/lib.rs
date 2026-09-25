//! TDD Runner Plugin — enforces RED-GREEN-REFACTOR cycle.

// ---------------------------------------------------------------------------
// TDD state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TddPhase {
    /// No active TDD cycle — waiting for a new test to be written.
    Idle,
    /// Test written, must confirm it FAILS (RED).
    Red,
    /// RED confirmed, write minimum implementation (GREEN).
    Green,
    /// All tests GREEN, clean up code (REFACTOR).
    Refactor,
}

/// Status of the last test run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRunResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub is_red: bool,
    pub is_green: bool,
}

#[derive(Debug, Clone)]
pub struct TddRunner {
    phase: TddPhase,
    last_result: Option<TestRunResult>,
}

impl TddRunner {
    pub fn new() -> Self {
        Self {
            phase: TddPhase::Idle,
            last_result: None,
        }
    }

    /// Get the current TDD phase.
    pub fn phase(&self) -> TddPhase {
        self.phase
    }

    /// Suggest the next action based on the current phase.
    pub fn suggest_action(&self) -> &'static str {
        match self.phase {
            TddPhase::Idle => "Write a failing test to start the RED phase.",
            TddPhase::Red => {
                "Run tests NOW — they must FAIL (RED). If tests pass, the test is wrong."
            }
            TddPhase::Green => "Write ONLY the minimum code to pass. No refactoring yet.",
            TddPhase::Refactor => "All tests GREEN. Refactor while keeping tests passing.",
        }
    }

    /// Transition the state machine based on the latest test result.
    pub fn transition(&mut self, result: &TestRunResult) {
        self.last_result = Some(result.clone());

        match (&self.phase, result.is_red, result.is_green) {
            // Start new cycle
            (TddPhase::Idle, _, _) => self.phase = TddPhase::Red,
            // RED confirmed → move to GREEN
            (TddPhase::Red, true, false) => self.phase = TddPhase::Green,
            // GREEN confirmed → move to REFACTOR
            (TddPhase::Green, false, true) => self.phase = TddPhase::Refactor,
            // Still failing → stay in GREEN (or go back to it)
            (_, true, false) => self.phase = TddPhase::Green,
            // Stay in Refactor as long as GREEN
            (TddPhase::Refactor, false, true) => {}
            // Any other → stay put
            _ => {}
        }
    }
}

impl Default for TddRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// File change detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    Test,
    Source,
    Other,
}

/// Classify a file path as test, source, or other.
pub fn classify_file(path: &str) -> FileChangeKind {
    let lower = path.to_lowercase();
    if lower.contains("test")
        || lower.ends_with("_test.rs")
        || lower.contains("__tests__")
        || lower.contains(".test.")
    {
        return FileChangeKind::Test;
    }
    if lower.ends_with(".rs")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".py")
    {
        return FileChangeKind::Source;
    }
    FileChangeKind::Other
}

// ---------------------------------------------------------------------------
// Test framework detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestFramework {
    CargoTest,
    Jest,
    Pytest,
    Unknown,
}

/// Detect the test framework from project files.
pub fn detect_framework(project_files: &[&str]) -> TestFramework {
    for f in project_files {
        let lower = f.to_lowercase();
        if lower.contains("jest.config") || lower.contains("jest") {
            return TestFramework::Jest;
        }
        if lower.contains("conftest.py") || lower.contains("pytest") {
            return TestFramework::Pytest;
        }
    }
    // Heuristic: if any file ends with _test.rs, assume CargoTest
    if project_files.iter().any(|f| f.ends_with("_test.rs")) {
        return TestFramework::CargoTest;
    }
    TestFramework::Unknown
}

/// Build the test command for the detected framework.
pub fn build_test_command(framework: &TestFramework) -> &'static str {
    match framework {
        TestFramework::CargoTest => "cargo test",
        TestFramework::Jest => "npx jest",
        TestFramework::Pytest => "pytest -v",
        TestFramework::Unknown => "echo 'No test framework detected'",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tdd_cycle_red_green_refactor() {
        let mut runner = TddRunner::new();
        assert_eq!(runner.phase(), TddPhase::Idle);

        runner.transition(&TestRunResult {
            total: 0,
            passed: 0,
            failed: 0,
            is_red: false,
            is_green: false,
        });
        assert_eq!(runner.phase(), TddPhase::Red);

        runner.transition(&TestRunResult {
            total: 3,
            passed: 0,
            failed: 3,
            is_red: true,
            is_green: false,
        });
        assert_eq!(runner.phase(), TddPhase::Green);

        runner.transition(&TestRunResult {
            total: 3,
            passed: 3,
            failed: 0,
            is_red: false,
            is_green: true,
        });
        assert_eq!(runner.phase(), TddPhase::Refactor);
    }

    #[test]
    fn classify_test_file() {
        assert_eq!(classify_file("tests/foo_test.rs"), FileChangeKind::Test);
        assert_eq!(
            classify_file("src/__tests__/bar.test.ts"),
            FileChangeKind::Test
        );
        assert_eq!(classify_file("src/main.rs"), FileChangeKind::Source);
        assert_eq!(
            classify_file("components/Button.tsx"),
            FileChangeKind::Source
        );
        assert_eq!(classify_file("README.md"), FileChangeKind::Other);
    }

    #[test]
    fn detect_cargo_framework() {
        assert_eq!(
            detect_framework(&["src/lib.rs", "tests/foo_test.rs"]),
            TestFramework::CargoTest
        );
    }

    #[test]
    fn detect_jest() {
        assert_eq!(
            detect_framework(&["package.json", "jest.config.ts"]),
            TestFramework::Jest
        );
    }

    #[test]
    fn detect_pytest() {
        assert_eq!(
            detect_framework(&["conftest.py", "tests/test_foo.py"]),
            TestFramework::Pytest
        );
    }

    #[test]
    fn suggest_action_for_phases() {
        let runner = TddRunner::new();
        assert!(
            runner.suggest_action().contains("RED"),
            "Idle → suggest RED start"
        );
    }

    #[test]
    fn green_back_to_green_on_failure() {
        let mut runner = TddRunner::new();
        runner.phase = TddPhase::Green;
        runner.transition(&TestRunResult {
            total: 3,
            passed: 1,
            failed: 2,
            is_red: true,
            is_green: false,
        });
        assert_eq!(
            runner.phase(),
            TddPhase::Green,
            "stay in Green if tests fail again"
        );
    }
}
