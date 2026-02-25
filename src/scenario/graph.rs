//! Git graph types and Mermaid rendering for scenario documentation.
//!
//! The core types ([`Commit`], [`Branch`], [`GitGraph`]) represent a branch
//! tree in DFS order. [`Mermaid`] wraps a graph for `Display` as Mermaid
//! gitGraph syntax, which renders natively in VS Code, GitHub, and on PRs.
//!
//! [`GitGraph::from_git_dir`] reconstructs the branch tree by shelling out
//! to git commands, inferring parent relationships from the commit DAG
//! (since git-stack doesn't store explicit parent config keys).

use std::fmt;
use std::path::Path;
use std::process::Command;

/// A single commit on a branch.
#[derive(Default)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    /// Optional Mermaid tag (e.g. `"HEAD"`). Rendered as `tag: "HEAD"` in the
    /// gitGraph output, which places a labeled arrow on the commit node.
    pub tag: Option<String>,
}

impl Commit {
    pub fn new(sha: &str, subject: &str) -> Self {
        Self {
            sha: sha.to_owned(),
            subject: subject.to_owned(),
            ..Default::default()
        }
    }
}

/// A branch and its own commits (not inherited from parent).
pub struct Branch {
    pub name: String,
    pub parent: Option<String>,
    pub commits: Vec<Commit>,
}

/// Complete graph: trunk + branches in DFS traversal order.
pub struct GitGraph {
    pub trunk: String,
    pub head: Option<String>,
    pub branches: Vec<Branch>,
}

impl GitGraph {
    /// Build a `GitGraph` by inspecting the git repo at `dir`.
    ///
    /// Reconstructs the branch tree from the commit DAG: for each non-trunk
    /// branch, finds the closest ancestor branch (by rev-list count) and
    /// treats it as the parent. Then DFS-walks from trunk, collecting each
    /// branch's own commits via `git log --reverse`.
    ///
    /// This is O(n^2) in branches, but scenario repos have at most 5-10
    /// branches, so performance is irrelevant.
    pub fn from_git_dir(dir: &Path) -> Result<Self, String> {
        let trunk = find_trunk(dir);
        let all_branches = find_branches(dir)?;

        // Non-trunk branches: find each one's parent
        let non_trunk: Vec<&str> = all_branches
            .iter()
            .filter(|b| b.as_str() != trunk)
            .map(|s| s.as_str())
            .collect();

        // Build parent map: branch -> parent branch name
        let mut parent_map: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
        for &branch in &non_trunk {
            // Candidates: trunk + all other non-trunk branches that aren't this one
            let mut candidates: Vec<&str> = vec![trunk.as_str()];
            for &other in &non_trunk {
                if other != branch {
                    candidates.push(other);
                }
            }
            if let Some(parent) = find_parent(dir, branch, &candidates) {
                parent_map.insert(branch, parent);
            }
        }

        // Build children map for DFS
        let mut children: std::collections::HashMap<&str, Vec<&str>> =
            std::collections::HashMap::new();
        for (&branch, &parent) in &parent_map {
            children.entry(parent).or_default().push(branch);
        }
        // Sort children for deterministic output
        for kids in children.values_mut() {
            kids.sort();
        }

        // DFS from trunk
        let mut branches = Vec::new();
        let mut stack = vec![trunk.as_str()];
        while let Some(name) = stack.pop() {
            let parent = parent_map.get(name).map(|p| p.to_string());
            let is_trunk = parent.is_none();

            let commits = if is_trunk {
                let output = git_output(dir, &["log", "--reverse", "--format=%h %s", &trunk]);
                output.map(|o| parse_log_output(&o)).unwrap_or_default()
            } else {
                let range = format!("{}..{}", parent.as_ref().unwrap(), name);
                let output = git_output(dir, &["log", "--reverse", "--format=%h %s", &range]);
                output.map(|o| parse_log_output(&o)).unwrap_or_default()
            };

            branches.push(Branch {
                name: name.to_owned(),
                parent,
                commits,
            });

            // Push children in reverse order so first child is processed first
            if let Some(kids) = children.get(name) {
                for &child in kids.iter().rev() {
                    stack.push(child);
                }
            }
        }

        let head = git_output(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);

        // Tag the tip commit of the HEAD branch
        if let Some(tip) = head
            .as_deref()
            .and_then(|h| branches.iter_mut().find(|b| b.name == h))
            .and_then(|b| b.commits.last_mut())
        {
            tip.tag = Some("HEAD".to_owned());
        }

        Ok(GitGraph {
            trunk,
            head,
            branches,
        })
    }
}

/// Run a git command in `dir` and return trimmed stdout, or None on failure.
fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Get the trunk branch name from git config, falling back to "main".
fn find_trunk(dir: &Path) -> String {
    git_output(dir, &["config", "--get", "stack.protected-branch"])
        .unwrap_or_else(|| "main".to_owned())
}

