use bpaf::{OptionParser, Parser, any, construct, literal, long, positional, pure};

use crate::scenario::error::ScenarioError;
use crate::scenario::types::{Assertion, Located, Operation, Scenario, Statement, TestBlock};

// ---- Tokenizer ----

/// Tokenize a DSL line, respecting double-quoted strings.
///
/// `commit 3 times message "fix stuff"` -> `["commit", "3", "times", "message", "fix stuff"]`
///
/// Inside quotes, `\"` produces a literal `"` and `\\` produces a literal `\`.
/// All other `\x` sequences are kept verbatim (the backslash is preserved).
/// Outside quotes, backslashes are always literal.
///
/// Returns an error on unterminated quotes or trailing escapes.
fn tokenize(line: &str) -> Result<Vec<String>, TokenizeError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            match ch {
                '"' | '\\' => current.push(ch),
                _ => {
                    current.push('\\');
                    current.push(ch);
                }
            }
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_quotes => escape = true,
            '\\' => current.push(ch),
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if escape {
        return Err(TokenizeError::TrailingEscape);
    }
    if in_quotes {
        return Err(TokenizeError::UnterminatedQuote);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

#[derive(Debug, Clone)]
enum TokenizeError {
    UnterminatedQuote,
    TrailingEscape,
}

impl std::fmt::Display for TokenizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedQuote => f.write_str("unterminated quoted string"),
            Self::TrailingEscape => f.write_str("trailing escape (\\) at end of line"),
        }
    }
}

/// Tokenize a line and run a bpaf parser on the result, mapping errors to `ScenarioError`.
fn run_bpaf<T>(
    parser: OptionParser<T>,
    line: &str,
    line_num: usize,
    text: &str,
) -> Result<T, ScenarioError> {
    let tokens = tokenize(line).map_err(|e| ScenarioError::Parse {
        line: line_num,
        text: text.to_owned(),
        message: e.to_string(),
    })?;
    let args: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
    parser
        .run_inner(args.as_slice())
        .map_err(|e| ScenarioError::Parse {
            line: line_num,
            text: text.to_owned(),
            message: e.unwrap_stderr(),
        })
}

// ---- Operation parsers ----

/// Collect all remaining arguments as a `Vec<String>`.
fn rest_args(name: &'static str) -> impl Parser<Vec<String>> {
    any::<String, _, _>(name, Some).anywhere().many()
}

fn create_parser() -> impl Parser<Operation> {
    let name = positional::<String>("NAME");
    let on_kw = literal("on");
    let parent = positional::<String>("PARENT");
    let on = construct!(on_kw, parent).map(|(_, p)| p).optional();
    construct!(Operation::Create { name, on })
        .to_options()
        .command("create")
}

fn commit_parser() -> impl Parser<Operation> {
    let count = positional::<u32>("N").optional();
    let kw_times = literal("times").optional();
    let kw_msg = literal("message");
    let msg_val = positional::<String>("MSG");
    let message = construct!(kw_msg, msg_val).map(|(_, m)| m).optional();
    construct!(count, kw_times, message)
        .map(|(count, _, message)| Operation::Commit {
            count: count.unwrap_or(1),
            message,
        })
        .to_options()
        .command("commit")
}

fn checkout_parser() -> impl Parser<Operation> {
    positional::<String>("BRANCH")
        .map(|branch| Operation::Checkout { branch })
        .to_options()
        .command("checkout")
}

fn sync_parser() -> impl Parser<Operation> {
    pure(Operation::Sync).to_options().command("sync")
}

fn stack_parser() -> impl Parser<Operation> {
    let rebase = long("rebase").switch();
    let push = long("push").switch();
    let repair = long("repair").switch();
    let fixup = long("fixup").argument::<String>("ACTION").optional();
    let format = long("format").argument::<String>("FORMAT").optional();
    construct!(rebase, push, repair, fixup, format)
        .map(|(rebase, push, repair, fixup, format)| Operation::Stack {
            rebase,
            push,
            repair,
            fixup,
            format,
        })
        .to_options()
        .command("stack")
}

fn amend_parser() -> impl Parser<Operation> {
    let message = long("message").argument::<String>("MSG").optional();
    let all = long("all").switch();
    let target = positional::<String>("TARGET").optional();
    construct!(message, all, target)
        .map(|(message, all, target)| Operation::Amend {
            message,
            all,
            target,
        })
        .to_options()
        .command("amend")
}

fn write_file_parser() -> impl Parser<Operation> {
    let path = positional::<String>("PATH");
    let content = positional::<String>("CONTENT");
    construct!(path, content)
        .map(|(path, content)| Operation::WriteFile { path, content })
        .to_options()
        .command("write-file")
}

