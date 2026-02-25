use std::path::Path;

fn run_scenario(path: &Path) -> datatest_stable::Result<()> {
    let content = std::fs::read_to_string(path)?;
    // foo.scenario.md -> file_stem gives "foo.scenario", strip the ".scenario" suffix
    let stem = path.file_stem().unwrap().to_str().unwrap();
    let name = stem.strip_suffix(".scenario").unwrap_or(stem);
    let scenario = git_stack::scenario::parse(&content, name)?;
    let binary = Path::new(env!("CARGO_BIN_EXE_git-stack"));

    let mut failures: Vec<(String, git_stack::scenario::error::ScenarioError)> = Vec::new();

    for test_block in &scenario.tests {
        let mut runner = git_stack::scenario::ScenarioRunner::new(binary);
        if let Err(e) = runner.execute_statements(&scenario.setup) {
            failures.push((test_block.name.clone(), e));
            continue;
        }
        if let Err(e) = runner.execute_statements(&test_block.statements) {
            failures.push((test_block.name.clone(), e));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        let mut msg = format!("{} IT block(s) failed:\n", failures.len());
        for (block_name, err) in &failures {
            msg.push_str(&format!("\n  IT \"{block_name}\":\n    {err}\n"));
        }
        Err(msg.into())
    }
}

datatest_stable::harness!(run_scenario, "tests/scenarios", r"\.scenario\.md$");
