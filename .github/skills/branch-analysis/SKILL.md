---
name: branch-analysis
description: "Analyze local git branches to find unmerged work, stale branches, and local-only changes. Use when: auditing branches, finding forgotten work, triaging local branches, checking for unpushed commits, identifying squash-merged branches, spring cleaning git branches."
argument-hint: "Optionally specify a branch name prefix to filter (e.g. 'minestarks/') or 'all' for every branch"
---

# Branch Analysis

Audit all local git branches to find unmerged work, identify branches already squash-merged into main, attempt merges with main, and produce a structured markdown report with recommendations.

## When to Use

- You want to find local branches with changes that never made it to `origin/main`
- You want to identify forgotten work, local-only branches, or unpushed commits
- You want to triage stale branches and decide what to keep, finish, or toss
- You want to check which branches have already been squash-merged into main
- You want a structured summary of all branch status in one place

## Safety Rules

- **NEVER `git push`** — this is a read-and-analyze workflow. The only write operation allowed is `git merge origin/main` into local branches (to test mergeability).
- **NEVER delete branches** — only report recommendations.
- **NEVER force-push or reset** — preserve all local state.
- Before starting, run `git fetch` to ensure remote refs are current (or confirm the user has already done so).

## Philosophy: Preserve Over Toss

- **Default to keeping branches.** A branch with unique code has value even if it's old, experimental, or WIP. The user chose to write that code — assume it's interesting until proven otherwise.
- **"Toss" is reserved for branches with ZERO unique value** — i.e., the exact same work (not just similar work) already landed in main via a squash-merge PR.
- **Exploration branches are valuable.** A branch called "refactoring-adventure" or "sized-array" with WIP commits still contains design ideas, experiments, and approaches worth preserving. Label these as **Keep (exploration)** not "Toss".
- **When in doubt, recommend Keep or Evaluate**, not Toss.
- **Even for landed work**, if the branch has _additional_ commits beyond what was merged, note them specifically — don't blanket-toss the whole branch.

## Procedure

### Phase 1: Identify the User and Branches

1. Get git identity: `git config user.name` and `git config user.email`
2. List all local branches: `git branch --format='%(refname:short)'`
3. If the user specified a prefix filter (e.g. `minestarks/`), filter to those. Otherwise, exclude `main` and analyze all branches.
4. For each branch, count commits ahead of `origin/main`:
   ```
   git rev-list --count origin/main..<branch>
   ```
   Skip branches with 0 commits ahead (already merged or identical to main).

### Phase 2: Classify Each Branch

For each branch with commits ahead of `origin/main`:

1. **Check remote tracking**: Does `origin/<branch>` exist?

   - If no → mark as **LOCAL ONLY** (highest risk — exists nowhere else)
   - If yes → compare local vs remote SHA:
     - Same SHA → **Pushed (up-to-date)**
     - Different → count unpushed commits: `git rev-list --count origin/<branch>..<branch>`

2. **Check if squash-merged**: Use three-dot diff to see if any actual changes remain:

   ```
   git diff origin/main...<branch> --stat | tail -1
   ```

   If empty → branch was squash-merged, mark as **Already in main**.

3. **Search for matching PRs**: Search `origin/main` log for keywords from the branch's commit messages:
   ```
   git log --oneline origin/main --grep="<keyword>"
   ```
   This catches squash-merged PRs where the branch commits differ but the work landed.

### Phase 3: Analyze Each Branch's Changes

For each branch that has a non-empty diff vs main:

1. Get the commit log:
   ```
   git log origin/main..<branch> --oneline --no-merges
   ```
2. Get the diff stat:
   ```
   git diff origin/main...<branch> --stat
   ```
3. Get the last activity date:
   ```
   git log -1 --format='%as' <branch>
   ```
4. **Read the actual diff in detail.** Don't just rely on commit messages or `--stat` — actually read the code:

   ```
   git diff origin/main...<branch>
   ```

   For large diffs, read at least the first 200 lines and sample key files. Look at:

   - New files added (these are often the most interesting part)
   - New test cases (they reveal intent)
   - Structural changes (new modules, new types, new traits)
   - Any `NOTES.md`, `TODO`, or design comments the author left
   - Configuration/manifest changes that reveal scope