fn stage_parser() -> impl Parser<Operation> {
    positional::<String>("PATH")
        .map(|path| Operation::Stage { path })
        .to_options()
        .command("stage")
}

fn reword_parser() -> impl Parser<Operation> {
    let message = long("message").argument::<String>("MSG");
    let target = positional::<String>("TARGET").optional();
    construct!(message, target)
        .map(|(message, target)| Operation::Reword { message, target })
        .to_options()
        .command("reword")
}

fn next_parser() -> impl Parser<Operation> {
    let count = positional::<u32>("N").optional();
    let branch = long("branch").switch();
    construct!(count, branch)
        .map(|(count, branch)| Operation::Next {
            count: count.unwrap_or(1),
            branch,
        })
        .to_options()
        .command("next")
}

fn prev_parser() -> impl Parser<Operation> {
    let count = positional::<u32>("N").optional();
    let branch = long("branch").switch();
    construct!(count, branch)
        .map(|(count, branch)| Operation::Prev {
            count: count.unwrap_or(1),
            branch,
        })
        .to_options()
        .command("prev")
}

fn run_parser() -> impl Parser<Operation> {
    rest_args("ARG")
        .map(|args| Operation::Run { args })
        .to_options()
        .command("run")
}

fn protect_parser() -> impl Parser<Operation> {
    positional::<String>("GLOB")
        .map(|glob| Operation::Protect { glob })
        .to_options()
        .command("protect")
}

fn git_parser() -> impl Parser<Operation> {
    rest_args("ARG")
        .map(|args| Operation::Git { args })
        .to_options()
        .command("git")
}

fn delete_branch_parser() -> impl Parser<Operation> {
    positional::<String>("NAME")
        .map(|branch| Operation::DeleteBranch { branch })
        .to_options()
        .command("delete-branch")
}

fn operation_parser() -> OptionParser<Operation> {
    let no_commit = long("no-commit").switch();
    let init = no_commit
        .map(|no_commit| Operation::Init { no_commit })
        .to_options()
        .command("INIT");
    let done = pure(Operation::Done).to_options().command("DONE");
    let create = create_parser();
    let commit = commit_parser();
    let checkout = checkout_parser();
    let sync = sync_parser();
    let stack = stack_parser();
    let amend = amend_parser();
    let reword = reword_parser();
    let next = next_parser();
    let prev = prev_parser();
    let run = run_parser();
    let protect = protect_parser();
    let git = git_parser();
    let delete_branch = delete_branch_parser();
    let write_file = write_file_parser();
    let stage = stage_parser();
    construct!([
        init,
        done,
        create,
        commit,
        checkout,
        sync,
        stack,
        amend,
        reword,
        next,
        prev,
        run,
        protect,
        git,
        delete_branch,
        write_file,
        stage
    ])
    .to_options()
}

// ---- Assertion parsers ----

fn ancestor_assertion() -> impl Parser<Assertion> {
    let ancestor = positional::<String>("ANCESTOR");
    let descendant = positional::<String>("DESCENDANT");
    construct!(Assertion::Ancestor {
        ancestor,
        descendant
    })
    .to_options()
    .command("ancestor")
}

fn on_branch_assertion() -> impl Parser<Assertion> {
    let kw = literal("branch");
    let name = positional::<String>("NAME");
    construct!(kw, name)
        .map(|(_, name)| Assertion::On(name))
        .to_options()
        .command("on")
}

fn exists_assertion() -> impl Parser<Assertion> {
    positional::<String>("NAME")
        .map(Assertion::Exists)
        .to_options()
        .command("exists")
}

fn not_exists_assertion() -> impl Parser<Assertion> {
    positional::<String>("NAME")
        .map(Assertion::NotExists)
        .to_options()
        .command("not-exists")
}

fn output_contains_assertion() -> impl Parser<Assertion> {
    let kw = literal("contains");
    let needle = positional::<String>("TEXT");
    construct!(kw, needle)
        .map(|(_, needle)| Assertion::OutputContains(needle))
        .to_options()
        .command("output")
}

fn stderr_contains_assertion() -> impl Parser<Assertion> {
    let kw = literal("contains");
    let needle = positional::<String>("TEXT");
    construct!(kw, needle)
        .map(|(_, needle)| Assertion::StderrContains(needle))
        .to_options()
        .command("stderr")
}

fn fails_assertion() -> impl Parser<Assertion> {
    any::<String, _, _>("ARG", Some)
        .anywhere()
        .some("fails assertion requires args")
        .map(Assertion::Fails)
        .to_options()
        .command("fails")
}

fn commit_message_assertion() -> impl Parser<Assertion> {
    positional::<String>("TEXT")
        .map(Assertion::CommitMessage)
        .to_options()
        .command("commit-message")
}

