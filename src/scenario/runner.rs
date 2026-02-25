use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::scenario::error::{GitStateSnapshot, ScenarioError};
use crate::scenario::types::{Assertion, Located, Operation, Statement};

/// Executes a parsed scenario against a temporary git repo.
pub struct ScenarioRunner {
    _work_dir: TempDir,
    active_dir: PathBuf,
    binary_path: PathBuf,
    last_stdout: String,
    last_stderr: String,
    last_exit: i32,
}

impl ScenarioRunner {
    pub fn new(binary: &Path) -> Self {
        let work_dir = tempfile::tempdir().expect("failed to create temp dir");
        let active_dir = work_dir.path().to_path_buf();
        Self {
            _work_dir: work_dir,
            active_dir,
            binary_path: binary.to_path_buf(),
            last_stdout: String::new(),
            last_stderr: String::new(),
            last_exit: 0,
        }
    }

    pub fn repo_dir(&self) -> &Path {
        &self.active_dir
    }

    pub fn last_exit(&self) -> i32 {
        self.last_exit
    }

    pub fn last_stderr(&self) -> &str {
        &self.last_stderr
    }

    /// Execute setup statements, stopping at the first DONE.
    /// Useful for setting up a repo to inspect its graph for documentation.
    pub fn execute_setup(&mut self, stmts: &[Located<Statement>]) -> Result<(), ScenarioError> {
        for stmt in stmts {
            match &stmt.inner {
                Statement::Op(Operation::Done) => return Ok(()),
                Statement::Op(op) => {
                    self.exec_op(op, stmt.line, &stmt.text)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Capture the current git graph as a decorated oneline log.
    /// Useful for generating documentation diagrams.
    pub fn git_graph(&self) -> String {
        self.git_output(&[
            "log",
            "--graph",
            "--oneline",
            "--all",
            "--decorate",
            "--decorate-refs=refs/heads/",
        ])
        .unwrap_or_else(|| "(no commits)".to_owned())
    }

    /// Execute only operations (Op and `TryOp`) from a list of statements,
    /// skipping assertions, comments, and blanks. Used by the graph generator
    /// to apply IT block mutations without running assertions.
    pub fn execute_ops_only(
        &mut self,
        stmts: &[Located<Statement>],
    ) -> Result<(), ScenarioError> {
        for stmt in stmts {
            match &stmt.inner {
                Statement::Op(op) => self.exec_op(op, stmt.line, &stmt.text)?,
                Statement::TryOp(op) => self.exec_op_try(op),
                _ => {}
            }
        }
        Ok(())
    }

    /// Execute a list of statements (used by the harness for both setup and IT blocks).
    pub fn execute_statements(
        &mut self,
        stmts: &[Located<Statement>],
    ) -> Result<(), ScenarioError> {
        for stmt in stmts {
            match &stmt.inner {
                Statement::Comment(_) | Statement::Blank => {}
                Statement::Op(op) => {
                    self.exec_op(op, stmt.line, &stmt.text)?;
                }
                Statement::TryOp(op) => {
                    self.exec_op_try(op);
                }
                Statement::Assert(assertion) => {
                    self.exec_assert(assertion, stmt.line, &stmt.text)?;
                }
            }
        }
        Ok(())
    }

    #[allow(unused_variables)]
    fn exec_op(&mut self, op: &Operation, line: usize, text: &str) -> Result<(), ScenarioError> {
        match op {
            Operation::Init { no_commit } => self.exec_init(*no_commit),
            Operation::Done => Ok(()),
            Operation::Create { name, on } => self.exec_create(name, on.as_deref()),
            Operation::Commit { count, message } => {
                self.exec_commit(*count, message.as_deref(), line)
            }
            Operation::Checkout { branch } => self.exec_checkout(branch),
            Operation::Sync => self.exec_sync(),
            Operation::Stack {
                rebase,
                push,
                repair,
                fixup,
                format,
            } => self.exec_stack(*rebase, *push, *repair, fixup.as_deref(), format.as_deref()),
            Operation::Amend {
                message,
                all,
                target,
            } => self.exec_amend(message.as_deref(), *all, target.as_deref()),
            Operation::WriteFile { path, content } => self.exec_write_file(path, content),
            Operation::Stage { path } => self.exec_stage(path),
            Operation::Reword { message, target } => {
                self.exec_reword(message, target.as_deref())
            }
            Operation::Next { count, branch } => self.exec_next(*count, *branch),
            Operation::Prev { count, branch } => self.exec_prev(*count, *branch),
            Operation::Run { args } => self.exec_run(args),
            Operation::Protect { glob } => self.exec_protect(glob),
            Operation::Git { args } => self.exec_git(args),

            Operation::DeleteBranch { branch } => self.exec_delete_branch(branch),
        }
    }

    /// Execute an operation without aborting on failure (for `try` prefix).
    /// Delegates to `exec_op` and discards any error; the last exit code and
    /// stderr are still captured by `run_binary_raw` before errors propagate.
    fn exec_op_try(&mut self, op: &Operation) {
        let _ = self.exec_op(op, 0, "");
    }

    fn exec_init(&mut self, no_commit: bool) -> Result<(), ScenarioError> {
        self.git(&["init", "-b", "main"])?;
        self.git(&["config", "user.email", "test@scenario.test"])?;
        self.git(&["config", "user.name", "Scenario Test"])?;
        if !no_commit {
            self.git(&["commit", "--allow-empty", "-m", "initial"])?;
            self.run_binary(&["--protect", "main"])?;
        }
        Ok(())
    }

    fn exec_create(&mut self, name: &str, on: Option<&str>) -> Result<(), ScenarioError> {
        if let Some(parent) = on {
            self.git(&["checkout", parent])?;
        }
        self.git(&["switch", "-c", name])?;
        Ok(())
    }

    fn exec_commit(
        &mut self,
        count: u32,
        message: Option<&str>,
        line: usize,
    ) -> Result<(), ScenarioError> {
        for i in 0..count {
            let filename = format!("commit-{line}-{i}.txt");
            let msg = message
                .map(|m| m.to_owned())
                .unwrap_or_else(|| format!("commit {} on line {line}", i + 1));
            self.write_file(&filename, &format!("content {i}"))?;
            self.git(&["add", &filename])?;
            self.git(&["commit", "-m", &msg])?;
        }
        Ok(())
    }

    fn exec_checkout(&mut self, branch: &str) -> Result<(), ScenarioError> {
        self.git(&["checkout", branch])?;
        Ok(())
    }

    fn exec_sync(&mut self) -> Result<(), ScenarioError> {
        self.run_binary(&["sync"])?;
        Ok(())
    }

    fn build_stack_args<'a>(
        rebase: bool,
        push: bool,
        repair: bool,
        fixup: Option<&'a str>,
        format: Option<&'a str>,
    ) -> Vec<&'a str> {
        let mut args: Vec<&str> = Vec::new();
        if rebase {
            args.push("--rebase");
        }
        if push {
            args.push("--push");
        }
        if repair {
            args.push("--repair");
        }
        if let Some(action) = fixup {
            args.push("--fixup");
            args.push(action);
        }
        if let Some(fmt) = format {
            args.push("--format");
            args.push(fmt);
        }
        args
    }

    fn exec_stack(
        &mut self,
        rebase: bool,
        push: bool,
        repair: bool,
        fixup: Option<&str>,
        format: Option<&str>,
    ) -> Result<(), ScenarioError> {
        let args = Self::build_stack_args(rebase, push, repair, fixup, format);
        self.run_binary(&args)?;
        Ok(())
    }

    fn build_amend_args<'a>(
        message: Option<&'a str>,
        all: bool,
        target: Option<&'a str>,
    ) -> Vec<&'a str> {
        let mut args = vec!["amend"];
        if all {
            args.push("--all");
        }
        if let Some(m) = message {
            args.push("--message");
            args.push(m);
        }
        if let Some(t) = target {
            args.push(t);
        }
        args
    }

