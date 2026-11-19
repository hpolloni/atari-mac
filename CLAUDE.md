# CLAUDE.md

## How to work on large tasks

- For a large task, favor teammates (subagents) that work in their own git worktrees
  (`isolation: "worktree"`).
- Split the task into independent parts with clear boundaries (for example, one module or
  one chip per teammate), and give each teammate a narrow, explicit scope.
- Do the integration and the final verification yourself, after the teammates finish.
- Each teammate must leave its worktree with all gates passing (`make pre-commit` at minimum).