fn branch_count_assertion() -> impl Parser<Assertion> {
    positional::<u32>("N")
        .map(Assertion::BranchCount)
        .to_options()
        .command("branch-count")
}

fn exit_code_assertion() -> impl Parser<Assertion> {
    positional::<i32>("N")
        .map(Assertion::ExitCode)
        .to_options()
        .command("exit-code")
}

fn assertion_parser() -> OptionParser<Assertion> {
    let ancestor = ancestor_assertion();
    let on = on_branch_assertion();
    let clean = pure(Assertion::Clean).to_options().command("clean");
    let exists = exists_assertion();
    let not_exists = not_exists_assertion();
    let output = output_contains_assertion();
    let stderr = stderr_contains_assertion();
    let fails = fails_assertion();
    let commit_message = commit_message_assertion();
    let branch_count = branch_count_assertion();
    let exit_code = exit_code_assertion();
    construct!([
        ancestor,
        on,
        clean,
        exists,
        not_exists,
        output,
        stderr,
        fails,
        commit_message,
        branch_count,
        exit_code
    ])
    .to_options()
}

// ---- Integration ----

fn parse_operation(line: &str, line_num: usize, text: &str) -> Result<Operation, ScenarioError> {
    run_bpaf(operation_parser(), line, line_num, text)
}

fn parse_assertion(rest: &str, line_num: usize, text: &str) -> Result<Assertion, ScenarioError> {
    run_bpaf(assertion_parser(), rest, line_num, text)
}

/// Extract the contents of all ` ```scenario ` fenced code blocks from markdown.
///
/// Returns a `Vec` of `(block_content, line_offset)` where `line_offset` is the
/// 1-based line number of the first line inside each block (so error messages
/// report correct file positions). Returns an empty vec if no blocks are found.
///
/// Errors if a ` ```scenario ` block is opened but never closed.
fn extract_scenario_blocks(content: &str) -> Result<Vec<(String, usize)>, ScenarioError> {
    let mut blocks = Vec::new();
    let mut inside = false;
    let mut block_lines = Vec::new();
    let mut fence_line = 0;
    let mut content_start = 0;

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !inside {
            if trimmed == "```scenario" {
                inside = true;
                fence_line = idx + 1; // 1-based line of the opening fence
                content_start = idx + 2; // 1-based line of first content line
                block_lines.clear();
            }
        } else if trimmed == "```" {
            let block = block_lines.join("\n");
            blocks.push((block, content_start));
            inside = false;
        } else {
            block_lines.push(line);
        }
    }

    if inside {
        return Err(ScenarioError::Parse {
            line: fence_line,
            text: String::new(),
            message: "unterminated ```scenario block (missing closing ```)".to_owned(),
        });
    }

    Ok(blocks)
}

/// Parse a `.scenario.md` (or raw `.scenario`) file into a `Scenario` AST.
///
/// If the content contains one or more ` ```scenario ` fenced code blocks, the
/// DSL is extracted from those blocks (with line numbers offset to match the
/// original file). Multiple blocks are concatenated in order, so SETUP can live
/// in one block and IT blocks in separate blocks. If no fenced blocks are found,
/// the entire content is parsed as raw DSL for backwards compatibility with
/// inline test strings.
pub fn parse(content: &str, name: &str) -> Result<Scenario, ScenarioError> {
    let blocks = extract_scenario_blocks(content)?;
    let lines: Vec<(usize, &str)> = if blocks.is_empty() {
        content
            .lines()
            .enumerate()
            .map(|(idx, line)| (idx + 1, line))
            .collect()
    } else {
        // Collect lines from each block with their per-block offsets, storing the
        // owned DSL content alongside so borrows remain valid.
        let mut all_lines = Vec::new();
        for (block, offset) in &blocks {
            for (idx, line) in block.lines().enumerate() {
                all_lines.push((idx + offset, line));
            }
        }
        all_lines
    };

    let pos = skip_to_setup(&lines)?;
    let (setup, next_pos) = collect_setup(&lines, pos)?;
    let tests = collect_test_blocks(&lines, next_pos)?;

    if tests.is_empty() {
        let err_line = lines.last().map(|(n, _)| *n).unwrap_or(1);
        return Err(ScenarioError::Parse {
            line: err_line,
            text: String::new(),
            message: "no IT blocks found; every scenario needs at least one IT \"...\" block"
                .to_owned(),
        });
    }

    Ok(Scenario {
        name: name.to_owned(),
        setup,
        tests,
    })
}

/// Phase 1: skip leading comments/blanks, find SETUP. Returns position after SETUP.
fn skip_to_setup(lines: &[(usize, &str)]) -> Result<usize, ScenarioError> {
    let mut pos = 0;
    while pos < lines.len() {
        let trimmed = lines[pos].1.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            pos += 1;
        } else if trimmed == "SETUP" {
            return Ok(pos + 1);
        } else {
            return Err(ScenarioError::Parse {
                line: lines[pos].0,
                text: lines[pos].1.to_owned(),
                message: "expected SETUP as first non-comment line".to_owned(),
            });
        }
    }
    Ok(pos)
}