    fn exec_amend(
        &mut self,
        message: Option<&str>,
        all: bool,
        target: Option<&str>,
    ) -> Result<(), ScenarioError> {
        let args = Self::build_amend_args(message, all, target);
        self.run_binary(&args)?;
        Ok(())
    }

    fn exec_write_file(&self, path: &str, content: &str) -> Result<(), ScenarioError> {
        self.write_file(path, content)
    }

    fn exec_stage(&mut self, path: &str) -> Result<(), ScenarioError> {
        self.git(&["add", path])?;
        Ok(())
    }

    fn exec_reword(&mut self, message: &str, target: Option<&str>) -> Result<(), ScenarioError> {
        let mut args = vec!["reword", "--message", message];
        if let Some(t) = target {
            args.push(t);
        }
        self.run_binary(&args)?;
        Ok(())
    }

    fn exec_next(&mut self, count: u32, branch: bool) -> Result<(), ScenarioError> {
        let count_str = count.to_string();
        let mut args = vec!["next", &count_str];
        if branch {
            args.push("--branch");
        }
        self.run_binary(&args)?;
        Ok(())
    }

    fn exec_prev(&mut self, count: u32, branch: bool) -> Result<(), ScenarioError> {
        let count_str = count.to_string();
        let mut args = vec!["previous", &count_str];
        if branch {
            args.push("--branch");
        }
        self.run_binary(&args)?;
        Ok(())
    }

