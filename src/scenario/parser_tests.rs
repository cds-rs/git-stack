use super::*;

#[test]
fn test_parse_setup_it_basic() {
    let input = "\
# test scenario

SETUP
INIT
create feature-A
commit 2 times
DONE

IT \"checks branch\"
EXPECT on branch feature-A

IT \"verifies ancestor\"
EXPECT ancestor main feature-A
";
    let scenario = parse(input, "test").unwrap();
    assert_eq!(scenario.name, "test");

    // Setup has: INIT, create, commit, DONE (+ comment, blank lines)
    let setup_ops: Vec<_> = scenario
        .setup
        .iter()
        .filter(|s| matches!(s.inner, Statement::Op(_) | Statement::Assert(_)))
        .collect();
    assert_eq!(setup_ops.len(), 4); // INIT, create, commit 2 times, DONE

    assert_eq!(scenario.tests.len(), 2);
    assert_eq!(scenario.tests[0].name, "checks branch");
    assert_eq!(scenario.tests[1].name, "verifies ancestor");
}

#[test]
fn test_parse_single_it() {
    let input = "\
SETUP
INIT

IT \"creates a repo\"
EXPECT on branch main
EXPECT clean
";
    let scenario = parse(input, "test").unwrap();
    assert_eq!(scenario.tests.len(), 1);
    assert_eq!(scenario.tests[0].name, "creates a repo");

    let asserts: Vec<_> = scenario.tests[0]
        .statements
        .iter()
        .filter(|s| matches!(s.inner, Statement::Assert(_)))
        .collect();
    assert_eq!(asserts.len(), 2);
}

#[test]
fn test_parse_no_setup_errors() {
    let input = "INIT\ncreate feature-A\n";
    let result = parse(input, "test");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ScenarioError::Parse { .. }));
}

#[test]
fn test_parse_setup_no_it_errors() {
    let input = "\
SETUP
INIT
create feature-A
";
    let result = parse(input, "test");
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        ScenarioError::Parse { message, .. } => {
            assert!(message.contains("no IT blocks"));
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn test_parse_comments_and_blanks() {
    let input = "\
# header comment

SETUP
# setup comment
INIT

IT \"test one\"
# test comment

EXPECT clean
";
    let scenario = parse(input, "test").unwrap();

    let setup_comments: Vec<_> = scenario
        .setup
        .iter()
        .filter(|s| matches!(s.inner, Statement::Comment(_)))
        .collect();
    assert_eq!(setup_comments.len(), 1);

    let test_comments: Vec<_> = scenario.tests[0]
        .statements
        .iter()
        .filter(|s| matches!(s.inner, Statement::Comment(_)))
        .collect();
    assert_eq!(test_comments.len(), 1);
}

#[test]
fn test_parse_create() {
    let input = "\
SETUP
INIT

IT \"test\"
create feature-A
create feature-B on feature-A
";
    let scenario = parse(input, "test").unwrap();
    let stmts = &scenario.tests[0].statements;
    let ops: Vec<_> = stmts
        .iter()
        .filter_map(|s| match &s.inner {
            Statement::Op(op) => Some(op),
            _ => None,
        })
        .collect();
    match &ops[0] {
        Operation::Create { name, on } => {
            assert_eq!(name, "feature-A");
            assert!(on.is_none());
        }
        other => panic!("expected Create, got {other:?}"),
    }
    match &ops[1] {
        Operation::Create { name, on } => {
            assert_eq!(name, "feature-B");
            assert_eq!(on.as_deref(), Some("feature-A"));
        }
        other => panic!("expected Create, got {other:?}"),
    }
}

#[test]
fn test_parse_commit() {
    let input = "\
SETUP
INIT

IT \"test\"
commit
commit 5 times
commit 3 times message \"fix stuff\"
";
    let scenario = parse(input, "test").unwrap();
    let ops: Vec<_> = scenario.tests[0]
        .statements
        .iter()
        .filter_map(|s| match &s.inner {
            Statement::Op(op) => Some(op),
            _ => None,
        })
        .collect();

    match &ops[0] {
        Operation::Commit { count, message } => {
            assert_eq!(*count, 1);
            assert!(message.is_none());
        }
        other => panic!("expected Commit, got {other:?}"),
    }
    match &ops[1] {
        Operation::Commit { count, message } => {
            assert_eq!(*count, 5);
            assert!(message.is_none());
        }
        other => panic!("expected Commit, got {other:?}"),
    }
    match &ops[2] {
        Operation::Commit { count, message } => {
            assert_eq!(*count, 3);
            assert_eq!(message.as_deref(), Some("fix stuff"));
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}

#[test]
fn test_parse_assertions() {
    let input = "\
SETUP
INIT

IT \"test\"
EXPECT ancestor main feature-A
EXPECT on branch feature-A
EXPECT clean
EXPECT exists feature-A
EXPECT output contains \"hello\"
EXPECT branch-count 3
";
    let scenario = parse(input, "test").unwrap();
    let stmts = &scenario.tests[0].statements;

    match &stmts[0].inner {
        Statement::Assert(Assertion::Ancestor {
            ancestor,
            descendant,
        }) => {
            assert_eq!(ancestor, "main");
            assert_eq!(descendant, "feature-A");
        }
        other => panic!("expected Ancestor, got {other:?}"),
    }

    assert!(matches!(
        &stmts[1].inner,
        Statement::Assert(Assertion::On(b)) if b == "feature-A"
    ));
    assert!(matches!(
        &stmts[2].inner,
        Statement::Assert(Assertion::Clean)
    ));
}

#[test]
fn test_parse_git_passthrough() {
    let input = "\
SETUP
INIT

IT \"test\"
git branch -D feature-A
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Git { args }) => {
            assert_eq!(args, &["branch", "-D", "feature-A"]);
        }
        other => panic!("expected Git, got {other:?}"),
    }
}

#[test]
fn test_parse_fails_assertion() {
    let input = "\
SETUP
INIT

IT \"test\"
EXPECT fails sync
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Assert(Assertion::Fails(args)) => {
            assert_eq!(args, &["sync"]);
        }
        other => panic!("expected Fails, got {other:?}"),
    }
}

#[test]
fn test_parse_error_unknown_op() {
    let input = "\
SETUP

IT \"test\"
foobar
";
    let result = parse(input, "t");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ScenarioError::Parse { line: 4, .. }));
}

