# reword/amend: protected branch rejection

Verify that both `reword` and `amend --message` refuse to operate
on protected commits (the protected branch is main).

```scenario
SETUP
INIT
DONE
```

<details>
<summary>After SETUP</summary>

```mermaid
gitGraph
  commit id: "initial" tag: "HEAD"
```

</details>

```scenario
IT "reword refuses protected commits"
try reword --message "hahahaha"
EXPECT stderr contains "cannot reword protected commits"
```

<details>
<summary>After "reword refuses protected commits" (FAILED)</summary>

```mermaid
gitGraph
  commit id: "initial" tag: "HEAD"
```

```console
$ exit code: 1
cannot reword protected commits
```

</details>

```scenario
IT "amend --message refuses protected commits"
try amend --message "hahahaha"
EXPECT stderr contains "cannot amend protected commits"
```

<details>
<summary>After "amend --message refuses protected commits" (FAILED)</summary>

```mermaid
gitGraph
  commit id: "initial" tag: "HEAD"
```

```console
$ exit code: 1
cannot amend protected commits
```

</details>