    fn exec_run(&mut self, args: &[String]) -> Result<(), ScenarioError> {
        let mut cmd_args = vec!["run"];
        cmd_args.extend(args.iter().map(|s| s.as_str()));
        self.run_binary(&cmd_args)?;
        Ok(())
    }

    fn exec_protect(&mut self, glob: &str) -> Result<(), ScenarioError> {
        self.run_binary(&["--protect", glob])?;
        Ok(())
    }

    fn exec_git(&mut self, args: &[String]) -> Result<(), ScenarioError> {
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.git(&refs)?;
        Ok(())
    }

    fn exec_delete_branch(&mut self, branch: &str) -> Result<(), ScenarioError> {
        self.git(&["branch", "-D", branch])?;
        Ok(())
    }

    fn exec_assert(
        &mut self,
        assertion: &Assertion,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        match assertion {
            Assertion::Ancestor {
                ancestor,
                descendant,
            } => self.assert_ancestor(ancestor, descendant, line, text),
            Assertion::On(expected) => self.assert_on(expected, line, text),
            Assertion::Clean => self.assert_clean(line, text),
            Assertion::Exists(branch) => self.assert_exists(branch, line, text),
            Assertion::NotExists(branch) => self.assert_not_exists(branch, line, text),
            Assertion::OutputContains(needle) => self.assert_output_contains(needle, line, text),
            Assertion::StderrContains(needle) => self.assert_stderr_contains(needle, line, text),
            Assertion::Fails(args) => self.assert_fails(args, line, text),
            Assertion::CommitMessage(expected) => self.assert_commit_message(expected, line, text),
            Assertion::BranchCount(expected) => self.assert_branch_count(*expected, line, text),
            Assertion::ExitCode(expected) => self.assert_exit_code(*expected, line, text),
        }
    }

    fn assert_ancestor(
        &self,
        ancestor: &str,
        descendant: &str,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        let result = Command::new("git")
            .arg("-C")
            .arg(self.repo_dir())
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .output()
            .ok();
        match result {
            Some(out) if out.status.success() => Ok(()),
            _ => self.fail(
                line,
                text,
                &format!("{ancestor} is not an ancestor of {descendant}"),
            ),
        }
    }

    fn assert_on(&self, expected: &str, line: usize, text: &str) -> Result<(), ScenarioError> {
        let actual = self
            .git_output(&["rev-parse", "--abbrev-ref", "HEAD"])
            .unwrap_or_default();
        if actual != expected {
            return self.fail(
                line,
                text,
                &format!("expected current branch {expected}, got {actual}"),
            );
        }
        Ok(())
    }

    fn assert_clean(&self, line: usize, text: &str) -> Result<(), ScenarioError> {
        let status = self
            .git_output(&["status", "--porcelain"])
            .unwrap_or_default();
        if !status.is_empty() {
            return self.fail(line, text, &format!("expected clean tree, got:\n{status}"));
        }
        Ok(())
    }

    fn assert_exists(&self, branch: &str, line: usize, text: &str) -> Result<(), ScenarioError> {
        let result = self.git_output(&["rev-parse", "--verify", &format!("refs/heads/{branch}")]);
        if result.is_none() {
            return self.fail(line, text, &format!("branch {branch} does not exist"));
        }
        Ok(())
    }

    fn assert_not_exists(
        &self,
        branch: &str,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        let result = self.git_output(&["rev-parse", "--verify", &format!("refs/heads/{branch}")]);
        if result.is_some() {
            return self.fail(line, text, &format!("branch {branch} exists but shouldn't"));
        }
        Ok(())
    }

    fn assert_output_contains(
        &self,
        needle: &str,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        if !self.last_stdout.contains(needle) {
            return self.fail(
                line,
                text,
                &format!(
                    "stdout does not contain {needle:?}\nstdout was:\n{}",
                    self.last_stdout
                ),
            );
        }
        Ok(())
    }

    fn assert_stderr_contains(
        &self,
        needle: &str,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        if !self.last_stderr.contains(needle) {
            return self.fail(
                line,
                text,
                &format!(
                    "stderr does not contain {needle:?}\nstderr was:\n{}",
                    self.last_stderr
                ),
            );
        }
        Ok(())
    }

    fn assert_fails(
        &mut self,
        args: &[String],
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let _ = self.run_binary_raw(&refs);
        if self.last_exit == 0 {
            return self.fail(
                line,
                text,
                &format!(
                    "expected command to fail, but it exited 0\nstdout: {}\nstderr: {}",
                    self.last_stdout, self.last_stderr
                ),
            );
        }
        Ok(())
    }