#[test]
fn test_extract_scenario_blocks_single() {
    let input = "\
# Title

Some description.

```scenario
SETUP
INIT
DONE

IT \"test\"
EXPECT clean
```
";
    let blocks = extract_scenario_blocks(input).expect("single block should parse");
    assert_eq!(blocks.len(), 1);
    let (block, offset) = &blocks[0];
    assert_eq!(*offset, 6); // line 6 in the file is "SETUP"
    assert!(block.starts_with("SETUP"));
    assert!(block.contains("IT \"test\""));
}

#[test]
fn test_extract_scenario_blocks_none() {
    let input = "SETUP\nINIT\nDONE\n\nIT \"test\"\nEXPECT clean\n";
    assert!(
        extract_scenario_blocks(input)
            .expect("no fenced blocks should parse")
            .is_empty()
    );
}

#[test]
fn test_extract_scenario_blocks_multiple() {
    let input = "\
# Title

```scenario
SETUP
INIT
DONE
```

Some prose.

```scenario
IT \"first test\"
EXPECT clean
```

More prose.

```scenario
IT \"second test\"
EXPECT on branch main
```
";
    let blocks = extract_scenario_blocks(input).expect("multiple blocks should parse");
    assert_eq!(blocks.len(), 3);
    assert!(blocks[0].0.starts_with("SETUP"));
    assert!(blocks[1].0.starts_with("IT \"first test\""));
    assert!(blocks[2].0.starts_with("IT \"second test\""));
}

#[test]
fn test_parse_multi_block_markdown() {
    let input = "\
# Title

```scenario
SETUP
INIT
create feature-A
commit 2 times
DONE
```

After setup graph.

```scenario
IT \"checks branch\"
EXPECT on branch feature-A
```

```scenario
IT \"verifies ancestor\"
EXPECT ancestor main feature-A
```
";
    let scenario = parse(input, "test").unwrap();
    assert_eq!(scenario.name, "test");
    assert_eq!(scenario.tests.len(), 2);
    assert_eq!(scenario.tests[0].name, "checks branch");
    assert_eq!(scenario.tests[1].name, "verifies ancestor");
}