5. **Describe each branch substantively.** The description should be 2-4 sentences covering:

   - What technical problem the branch addresses
   - What approach or design it takes
   - What specific new code structures it introduces (types, modules, algorithms)
   - What state the work is in (complete, WIP, exploring)

   Bad: "WIP circuit work. Superseded."
   Good: "Introduces a `rir_to_circuit` module that reconstructs structured control flow from RIR basic blocks using dominator/post-dominator analysis. Adds `ControlFlowReconstructor` type and 968 lines of control flow analysis in `control_flow.rs`. WIP — the nesting logic for RIR→circuit conversion has a known bug (per commit message). Contains unique algorithmic work not present in main."

### Phase 4: Attempt Merges (Try Hard)

For each branch that has unique work (not already in main):

1. Check out the branch: `git checkout <branch>`
2. Attempt merge: `git merge origin/main --no-edit`
3. Record the result:
   - **Clean merge** — no conflicts
   - **Conflicts** — count conflict markers, list conflicted files, and **read the conflicts**
4. **Try to resolve conflicts.** Don't give up at the first sign of conflict:
   - Read each conflict marker to understand what both sides changed
   - Snapshot file conflicts are almost always resolvable by taking one side (usually `origin/main` for files that moved forward)
   - API signature changes where the branch uses an old API that was renamed/refactored in main — take main's API and update the branch's usage
   - Test baseline conflicts where both sides changed expected output — take main's baselines if the branch's feature already landed, or keep the branch's baselines if the feature is unique
   - Only abort if the conflicts require understanding deep semantic intent that you cannot determine from context
5. For **partially resolvable** merges: resolve what you can, note what remains, and commit with a clear message like `"Merge origin/main - resolve N conflicts, M remain"`
6. Always return to `main` at the end: `git checkout main`

**Important:** A branch having merge conflicts does NOT make it less valuable. Many interesting branches diverge precisely because they touch the same code that evolved on main. Note conflicts as a data point, not a reason to toss.

### Phase 5: Generate the Report

Create a markdown file (default: `branch-analysis.md` in repo root) with the following structure:

#### Report Structure

```markdown
# Local Branch Analysis — <date>

## Legend

- Status, Merge result, Recommendation definitions

## Already Landed in Main (safe to toss)

| # | Branch | Commits | Last Date | Merged As | Merge w/ main | Recommendation |

## Branches With Unique Remaining Value

| # | Branch | Commits | Last Date | Local Only? | Merge | Description | Recommendation |

## Branches to Toss (WIP / experimental / superseded)

| # | Branch | Commits | Last Date | Description | Recommendation |

## Priority Actions

1. Branches to push to remote (local-only with significant work)
2. Branches ready for PRs (clean merge, unique value)
3. Branches needing decisions
4. Cleanup candidates
```

#### Recommendation Categories

- **Toss**: Work is **fully and exactly** in main already. The branch has zero unique code remaining. (This is rare — even "merged" branches often have extra commits.)
- **Finish**: Has unique value not in main, in good enough shape to become a PR with some work
- **Keep (exploration)**: Contains experimental/WIP work, design ideas, or algorithmic approaches worth preserving for future reference even if not PR-ready. This includes branches with interesting code that may inform future work.
- **Evaluate**: Needs human decision — the work may overlap partially with main, or its relevance depends on current priorities
- **Push to remote ASAP**: Local-only branch with significant work (backup urgency)

**Anti-pattern: Don't label exploration branches as "Toss".** If a branch has 50+ commits of exploratory work with unique algorithms, data structures, or design approaches, it has value even if the final implementation took a different path. Label it **Keep (exploration)** and describe what's interesting in it.

#### Description Quality

For **every** branch (not just ones with "unique work"), the description should:

- Explain **what** the branch does (not just repeat the branch name)
- Note specific new files, modules, types, or algorithms introduced
- Quote interesting commit messages or code comments that reveal intent
- Mention the size of changes (files changed, insertions)
- For large branches, list the top 3-5 most interesting files added or significantly modified
- Call out relationships between branches (e.g., "evolution of Y", "shares commits with Z")
- Flag identical branches (same HEAD SHA)
- **Never say "superseded" without evidence.** A branch is only superseded if the _same approach_ landed in main. If main took a _different approach_ to solve the same problem, the branch's approach is still interesting and unique.

### Phase 6: Summary

After creating the report, give a brief verbal summary highlighting:

- Total branches analyzed
- How many are already in main vs have unique work
- Any **LOCAL ONLY** branches that need immediate backup
- Top 3 recommended actions