    fn assert_commit_message(
        &self,
        expected: &str,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        let actual = self
            .git_output(&["log", "-1", "--format=%s"])
            .unwrap_or_default();
        if actual != expected {
            return self.fail(
                line,
                text,
                &format!("expected commit message {expected:?}, got {actual:?}"),
            );
        }
        Ok(())
    }

    fn assert_branch_count(
        &self,
        expected: u32,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        let output = self.git_output(&["branch", "--list"]).unwrap_or_default();
        let actual = output.lines().count() as u32;
        if actual != expected {
            return self.fail(
                line,
                text,
                &format!("expected {expected} branches, got {actual}\nbranches:\n{output}"),
            );
        }
        Ok(())
    }

    fn assert_exit_code(
        &self,
        expected: i32,
        line: usize,
        text: &str,
    ) -> Result<(), ScenarioError> {
        if self.last_exit != expected {
            return self.fail(
                line,
                text,
                &format!(
                    "expected exit code {expected}, got {}\nstdout: {}\nstderr: {}",
                    self.last_exit, self.last_stdout, self.last_stderr
                ),
            );
        }
        Ok(())
    }

    // -- Helpers --

    /// Run a git command, updating `last_stdout/stderr/exit`.
    fn git(&mut self, args: &[&str]) -> Result<(), ScenarioError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.repo_dir())
            .args(args)
            .output()
            .map_err(|e| ScenarioError::Exec {
                line: 0,
                text: format!("git {}", args.join(" ")),
                message: format!("failed to spawn git: {e}"),
                snapshot: None,
            })?;

        self.last_stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        self.last_stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        self.last_exit = output.status.code().unwrap_or(-1);

        if !output.status.success() {
            return Err(ScenarioError::Exec {
                line: 0,
                text: format!("git {}", args.join(" ")),
                message: format!(
                    "git command failed (exit {}): {}",
                    self.last_exit, self.last_stderr
                ),
                snapshot: Some(GitStateSnapshot::capture(self.repo_dir())),
            });
        }
        Ok(())
    }

    /// Run a git command and return stdout, or None on failure. Does NOT update last_* state.
    fn git_output(&self, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.repo_dir())
            .args(args)
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        } else {
            None
        }
    }

    /// Run the git-stack binary, updating `last_stdout/stderr/exit`.
    /// Returns Ok(()) regardless of exit code (callers check `last_exit`).
    fn run_binary_raw(&mut self, args: &[&str]) -> Result<(), ScenarioError> {
        let output = Command::new(&self.binary_path)
            .args(args)
            .current_dir(self.repo_dir())
            .env("NO_COLOR", "1")
            .env("RUST_BACKTRACE", "1")
            .output()
            .map_err(|e| ScenarioError::Exec {
                line: 0,
                text: format!("git-stack {}", args.join(" ")),
                message: format!("failed to spawn binary: {e}"),
                snapshot: None,
            })?;

        self.last_stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        self.last_stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        self.last_exit = output.status.code().unwrap_or(-1);
        Ok(())
    }

    /// Run the git-stack binary and fail if it exits non-zero.
    fn run_binary(&mut self, args: &[&str]) -> Result<(), ScenarioError> {
        self.run_binary_raw(args)?;
        if self.last_exit != 0 {
            return Err(ScenarioError::Exec {
                line: 0,
                text: format!("git-stack {}", args.join(" ")),
                message: format!(
                    "binary exited {} -- stdout: {} -- stderr: {}",
                    self.last_exit, self.last_stdout, self.last_stderr
                ),
                snapshot: Some(GitStateSnapshot::capture(self.repo_dir())),
            });
        }
        Ok(())
    }

    fn write_file(&self, path: &str, content: &str) -> Result<(), ScenarioError> {
        let full_path = self.repo_dir().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ScenarioError::Exec {
                line: 0,
                text: format!("write {path}"),
                message: format!("failed to create parent dir: {e}"),
                snapshot: None,
            })?;
        }
        fs::write(&full_path, content).map_err(|e| ScenarioError::Exec {
            line: 0,
            text: format!("write {path}"),
            message: format!("failed to write file: {e}"),
            snapshot: None,
        })
    }

    fn fail<T>(&self, line: usize, text: &str, message: &str) -> Result<T, ScenarioError> {
        Err(ScenarioError::Exec {
            line,
            text: text.to_owned(),
            message: message.to_owned(),
            snapshot: Some(GitStateSnapshot::capture(self.repo_dir())),
        })
    }
}