#[test]
fn test_parse_multi_block_line_numbers() {
    let input = "\
# Title

```scenario
SETUP
```

```scenario
IT \"test\"
foobar
```
";
    let result = parse(input, "t");
    assert!(result.is_err());
    let err = result.unwrap_err();
    // ```scenario on line 7, IT on line 8, foobar on line 9
    assert!(matches!(err, ScenarioError::Parse { line: 9, .. }));
}

#[test]
fn test_parse_markdown_wrapped() {
    let input = "\
# My scenario

Description text here.

```mermaid
gitGraph
  commit id: \"initial\"
```

```scenario
SETUP
INIT
DONE

IT \"works inside markdown\"
EXPECT clean
```
";
    let scenario = parse(input, "test").unwrap();
    assert_eq!(scenario.tests.len(), 1);
    assert_eq!(scenario.tests[0].name, "works inside markdown");
}

#[test]
fn test_parse_markdown_line_numbers() {
    let input = "\
# Title
description
more text

```scenario
SETUP

IT \"test\"
foobar
```
";
    let result = parse(input, "t");
    assert!(result.is_err());
    let err = result.unwrap_err();
    // ```scenario is line 5, content starts line 6: SETUP, line 7: blank,
    // line 8: IT "test", line 9: foobar
    assert!(matches!(err, ScenarioError::Parse { line: 9, .. }));
}

#[test]
fn test_parse_stack_op() {
    let input = "\
SETUP
INIT

IT \"test\"
stack --rebase --fixup squash
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Stack {
            rebase,
            push,
            repair,
            fixup,
            format,
        }) => {
            assert!(*rebase);
            assert!(!*push);
            assert!(!*repair);
            assert_eq!(fixup.as_deref(), Some("squash"));
            assert!(format.is_none());
        }
        other => panic!("expected Stack, got {other:?}"),
    }
}

#[test]
fn test_parse_next_prev() {
    let input = "\
SETUP
INIT

IT \"test\"
next 3 --branch
prev
";
    let scenario = parse(input, "test").unwrap();
    let ops: Vec<_> = scenario.tests[0]
        .statements
        .iter()
        .filter_map(|s| match &s.inner {
            Statement::Op(op) => Some(op),
            _ => None,
        })
        .collect();
    match &ops[0] {
        Operation::Next { count, branch } => {
            assert_eq!(*count, 3);
            assert!(*branch);
        }
        other => panic!("expected Next, got {other:?}"),
    }
    match &ops[1] {
        Operation::Prev { count, branch } => {
            assert_eq!(*count, 1);
            assert!(!*branch);
        }
        other => panic!("expected Prev, got {other:?}"),
    }
}

#[test]
fn test_parse_delete_branch() {
    let input = "\
SETUP
INIT

IT \"test\"
delete-branch feature-A
";
    let scenario = parse(input, "test").unwrap();
    assert!(matches!(
        &scenario.tests[0].statements[0].inner,
        Statement::Op(Operation::DeleteBranch { branch }) if branch == "feature-A"
    ));
}

#[test]
fn test_parse_stderr_assertion() {
    let input = "\
SETUP
INIT

IT \"test\"
EXPECT stderr contains \"error\"
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Assert(Assertion::StderrContains(needle)) => {
            assert_eq!(needle, "error");
        }
        other => panic!("expected StderrContains, got {other:?}"),
    }
}

#[test]
fn test_parse_commit_message_assertion() {
    let input = "\
SETUP
INIT

IT \"test\"
EXPECT commit-message \"fix stuff\"
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Assert(Assertion::CommitMessage(msg)) => {
            assert_eq!(msg, "fix stuff");
        }
        other => panic!("expected CommitMessage, got {other:?}"),
    }
}

#[test]
fn test_parse_init_no_commit() {
    let input = "\
SETUP
INIT --no-commit

IT \"test\"
EXPECT clean
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.setup[0].inner {
        Statement::Op(Operation::Init { no_commit }) => {
            assert!(*no_commit);
        }
        other => panic!("expected Init, got {other:?}"),
    }
}

#[test]
fn test_parse_init_default() {
    let input = "\
SETUP
INIT

IT \"test\"
EXPECT clean
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.setup[0].inner {
        Statement::Op(Operation::Init { no_commit }) => {
            assert!(!*no_commit);
        }
        other => panic!("expected Init, got {other:?}"),
    }
}

