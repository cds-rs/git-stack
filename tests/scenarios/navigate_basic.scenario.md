# navigate: next and prev across a stack

Verify that `git next` and `git prev` navigate through branches in a stack.

```scenario
SETUP
INIT
create feature-A
commit 2 times
create feature-B
commit 2 times
checkout main
DONE
```

<details>
<summary>After SETUP</summary>

```mermaid
gitGraph
  commit id: "initial" tag: "HEAD"
  branch feature-A
  commit id: "commit 1 on line 9"
  commit id: "commit 2 on line 9"
  branch feature-B
  commit id: "commit 1 on line 11"
  commit id: "commit 2 on line 11"
```

</details>

```scenario
IT "next --branch moves to the first child branch"
next --branch
EXPECT on branch feature-A
```

<details>
<summary>After "next --branch moves to the first child branch"</summary>

```mermaid
gitGraph
  commit id: "initial"
  branch feature-A
  commit id: "commit 1 on line 9"
  commit id: "commit 2 on line 9" tag: "HEAD"
  branch feature-B
  commit id: "commit 1 on line 11"
  commit id: "commit 2 on line 11"
```

</details>

```scenario
IT "prev --branch from feature-B returns to feature-A"
checkout feature-B
prev --branch
EXPECT on branch feature-A
```

<details>
<summary>After "prev --branch from feature-B returns to feature-A"</summary>

```mermaid
gitGraph
  commit id: "initial"
  branch feature-A
  commit id: "commit 1 on line 9"
  commit id: "commit 2 on line 9" tag: "HEAD"
  branch feature-B
  commit id: "commit 1 on line 11"
  commit id: "commit 2 on line 11"
```

</details>

```scenario
IT "next then prev is a round trip"
next --branch
EXPECT on branch feature-A
next --branch
EXPECT on branch feature-B
prev --branch
EXPECT on branch feature-A
prev --branch
EXPECT on branch main
```
