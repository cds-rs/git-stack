//! Generate mermaid git graph diagrams for .scenario.md files.
//!
//! Runs each scenario's SETUP and IT blocks, captures the repo state as mermaid
//! gitGraph diagrams, and either prints them or rewrites the .scenario.md files
//! with interleaved `<details>` blocks.
//!
//! Usage:
//!   cargo run --features scenario --bin scenario-graph -- [--update] [FILE...]
//!
//! With no files, processes all .scenario.md files in tests/scenarios/.
//! With --update, rewrites files with interleaved scenario blocks and
//! per-section mermaid diagrams wrapped in `<details>`.
//! Without --update, prints mermaid graphs to stdout.

use std::path::{Path, PathBuf};

use git_stack::scenario::graph::{GitGraph, LabelStyle, Mermaid};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let update = args.iter().any(|a| a == "--update");
    let files: Vec<&str> = args
        .iter()
        .filter(|a| *a != "--update")
        .map(|s| s.as_str())
        .collect();

    let binary = find_binary();

    let scenario_files = if files.is_empty() {
        discover_scenarios()
    } else {
        files.iter().map(PathBuf::from).collect()
    };

    if scenario_files.is_empty() {
        eprintln!("No .scenario.md files found.");
        std::process::exit(1);
    }

    for path in &scenario_files {
        let content = std::fs::read_to_string(path).expect("failed to read scenario file");
        // foo.scenario.md -> file_stem gives "foo.scenario", strip ".scenario"
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let name = stem.strip_suffix(".scenario").unwrap_or(stem);

        if update {
            match process_interleaved(&content, name, &binary) {
                Ok(new_content) => {
                    std::fs::write(path, new_content).expect("failed to write scenario file");
                    eprintln!("Updated {}", path.display());
                }
                Err(e) => {
                    eprintln!("SKIP {}: {e}", path.display());
                }
            }
        } else {
            match process_stdout(&content, name, &binary) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("SKIP {}: {e}", path.display());
                }
            }
        }
    }
}

/// Print mermaid graphs to stdout for each section (SETUP + IT blocks).
fn process_stdout(content: &str, name: &str, binary: &Path) -> Result<(), String> {
    let scenario =
        git_stack::scenario::parse(content, name).map_err(|e| format!("{e}"))?;

    let mut setup_runner = git_stack::scenario::ScenarioRunner::new(binary);
    setup_runner
        .execute_setup(&scenario.setup)
        .map_err(|e| format!("setup failed: {e}"))?;
    let setup_graph = build_mermaid_graph(&setup_runner);

    println!("=== {name}: SETUP ===");
    println!("{setup_graph}");

    for test_block in &scenario.tests {
        let mut runner = git_stack::scenario::ScenarioRunner::new(binary);
        if runner.execute_setup(&scenario.setup).is_err() {
            continue;
        }
        let _ = runner.execute_ops_only(&test_block.statements);
        let graph = build_mermaid_graph(&runner);

        if graph != setup_graph {
            println!();
            println!("=== {name}: \"{}\" ===", test_block.name);
            println!("{graph}");
        }
    }
    println!();
    Ok(())
}

// ---- Interleaved document generation (--update --mermaid) ----

/// An IT section extracted from the raw DSL lines.
struct ItSection {
    name: String,
    lines: Vec<String>,
}

/// Captured result of running an IT block's operations.
#[derive(Default)]
struct ItResult {
    /// Mermaid graph string, or None if identical to SETUP graph.
    graph: Option<String>,
    /// Error context from the last command, if it failed.
    error: Option<OpError>,
}

/// Error context captured after running an IT block.
struct OpError {
    exit_code: i32,
    stderr: String,
}