#[test]
fn test_parse_amend_with_message() {
    let input = "\
SETUP
INIT

IT \"test\"
amend --message \"updated commit\"
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Amend { message, all, target }) => {
            assert_eq!(message.as_deref(), Some("updated commit"));
            assert!(!*all);
            assert!(target.is_none());
        }
        other => panic!("expected Amend, got {other:?}"),
    }
}

#[test]
fn test_parse_amend_all() {
    let input = "\
SETUP
INIT

IT \"test\"
amend --all
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Amend { message, all, target }) => {
            assert!(message.is_none());
            assert!(*all);
            assert!(target.is_none());
        }
        other => panic!("expected Amend, got {other:?}"),
    }
}

#[test]
fn test_parse_amend_bare() {
    let input = "\
SETUP
INIT

IT \"test\"
amend
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Amend { message, all, target }) => {
            assert!(message.is_none());
            assert!(!*all);
            assert!(target.is_none());
        }
        other => panic!("expected Amend, got {other:?}"),
    }
}

#[test]
fn test_parse_amend_with_target() {
    let input = "\
SETUP
INIT

IT \"test\"
amend --message \"updated\" target-branch
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Amend {
            message,
            all,
            target,
        }) => {
            assert_eq!(message.as_deref(), Some("updated"));
            assert!(!*all);
            assert_eq!(target.as_deref(), Some("target-branch"));
        }
        other => panic!("expected Amend, got {other:?}"),
    }
}

#[test]
fn test_parse_reword_with_target() {
    let input = "\
SETUP
INIT

IT \"test\"
reword --message \"new msg\" target-branch
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Reword { message, target }) => {
            assert_eq!(message, "new msg");
            assert_eq!(target.as_deref(), Some("target-branch"));
        }
        other => panic!("expected Reword, got {other:?}"),
    }
}

#[test]
fn test_parse_write_file() {
    let input = "\
SETUP
INIT

IT \"test\"
write-file change.txt \"new content\"
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::WriteFile { path, content }) => {
            assert_eq!(path, "change.txt");
            assert_eq!(content, "new content");
        }
        other => panic!("expected WriteFile, got {other:?}"),
    }
}

#[test]
fn test_parse_stage() {
    let input = "\
SETUP
INIT

IT \"test\"
stage change.txt
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Stage { path }) => {
            assert_eq!(path, "change.txt");
        }
        other => panic!("expected Stage, got {other:?}"),
    }
}

#[test]
fn test_parse_stack_format() {
    let input = "\
SETUP
INIT

IT \"test\"
stack --format list
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Stack { format, .. }) => {
            assert_eq!(format.as_deref(), Some("list"));
        }
        other => panic!("expected Stack, got {other:?}"),
    }
}

#[test]
fn test_parse_exit_code_assertion() {
    let input = "\
SETUP
INIT

IT \"test\"
EXPECT exit-code 1
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Assert(Assertion::ExitCode(code)) => {
            assert_eq!(*code, 1);
        }
        other => panic!("expected ExitCode, got {other:?}"),
    }
}

#[test]
fn test_parse_try_prefix() {
    let input = "\
SETUP
INIT

IT \"test\"
try stack
EXPECT exit-code 0
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::TryOp(Operation::Stack { .. }) => {}
        other => panic!("expected TryOp(Stack), got {other:?}"),
    }
}

// ---- Tokenizer error tests ----

#[test]
fn test_unterminated_quote_in_op() {
    let input = "\
SETUP
INIT

IT \"test\"
commit 3 times message \"fix stuff
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { line: 5, message, .. } => {
            assert!(message.contains("unterminated"), "got: {message}");
        }
        other => panic!("expected Parse with unterminated, got {other:?}"),
    }
}

#[test]
fn test_unterminated_quote_in_it_header() {
    let input = "\
SETUP
INIT

IT \"oops
EXPECT clean
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { message, .. } => {
            assert!(message.contains("unterminated"), "got: {message}");
        }
        other => panic!("expected Parse with unterminated, got {other:?}"),
    }
}

#[test]
fn test_escaped_quote_in_message() {
    let input = r#"
SETUP
INIT

IT "test"
commit 1 times message "say \"hello\""
"#;
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Commit { message, .. }) => {
            assert_eq!(message.as_deref(), Some("say \"hello\""));
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}