/// List all local branch names.
fn find_branches(dir: &Path) -> Result<Vec<String>, String> {
    let output = git_output(
        dir,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )
    .ok_or_else(|| "failed to list branches".to_owned())?;
    Ok(output.lines().map(|l| l.to_owned()).collect())
}

/// Find the parent of `branch` among `candidates`: the candidate whose tip
/// is an ancestor of `branch`'s tip with the smallest rev-list count.
fn find_parent<'a>(dir: &Path, branch: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let mut best: Option<(&'a str, usize)> = None;

    for &candidate in candidates {
        // Check if candidate is an ancestor of branch
        let is_ancestor = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["merge-base", "--is-ancestor", candidate, branch])
            .output()
            .is_ok_and(|o| o.status.success());

        if !is_ancestor {
            continue;
        }

        let range = format!("{candidate}..{branch}");
        if let Some(count_str) = git_output(dir, &["rev-list", "--count", &range]) {
            if let Ok(count) = count_str.parse::<usize>() {
                if best.as_ref().is_none_or(|(_, c)| count < *c) {
                    best = Some((candidate, count));
                }
            }
        }
    }

    best.map(|(name, _)| name)
}

/// Parse `git log --format="%h %s"` output into Commit structs.
fn parse_log_output(output: &str) -> Vec<Commit> {
    if output.is_empty() {
        return Vec::new();
    }
    output
        .lines()
        .filter_map(|line| {
            let (sha, subject) = line.split_once(' ')?;
            Some(Commit::new(sha, subject))
        })
        .collect()
}

/// How to label commits in rendered output.
pub enum LabelStyle {
    /// Commit message only (deterministic; for scenario docs).
    MessageOnly,
    /// "abcdef1 subject" (for live repos).
    ShaAndMessage,
}

/// Wraps a `GitGraph` for Display as Mermaid gitGraph syntax.
///
/// Note: Mermaid's gitGraph always branches from the current HEAD of the
/// active branch; you can't attach a branch to an arbitrary earlier commit.
/// Since `GitGraph::from_git_dir` captures the current repo state (where all
/// branches are already rebased onto their parents), the DFS emission order
/// naturally produces correct diagrams. This limitation only matters for
/// hand-authored diagrams that try to show "before restack" drift.
pub struct Mermaid<'a> {
    graph: &'a GitGraph,
    style: LabelStyle,
}

impl<'a> Mermaid<'a> {
    pub fn new(graph: &'a GitGraph, style: LabelStyle) -> Self {
        Self { graph, style }
    }

    fn format_label(&self, commit: &Commit) -> String {
        match &self.style {
            LabelStyle::MessageOnly => commit.subject.clone(),
            LabelStyle::ShaAndMessage => format!("{} {}", commit.sha, commit.subject),
        }
    }
}