/// Build the interleaved multi-block document for a single scenario file.
///
/// Parses the file, runs SETUP and each IT block's ops in isolated runners,
/// captures mermaid graphs, and assembles the output with `<details>` blocks.
fn process_interleaved(content: &str, name: &str, binary: &Path) -> Result<String, String> {
    let scenario =
        git_stack::scenario::parse(content, name).map_err(|e| format!("{e}"))?;

    let header = extract_header(content);
    let dsl_lines = extract_all_dsl_lines(content);
    let (setup_lines, it_sections) = split_dsl_sections(&dsl_lines);

    // Run SETUP, capture mermaid graph
    let mut setup_runner = git_stack::scenario::ScenarioRunner::new(binary);
    setup_runner
        .execute_setup(&scenario.setup)
        .map_err(|e| format!("setup failed: {e}"))?;
    let setup_graph = build_mermaid_graph(&setup_runner);

    // For each IT block: fresh runner, run SETUP + IT ops, capture graph.
    // If the graph matches SETUP and no errors occurred, we skip the <details> section.
    let mut it_entries: Vec<(&ItSection, ItResult)> = Vec::new();
    for (section, test_block) in it_sections.iter().zip(scenario.tests.iter()) {
        let mut runner = git_stack::scenario::ScenarioRunner::new(binary);
        if runner.execute_setup(&scenario.setup).is_err() {
            it_entries.push((section, ItResult::default()));
            continue;
        }

        let _ = runner.execute_ops_only(&test_block.statements);
        let graph = build_mermaid_graph(&runner);

        let error = (runner.last_exit() != 0).then(|| OpError {
            exit_code: runner.last_exit(),
            stderr: sanitize_paths(runner.last_stderr()),
        });

        let graph = (graph != setup_graph || error.is_some()).then_some(graph);

        it_entries.push((section, ItResult { graph, error }));
    }

    Ok(build_document(
        &header,
        &setup_lines,
        &setup_graph,
        &it_entries,
    ))
}

/// Extract the prose header: everything before the first fenced block or
/// `<details>` tag. Trailing blank lines are trimmed.
fn extract_header(content: &str) -> String {
    let mut header_lines: Vec<&str> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "```scenario" || trimmed == "```mermaid" || trimmed == "<details>" {
            break;
        }
        header_lines.push(line);
    }
    while header_lines
        .last()
        .is_some_and(|l| l.trim().is_empty())
    {
        header_lines.pop();
    }
    let mut result = String::new();
    for line in &header_lines {
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Collect all raw DSL lines from every ` ```scenario ` block in the file.
fn extract_all_dsl_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if !inside {
            if trimmed == "```scenario" {
                inside = true;
            }
        } else if trimmed == "```" {
            inside = false;
        } else {
            lines.push(line.to_owned());
        }
    }
    lines
}

/// Split concatenated DSL lines into SETUP lines and per-IT sections.
fn split_dsl_sections(dsl_lines: &[String]) -> (Vec<String>, Vec<ItSection>) {
    let mut setup_lines = Vec::new();
    let mut it_blocks: Vec<ItSection> = Vec::new();
    let mut current_it: Option<ItSection> = None;

    for line in dsl_lines {
        let trimmed = line.trim();
        if let Some(after_it) = trimmed.strip_prefix("IT ") {
            if let Some(mut block) = current_it.take() {
                trim_trailing_blanks(&mut block.lines);
                it_blocks.push(block);
            }
            let rest = after_it.trim();
            let name = extract_it_name(rest);
            current_it = Some(ItSection {
                name,
                lines: vec![line.clone()],
            });
        } else if let Some(ref mut block) = current_it {
            block.lines.push(line.clone());
        } else {
            setup_lines.push(line.clone());
        }
    }
    if let Some(mut block) = current_it {
        trim_trailing_blanks(&mut block.lines);
        it_blocks.push(block);
    }
    trim_trailing_blanks(&mut setup_lines);
    (setup_lines, it_blocks)
}

/// Extract the name from an IT header line: `"some name"` -> `some name`.
fn extract_it_name(rest: &str) -> String {
    if let Some(after_quote) = rest.strip_prefix('"') {
        after_quote
            .find('"')
            .map(|end| after_quote[..end].to_owned())
            .unwrap_or_default()
    } else {
        rest.to_owned()
    }
}

fn trim_trailing_blanks(lines: &mut Vec<String>) {
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
}