#[test]
fn test_it_bare_errors() {
    let input = "\
SETUP
INIT

IT
EXPECT clean
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { message, .. } => {
            assert!(
                message.contains("IT requires a name"),
                "got: {message}"
            );
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn test_it_extra_tokens_errors() {
    let input = "\
SETUP
INIT

IT \"name\" extra
EXPECT clean
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { message, .. } => {
            assert!(
                message.contains("unexpected extra token"),
                "got: {message}"
            );
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn test_try_requires_space() {
    // "tryfoo" should not be parsed as "try foo"
    let input = "\
SETUP
INIT

IT \"test\"
tryfoo
";
    let result = parse(input, "test");
    assert!(result.is_err());
}

#[test]
fn test_git_args_preserved_as_vec() {
    let input = "\
SETUP
INIT

IT \"test\"
git commit --allow-empty -m \"hello world\"
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Git { args }) => {
            assert_eq!(
                args,
                &["commit", "--allow-empty", "-m", "hello world"]
            );
        }
        other => panic!("expected Git, got {other:?}"),
    }
}

#[test]
fn test_run_args_preserved_as_vec() {
    let input = "\
SETUP
INIT

IT \"test\"
run echo \"hello world\"
";
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Run { args }) => {
            assert_eq!(args, &["echo", "hello world"]);
        }
        other => panic!("expected Run, got {other:?}"),
    }
}

#[test]
fn test_unterminated_scenario_block() {
    let input = "\
# Title

```scenario
SETUP
INIT

IT \"test\"
EXPECT clean
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { line, message, .. } => {
            assert_eq!(line, 3, "error should point at the opening fence");
            assert!(message.contains("unterminated"), "got: {message}");
            assert!(message.contains("```scenario"), "got: {message}");
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn test_escape_only_quote_and_backslash() {
    // \t inside quotes should be kept as literal \t, not just t
    let input = r#"
SETUP
INIT

IT "test"
commit 1 times message "hello\tworld"
"#;
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Commit { message, .. }) => {
            assert_eq!(message.as_deref(), Some("hello\\tworld"));
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}

#[test]
fn test_escaped_backslash_in_quotes() {
    let input = r#"
SETUP
INIT

IT "test"
commit 1 times message "path\\to\\file"
"#;
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Commit { message, .. }) => {
            assert_eq!(message.as_deref(), Some("path\\to\\file"));
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}

#[test]
fn test_it_unquoted_single_word() {
    // Single-word unquoted IT names are accepted
    let input = "\
SETUP
INIT

IT smoke
EXPECT clean
";
    let scenario = parse(input, "test").unwrap();
    assert_eq!(scenario.tests[0].name, "smoke");
}

#[test]
fn test_it_unquoted_multi_word_errors() {
    // Multi-word unquoted IT names produce a helpful error
    let input = "\
SETUP
INIT

IT hello world
EXPECT clean
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { message, .. } => {
            assert!(message.contains("wrap in quotes"), "got: {message}");
            // Suggestion should combine all tokens
            assert!(message.contains("IT \"hello world\""), "got: {message}");
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn test_it_many_extra_tokens_combined() {
    let input = "\
SETUP
INIT

IT a b c
EXPECT clean
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { message, .. } => {
            // All tokens should appear in the suggestion
            assert!(message.contains("IT \"a b c\""), "got: {message}");
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn test_unterminated_quote_in_assertion() {
    let input = "\
SETUP
INIT

IT \"test\"
EXPECT output contains \"oops
";
    let result = parse(input, "test");
    assert!(result.is_err());
    match result.unwrap_err() {
        ScenarioError::Parse { message, .. } => {
            assert!(message.contains("unterminated"), "got: {message}");
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn test_escape_unknown_sequence_preserved() {
    // \q should be kept as literal \q, not just q
    let input = r#"
SETUP
INIT

IT "test"
commit 1 times message "a\qb"
"#;
    let scenario = parse(input, "test").unwrap();
    match &scenario.tests[0].statements[0].inner {
        Statement::Op(Operation::Commit { message, .. }) => {
            assert_eq!(message.as_deref(), Some("a\\qb"));
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}
