use std::fmt;
use std::path::Path;
use std::process::Command;

/// Snapshot of git state at the time of a scenario failure.
/// Included in error output to aid debugging.
#[derive(Debug)]
pub struct GitStateSnapshot {
    pub branch: Option<String>,
    pub status: Option<String>,
    pub recent_log: Option<String>,
    pub stack_config: Option<String>,
}

impl GitStateSnapshot {
    /// Capture a snapshot of the current git state in the given repo directory.
    pub fn capture(repo_dir: &Path) -> Self {
        let branch = run_git(repo_dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let status = run_git(repo_dir, &["status", "--porcelain"]);
        let recent_log = run_git(repo_dir, &["log", "--oneline", "-5"]);
        let stack_config = run_git(repo_dir, &["config", "--get-regexp", r"^stack\."]);
        Self {
            branch,
            status,
            recent_log,
            stack_config,
        }
    }
}

impl fmt::Display for GitStateSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref b) = self.branch {
            writeln!(f, "  branch: {b}")?;
        }
        if let Some(ref s) = self.status
            && !s.is_empty()
        {
            writeln!(f, "  status:\n    {}", s.replace('\n', "\n    "))?;
        }
        if let Some(ref l) = self.recent_log {
            writeln!(f, "  log:\n    {}", l.replace('\n', "\n    "))?;
        }
        if let Some(ref c) = self.stack_config
            && !c.is_empty()
        {
            writeln!(f, "  stack config:\n    {}", c.replace('\n', "\n    "))?;
        }
        Ok(())
    }
}

/// Errors from parsing or executing a scenario.
#[derive(Debug)]
pub enum ScenarioError {
    /// Parse error: bad syntax on a specific line.
    Parse {
        line: usize,
        text: String,
        message: String,
    },
    /// Execution error: an operation or assertion failed.
    Exec {
        line: usize,
        text: String,
        message: String,
        snapshot: Option<GitStateSnapshot>,
    },
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScenarioError::Parse {
                line,
                text,
                message,
            } => {
                write!(f, "parse error on line {line}: {message}\n  | {text}")
            }
            ScenarioError::Exec {
                line,
                text,
                message,
                snapshot,
            } => {
                write!(f, "scenario failed on line {line}: {message}\n  | {text}")?;
                if let Some(snap) = snapshot {
                    write!(f, "\ngit state:\n{snap}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ScenarioError {}

fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        None
    }
}
