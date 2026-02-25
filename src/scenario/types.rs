//! AST types for the scenario DSL.
//!
//! Every `.scenario.md` file has a SETUP block and one or more IT test blocks.
//! The filename becomes the scenario name.

/// A parsed scenario: a named setup section with one or more test blocks.
#[derive(Debug)]
pub struct Scenario {
    pub name: String,
    pub setup: Vec<Located<Statement>>,
    pub tests: Vec<TestBlock>,
}

/// A single IT block: a named test that runs against a fresh repo with SETUP applied.
#[derive(Debug)]
pub struct TestBlock {
    pub name: String,
    pub line: usize,
    pub statements: Vec<Located<Statement>>,
}

/// Wraps a value with its source location for error reporting.
#[derive(Debug)]
pub struct Located<T> {
    pub line: usize,
    pub text: String,
    pub inner: T,
}

/// A single line in a scenario file.
#[derive(Debug)]
pub enum Statement {
    Comment(String),
    Blank,
    Op(Operation),
    /// Like `Op`, but does not abort on non-zero exit (for testing error paths).
    TryOp(Operation),
    Assert(Assertion),
}

/// Setup and mutation operations.
#[derive(Debug, Clone)]
pub enum Operation {
    Init {
        no_commit: bool,
    },
    Done,
    Create {
        name: String,
        on: Option<String>,
    },
    Commit {
        count: u32,
        message: Option<String>,
    },
    Checkout {
        branch: String,
    },
    Sync,
    Stack {
        rebase: bool,
        push: bool,
        repair: bool,
        fixup: Option<String>,
        format: Option<String>,
    },
    Amend {
        message: Option<String>,
        all: bool,
        target: Option<String>,
    },
    WriteFile {
        path: String,
        content: String,
    },
    Stage {
        path: String,
    },
    Reword {
        message: String,
        target: Option<String>,
    },
    Next {
        count: u32,
        branch: bool,
    },
    Prev {
        count: u32,
        branch: bool,
    },
    Run {
        args: Vec<String>,
    },
    Protect {
        glob: String,
    },
    Git {
        args: Vec<String>,
    },
    DeleteBranch {
        branch: String,
    },
}

/// Assertions that check state after operations.
#[derive(Debug, Clone)]
pub enum Assertion {
    Ancestor {
        ancestor: String,
        descendant: String,
    },
    On(String),
    Clean,
    Exists(String),
    NotExists(String),
    OutputContains(String),
    StderrContains(String),
    Fails(Vec<String>),
    CommitMessage(String),
    BranchCount(u32),
    ExitCode(i32),
}