impl fmt::Display for Mermaid<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "gitGraph")?;

        let mut current_branch: Option<&str> = None;

        for branch in &self.graph.branches {
            let is_trunk = branch.parent.is_none();

            if !is_trunk {
                // If we need to checkout the parent first
                let parent = branch.parent.as_ref().unwrap();
                if current_branch != Some(parent.as_str()) {
                    writeln!(f, "  checkout {parent}")?;
                }
                writeln!(f, "  branch {}", branch.name)?;
                current_branch = Some(&branch.name);
            } else {
                current_branch = Some(&branch.name);
            }

            if branch.commits.is_empty() && !is_trunk {
                writeln!(f, "  commit id: \"(no commits)\"")?;
            } else {
                for commit in &branch.commits {
                    let label = self.format_label(commit);
                    if let Some(ref tag) = commit.tag {
                        writeln!(f, "  commit id: \"{label}\" tag: \"{tag}\"")?;
                    } else {
                        writeln!(f, "  commit id: \"{label}\"")?;
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = "main";
    const FEATURE_A: &str = "feature-A";
    const FEATURE_A1: &str = "feature-A1";
    const FEATURE_B: &str = "feature-B";

    fn branch(name: &str, parent: Option<&str>, commits: Vec<Commit>) -> Branch {
        Branch {
            name: name.to_owned(),
            parent: parent.map(|p| p.to_owned()),
            commits,
        }
    }

    /// Helper: build a GitGraph by hand (no git repo needed).
    fn linear_stack() -> GitGraph {
        GitGraph {
            trunk: MAIN.to_owned(),
            head: None,
            branches: vec![
                branch(MAIN, None, vec![Commit::new("abc1234", "initial")]),
                branch(
                    FEATURE_A,
                    Some(MAIN),
                    vec![
                        Commit::new("def5678", "A work 1"),
                        Commit::new("ghi9012", "A work 2"),
                    ],
                ),
                branch(
                    FEATURE_B,
                    Some(FEATURE_A),
                    vec![
                        Commit::new("jkl3456", "B work 1"),
                        Commit::new("mno7890", "B work 2"),
                    ],
                ),
            ],
        }
    }

    #[test]
    fn test_mermaid_linear_stack() {
        let graph = linear_stack();
        let output = Mermaid::new(&graph, LabelStyle::MessageOnly).to_string();

        let expected = "\
gitGraph
  commit id: \"initial\"
  branch feature-A
  commit id: \"A work 1\"
  commit id: \"A work 2\"
  branch feature-B
  commit id: \"B work 1\"
  commit id: \"B work 2\"
";

        assert_eq!(output, expected);

        // No unnecessary checkout in a linear stack
        assert!(!output.contains("checkout"));
    }

    #[test]
    fn test_mermaid_branching_stack() {
        let graph = GitGraph {
            trunk: MAIN.to_owned(),
            head: None,
            branches: vec![
                branch(MAIN, None, vec![Commit::new("abc1234", "initial")]),
                branch(FEATURE_A, Some(MAIN), vec![Commit::new("def5678", "A work")]),
                branch(FEATURE_B, Some(MAIN), vec![Commit::new("ghi9012", "B work")]),
            ],
        };

        let output = Mermaid::new(&graph, LabelStyle::MessageOnly).to_string();

        let expected = "\
gitGraph
  commit id: \"initial\"
  branch feature-A
  commit id: \"A work\"
  checkout main
  branch feature-B
  commit id: \"B work\"
";

        assert_eq!(output, expected);
    }

    #[test]
    fn test_mermaid_deep_tree() {
        // trunk -> A -> A1, trunk -> B
        let graph = GitGraph {
            trunk: MAIN.to_owned(),
            head: None,
            branches: vec![
                branch(MAIN, None, vec![Commit::new("abc1234", "initial")]),
                branch(FEATURE_A, Some(MAIN), vec![Commit::new("def5678", "A work")]),
                branch(FEATURE_A1, Some(FEATURE_A), vec![Commit::new("ghi9012", "A1 work")]),
                branch(FEATURE_B, Some(MAIN), vec![Commit::new("jkl3456", "B work")]),
            ],
        };

        let output = Mermaid::new(&graph, LabelStyle::MessageOnly).to_string();

        // After A1, needs to checkout main to branch B
        assert!(output.contains("checkout main\n  branch feature-B"));
    }

    #[test]
    fn test_mermaid_empty_branch() {
        let graph = GitGraph {
            trunk: MAIN.to_owned(),
            head: None,
            branches: vec![
                branch(MAIN, None, vec![Commit::new("abc1234", "initial")]),
                branch("empty", Some(MAIN), vec![]),
            ],
        };

        let output = Mermaid::new(&graph, LabelStyle::MessageOnly).to_string();
        assert!(output.contains("commit id: \"(no commits)\""));
    }

    #[test]
    fn test_label_style_message_only() {
        let graph = linear_stack();
        let output = Mermaid::new(&graph, LabelStyle::MessageOnly).to_string();
        // Should NOT contain SHAs
        assert!(!output.contains("abc1234"));
        assert!(output.contains("\"initial\""));
    }

    #[test]
    fn test_label_style_sha_and_message() {
        let graph = linear_stack();
        let output = Mermaid::new(&graph, LabelStyle::ShaAndMessage).to_string();
        // Should contain SHAs
        assert!(output.contains("abc1234 initial"));
    }

    #[test]
    fn test_parse_log_output() {
        let output = "abc1234 first commit\ndef5678 second commit";
        let commits = parse_log_output(output);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].sha, "abc1234");
        assert_eq!(commits[0].subject, "first commit");
        assert_eq!(commits[1].sha, "def5678");
        assert_eq!(commits[1].subject, "second commit");
    }

    #[test]
    fn test_parse_log_output_empty() {
        assert!(parse_log_output("").is_empty());
    }

    #[test]
    fn test_mermaid_head_tag() {
        let graph = GitGraph {
            trunk: MAIN.to_owned(),
            head: Some(FEATURE_A.to_owned()),
            branches: vec![
                branch(MAIN, None, vec![Commit::new("abc1234", "initial")]),
                branch(FEATURE_A, Some(MAIN), vec![Commit {
                    tag: Some("HEAD".to_owned()),
                    ..Commit::new("def5678", "A work")
                }]),
            ],
        };

        let output = Mermaid::new(&graph, LabelStyle::MessageOnly).to_string();

        let expected = "\
gitGraph
  commit id: \"initial\"
  branch feature-A
  commit id: \"A work\" tag: \"HEAD\"
";

        assert_eq!(output, expected);
    }
}
