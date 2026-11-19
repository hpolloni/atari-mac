# CLAUDE.md

## How to work on large tasks

- For a large task, favor teammates (subagents) that work in their own git worktrees
  (`isolation: "worktree"`).
- Split the task into independent parts with clear boundaries (for example, one module or
  one chip per teammate), and give each teammate a narrow, explicit scope.
- Do the integration and the final verification yourself, after the teammates finish.
- Each teammate must leave its worktree with all gates passing (`make pre-commit` at minimum).

## The lead role

- The main session is the lead. The lead plans, decides, writes the briefs, reviews the
  results, merges the branches and runs the final checks.
- The lead does not write implementation code or tests. A teammate does that work, also for
  small packages such as interface stubs or test helpers.
- The lead may make small fixes during a merge (for example a conflict or a broken link), and
  edits to the specification, the plan and this file.