/// Phase 2: collect setup statements until first IT line.
/// Returns (`setup_stmts`, `next_position`).
fn collect_setup(
    lines: &[(usize, &str)],
    mut pos: usize,
) -> Result<(Vec<Located<Statement>>, usize), ScenarioError> {
    let mut setup = Vec::new();
    while pos < lines.len() {
        let (line_num, raw_line) = lines[pos];
        let trimmed = raw_line.trim();
        if is_it_header(trimmed) {
            break;
        }
        setup.push(parse_line(trimmed, raw_line, line_num)?);
        pos += 1;
    }
    Ok((setup, pos))
}

/// Check whether a trimmed line starts an IT block.
///
/// Requires whitespace after `IT`: `IT "name"` matches, `ITfoo` does not.
fn is_it_header(trimmed: &str) -> bool {
    trimmed == "IT" || trimmed.starts_with("IT ")
}

/// Phase 3: parse IT blocks from current position to end.
fn collect_test_blocks(
    lines: &[(usize, &str)],
    mut pos: usize,
) -> Result<Vec<TestBlock>, ScenarioError> {
    let mut tests = Vec::new();
    while pos < lines.len() {
        let (line_num, raw_line) = lines[pos];
        let trimmed = raw_line.trim();

        if !is_it_header(trimmed) {
            return Err(ScenarioError::Parse {
                line: line_num,
                text: raw_line.to_owned(),
                message: "expected IT \"...\" block".to_owned(),
            });
        }

        let block_name = parse_it_header(trimmed, line_num, raw_line)?;
        let block_line = line_num;
        pos += 1;

        let mut statements = Vec::new();
        while pos < lines.len() {
            let (ln, rl) = lines[pos];
            let t = rl.trim();
            if is_it_header(t) {
                break;
            }
            statements.push(parse_line(t, rl, ln)?);
            pos += 1;
        }

        tests.push(TestBlock {
            name: block_name,
            line: block_line,
            statements,
        });
    }
    Ok(tests)
}

/// Parse an `IT "name"` header line using the shared tokenizer.
///
/// Accepts both `IT smoke` (single unquoted token) and `IT "does something"`
/// (quoted multi-word name). Rejects bare `IT` (no name), unterminated quotes,
/// and trailing tokens after the name.
fn parse_it_header(
    trimmed: &str,
    line_num: usize,
    raw_line: &str,
) -> Result<String, ScenarioError> {
    let tokens = tokenize(trimmed).map_err(|e| ScenarioError::Parse {
        line: line_num,
        text: raw_line.to_owned(),
        message: e.to_string(),
    })?;

    let mut it = tokens.into_iter();
    let first = it.next().unwrap_or_default();
    if first != "IT" {
        return Err(ScenarioError::Parse {
            line: line_num,
            text: raw_line.to_owned(),
            message: "expected IT \"...\" block".to_owned(),
        });
    }

    let name = it.next().ok_or_else(|| ScenarioError::Parse {
        line: line_num,
        text: raw_line.to_owned(),
        message: "IT requires a name, e.g. IT smoke or IT \"does something\"".to_owned(),
    })?;

    let extras: Vec<String> = it.collect();
    if !extras.is_empty() {
        let mut combined = name;
        for x in &extras {
            combined.push(' ');
            combined.push_str(x);
        }
        return Err(ScenarioError::Parse {
            line: line_num,
            text: raw_line.to_owned(),
            message: format!(
                "unexpected extra tokens after IT name; wrap in quotes: IT \"{combined}\""
            ),
        });
    }

    Ok(name)
}

/// Parse a single line into a Located<Statement>.
fn parse_line(
    trimmed: &str,
    raw_line: &str,
    line_num: usize,
) -> Result<Located<Statement>, ScenarioError> {
    let text = raw_line.to_owned();
    let stmt = if trimmed.is_empty() {
        Statement::Blank
    } else if let Some(comment) = trimmed.strip_prefix('#') {
        Statement::Comment(comment.trim().to_owned())
    } else if let Some(rest) = trimmed.strip_prefix("EXPECT ") {
        Statement::Assert(parse_assertion(rest, line_num, &text)?)
    } else if let Some(rest) = trimmed.strip_prefix("try ") {
        Statement::TryOp(parse_operation(rest, line_num, &text)?)
    } else {
        Statement::Op(parse_operation(trimmed, line_num, &text)?)
    };

    Ok(Located {
        line: line_num,
        text,
        inner: stmt,
    })
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