/// Wrap content in a fenced code block with the given language tag.
/// Ensures a newline before the closing fence even if `content` doesn't end with one.
fn fenced_block(lang: &str, content: &str) -> String {
    let sep = if content.ends_with('\n') { "" } else { "\n" };
    format!("```{lang}\n{content}{sep}```\n")
}

/// Wrap DSL lines in a ` ```scenario ` fenced code block.
fn format_scenario_block(lines: &[String]) -> String {
    let mut content = String::new();
    for line in lines {
        content.push_str(line);
        content.push('\n');
    }
    fenced_block("scenario", &content)
}

/// Wrap a mermaid graph in a `<details>` block with the given summary.
/// If `error` is provided, appends a console block with the error details.
fn format_details_block(summary: &str, mermaid_content: &str, error: Option<&OpError>) -> String {
    let failed = if error.is_some() { " (FAILED)" } else { "" };
    let mut out = format!(
        "<details>\n<summary>{summary}{failed}</summary>\n\n{}",
        fenced_block("mermaid", mermaid_content),
    );
    if let Some(err) = error {
        let mut console = format!("$ exit code: {}\n", err.exit_code);
        if !err.stderr.is_empty() {
            console.push_str(&err.stderr);
            if !err.stderr.ends_with('\n') {
                console.push('\n');
            }
        }
        out.push('\n');
        out.push_str(&fenced_block("console", &console));
    }
    out.push_str("\n</details>\n");
    out
}

/// Assemble the full interleaved markdown document.
fn build_document(
    header: &str,
    setup_lines: &[String],
    setup_graph: &str,
    it_entries: &[(&ItSection, ItResult)],
) -> String {
    let mut out = String::new();
    out.push_str(header);
    out.push('\n');
    out.push_str(&format_scenario_block(setup_lines));
    out.push('\n');
    out.push_str(&format_details_block("After SETUP", setup_graph, None));

    for (section, result) in it_entries {
        out.push('\n');
        out.push_str(&format_scenario_block(&section.lines));
        if let Some(ref g) = result.graph {
            out.push('\n');
            out.push_str(&format_details_block(
                &format!("After \"{}\"", section.name),
                g,
                result.error.as_ref(),
            ));
        }
    }

    out
}

// ---- Path sanitization ----

/// Replace the user's home directory with `~` in stderr output so that
/// stack traces committed to the repo don't leak machine-specific paths.
fn sanitize_paths(text: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => text.replace(&home, "~"),
        _ => text.to_owned(),
    }
}

// ---- Shared helpers ----

/// Build a Mermaid gitGraph from the scenario runner's repo state.
fn build_mermaid_graph(runner: &git_stack::scenario::ScenarioRunner) -> String {
    let graph = match GitGraph::from_git_dir(runner.repo_dir()) {
        Ok(g) => g,
        Err(e) => return format!("%% error building graph: {e}"),
    };
    Mermaid::new(&graph, LabelStyle::MessageOnly).to_string()
}

/// Find the git-stack binary. Prefers the debug build next to us.
fn find_binary() -> PathBuf {
    // When run via `cargo run --bin scenario-graph`, we're in target/debug/.
    // The main binary is alongside us.
    let self_path = std::env::current_exe().expect("can't find self");
    let dir = self_path.parent().unwrap();
    let candidate = dir.join("git-stack");
    if candidate.exists() {
        return candidate;
    }
    // Fallback: look in common cargo target locations
    for rel in &["target/debug/git-stack", "target/release/git-stack"] {
        let p = Path::new(rel);
        if p.exists() {
            return p.to_path_buf();
        }
    }
    panic!("Cannot find git-stack binary. Run `cargo build` first.");
}

/// Discover all .scenario.md files in the conventional location.
fn discover_scenarios() -> Vec<PathBuf> {
    let dir = Path::new("tests/scenarios");
    if dir.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|ext| ext == "md")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.contains(".scenario."))
            })
            .collect();
        files.sort();
        return files;
    }
    Vec::new()
}

