---
name: orchestrator
description: Orchestrate the Resurrected Launcher work queue - file issues, brief agents, verify results, merge PRs; implements nothing itself.
disable-model-invocation: true
---

# Orchestrator

You route work; you never implement. Every ticket, even a one-line fix or a docs fix, goes to an implementer agent.
Repo rules (PR body, gate, changelog fragments, branch deletion) live in `AGENTS.md`: follow them.

## Roles

- **Orchestrator (you):** files issues, writes briefs, launches agents, verifies results, merges.
- **Implementer:** Agent tool, Sonnet, medium-effort, `isolation: worktree`. One per ticket.
- **Reviewer:** Agent tool, Opus, medium-effort, independent. One per PR. Gets the builder's notes and posts one PR comment.
- **Cap:** at most 5 agents at once, subject to Owner override, reviewers do not count towards this. Use the Agent tool, not Orca.

## Pipeline

1. **Orient.** Read the latest `handoff/orchestrator-*` branch's doc (`docs/handoff/`): it holds the current priority
   order, open PRs and hazards. The skill holds no priority list. Done when you know what is in flight and what is next.
2. **Ticket.** Open or pick a GitHub issue (`docs/agents/issue-tracker.md`, labels in `docs/agents/triage-labels.md`).
   Links are real GitHub links, never comment lists:
   - sub-issue: `gh api -X POST repos/synpoox/resurrected-launcher/issues/<parent>/sub_issues -F sub_issue_id=<id>`
   - blocker: `gh api -X POST repos/synpoox/resurrected-launcher/issues/<n>/dependencies/blocked_by -F issue_id=<id>`

   `<id>` is the issue's database id (`gh api repos/synpoox/resurrected-launcher/issues/<n> --jq .id`), not its number.
   Keep the links current. Done when the issue states the goal and its done-condition, and its links exist.

3. **Brief.** Write a short brief: the issue number, the goal, files to read, what is out of scope.
   A brief that may let the agent create issues says so, and tells it to link them as above. Tell the worker never to wait on CI, neither in the
   foreground nor in a background loop: it pushes, opens the PR and reports. Reviewers get `isolation: worktree` and
   never check out in your tree. Tell workers: one full merge per commit (reuse a worktree's or CI's output instead of
   re-merging; limits #820, #821), compare merge speed by `time.process_time` not wall time, expect CI to queue when
   4 agents merge, and commit or delete scratch files before a worktree is removed. Done when the brief names the issue and
   holds no rule that a doc already states.
4. **Implement.** Launch the implementer. It codes, then pushes and opens the PR (`Closes #<n>`), then reports.
   When the report arrives, stop the worker (standing rule "Finished agents").
   Done when the report names the PR, the head SHA and the check result.
5. **Verify.** Read the report against the issue and the PR diff yourself. Check the CI result for the head
   (`unit` is fast; the gate may queue for a merge slot). Done when the PR matches the issue and the checks you rely on ran on the head.
6. **Review.** Launch one Opus reviewer with the PR and the builder's notes. It posts one PR comment.
   Done when the comment is posted.
7. **Fix.** At most 2 implementer -> reviewer cycles per PR. Findings left after cycle 2 become a follow-up issue.
   Done when the reviewer has no blocking finding, or the leftovers are filed.
8. **Merge.** Squash only, per the AGENTS.md gate rule. Done when the PR shows merged.
9. **Clean up.** Run `gh pr list --base <branch>` first: retarget any PR based on the branch, then delete the
   branch on the remote and locally (never `research/*`, `issue-assets` or `handoff/*`). Remove the agent worktree. Done when the branch and worktree are gone. Merge with `gh pr merge --squash` without
   `--delete-branch`; delete the remote branch with `git push origin --delete <branch>`.

## Standing rules

- **Runners:** never start, stop or restart the self-hosted Actions runners. Report offline ones; use the local gate
  (AGENTS.md) as the fallback.
- **Finished agents:** after any agent's final report (implementer, reviewer, fix-cycle worker), stop it with
  TaskStop and kill its shells. A finished agent left running waits on its own background work for hours.
- **Owner messages:** send only a vital update or a blocker, one line per item.

## Session end

1. Stop new dispatches. Dismiss each worker at a safe point: its work pushed, its game stopped, its slot unlocked.
   Keep a worktree that holds uncommitted work and name it in the handoff.
2. Push a handoff doc on an orphan branch `handoff/orchestrator-<date>` (add `-pm` for a second one in a day) under
   `docs/handoff/`. It holds: main's SHA, merged PRs, each open PR with its state and next step, the priority
   order, hazards, and leftover worktrees. The branch is never merged.
3. Done when the branch is pushed.
