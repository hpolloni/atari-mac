# CLAUDE.md

An Atari 65XE emulator in Rust, exact to the CPU cycle. The core is `crates/atari-core` (no
dependencies), the test harness and the specification gates are `crates/harness`, and the
macOS frontend is the root crate `atari-mac`. `README.md` gives the layout and the commands.

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
  edits to the specification, the plan, `README.md` and this file.

## Decisions

- Hardware questions and scope questions belong to the user. Record each one as an open item
  (`docs/spec/open-items/OI-NNN.md`), show the user the options with the HRM sources, and
  record the user's decision in the item. Do not decide them for the user.
- Integration bugs between two designs are not user questions. The lead fixes them.
- When a requirement is wrong or cannot be met, a teammate reports it and does not weaken the
  test. The lead changes the requirement, or asks the user when the change is a decision.
- The main hardware source is the Altirra Hardware Reference Manual (HRM,
  `reference/altirra-hardware-reference.pdf`, in git). The other references are second
  sources (OI-041), listed in `docs/spec/references/`; `make reference` downloads the pinned
  ones. The HRM is correct when two references do not agree. Check a claim against the HRM
  text, and against the second sources that cover it, before you show it to the user.

## Order of work for each change

1. A requirement (`docs/spec/requirements/REQ-NNN.md`, YAML frontmatter) states the behavior
   with exact acceptance criteria. Follow the rules in `docs/spec/README.md`.
2. A design document (`docs/design/`) states the interface and names the tests in its
   `## Test plan`. `docs/design/system.md` "Timing" is the only definition of the machine
   cycle order.
3. Each test that a test plan names exists in the code. Until its package is `done`, it can
   be an ignored stub with the reason `planned: SLICE-NN` (REQ-027, REQ-049).
4. A package replaces its stubs with real tests and writes the code. Each test has a
   `/// Verifies REQ-NNN` comment.
5. Each requirement that is not withdrawn is in one work package of a slice
   (`docs/plan/SLICE-NN.md`). A package or a slice is `done` only when each `test`
   requirement has a test that runs.

### Small changes

- The full order above is for new features and for hardware behavior.
- A change that stays inside one existing requirement gets one edit to that requirement, at
  most one new test name in its test plan row, and one teammate. It needs no new design
  section, no new slice and no measurement study.
- Ask the user only when the options give different behavior that the user sees. Do not ask
  how to do something that the user already asked for. Pick the obvious way and say so.

## Gates

- `make check` runs every gate (more than 10 minutes). `make pre-commit` is the minimum for
  a teammate. `make spec-check` runs the specification gates only.
- After a change to a requirement, an open item, a slice or `README.md`, run
  `cargo run -p atari-harness --bin spec-index`. It writes the indexes and the status block
  of `README.md`, and the gates fail when they are out of date.
- `docs/spec/gate-baseline.txt` holds the old violations. It may only become smaller. Remove
  a line when the gate reports it as stale. Do not add lines.
- An evidence record (`docs/verification/REQ-NNN.md`) is only for `inspection` and
  `demonstration` requirements, and only for what the user confirmed. A review of golden
  images is recorded in `crates/harness/golden/README.md`.
- A new golden image must show what its test intends. The teammate checks it with pixel
  runs against the expected data, not only by eye. The lead looks at each image enlarged
  before the user reviews it. A description from a teammate is not proof.
- No `unsafe`. To silence a lint, use `#[expect(lint, reason = "...")]`. `#[allow]` is
  rejected.

## Worktrees

- A new worktree can start from an older commit. A teammate first checks
  `git log --oneline -1` and fast-forwards to `main` if necessary.
- ROMs and test data are not in git. A teammate copies `roms/*.rom` from the main checkout
  and makes a symlink to `test-data/`. Neither is committed.
- Before a teammate reports, it stops every background command and wait loop that it
  started. A wait loop must also end when the command that it waits for has ended.
- Merge the branches that other branches depend on first. A gate change that makes the gates
  stricter merges after the work that it checks.

## Git

- The lead commits merges and its own edits on `main` locally. Push only when the user asks,
  and ask again before each push.
- No ROM files, test data or secrets in git.
- No commercial software. A test, a requirement, a document or a commit message does not
  read or name a game, a program or a media image that the user supplies (for example a
  file in `roms/`). The user names such a file only as an example. Tests make their media
  in code.
- In records, name roles (the user, the lead, a teammate), not persons.
