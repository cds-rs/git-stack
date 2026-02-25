# bug #367: crash on repo with no commits

Reproduces https://github.com/gitext-rs/git-stack/issues/367.
git-stack panics when run in a repo that has no commits yet.
The process is killed by a signal (panic), producing exit code -1
instead of a clean non-zero exit with a friendly error message.

```scenario
SETUP
INIT --no-commit
DONE
```

<details>
<summary>After SETUP</summary>

```mermaid
%% error building graph: failed to list branches
```

</details>

```scenario
IT "does not crash on empty repo"
try stack
# Panics give exit code -1 (signal); a fixed version should give 1
EXPECT exit-code 1
```

<details>
<summary>After "does not crash on empty repo" (FAILED)</summary>

```mermaid
%% error building graph: failed to list branches
```

```console
$ exit code: -1
thread 'main' (2004310) panicked at src/legacy/git/repo.rs:250:33:
Unexpected git2 error: reference 'refs/heads/main' not found; class=Reference (4); code=UnbornBranch (-9)
stack backtrace:
   0: __rustc::rust_begin_unwind
             at /rustc/842bd5be253e17831e318fdbd9d01d716557cc75/library/std/src/panicking.rs:689:5
   1: core::panicking::panic_fmt
             at /rustc/842bd5be253e17831e318fdbd9d01d716557cc75/library/core/src/panicking.rs:80:14
   2: <git_stack::legacy::git::repo::GitRepo>::head_commit::{closure#0}
             at ~/oss/git-stack/src/legacy/git/repo.rs:250:33
   3: <core::result::Result<git2::reference::Reference, git2::error::Error>>::unwrap_or_else::<<git_stack::legacy::git::repo::GitRepo>::head_commit::{closure#0}>
             at ~/.rustup/toolchains/nightly-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/result.rs:1622:23
   4: <git_stack::legacy::git::repo::GitRepo>::head_commit
             at ~/oss/git-stack/src/legacy/git/repo.rs:250:14
   5: <git_stack::stack::State>::new
             at ~/oss/git-stack/src/bin/git-stack/stack.rs:122:32
   6: git_stack::stack::stack
             at ~/oss/git-stack/src/bin/git-stack/stack.rs:316:21
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
```

</details>
