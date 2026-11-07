# Issue tracker: GitHub

Issues and specs live in synpoox/resurrected-launcher on GitHub. Use the gh CLI.

- Before creating issues, verify the target with `gh repo view --json nameWithOwner`.
- Create issues with `gh issue create`. For multiline bodies, write a temporary file and pass `--body-file`.
- Read issues with `gh issue view <number> --json number,title,body,labels,comments`.
- List issues with `gh issue list`, using appropriate state and label filters.
- Update labels with `gh issue edit <number> --add-label` or `--remove-label`.
- Before closing an issue, post the explanation with `gh issue comment`, then run `gh issue close`.
- Use native sub-issues and dependencies where supported. Otherwise, record `Part of #<parent>` and `Blocked by: #<number>` in issue bodies.
- Publish to the issue tracker by creating a GitHub issue; fetch tickets by reading the referenced issue.

File issues only in this repo unless the user explicitly names another destination.

## Pull requests as a triage surface

**PRs as a request surface: no.**

## Wayfinding operations

- A map is one issue labelled `wayfinder:map`; its decision tickets are native GitHub sub-issues labelled `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`, or `wayfinder:task`.
- Link children with `POST repos/synpoox/resurrected-launcher/issues/<map>/sub_issues`, passing the child's database ID as `sub_issue_id`.
- Add blockers with `POST repos/synpoox/resurrected-launcher/issues/<ticket>/dependencies/blocked_by`, passing the blocker's database ID as `issue_id`. Fetch database IDs with `gh api repos/synpoox/resurrected-launcher/issues/<number> --jq .id`.
- Query children with `GET repos/synpoox/resurrected-launcher/issues/<map>/sub_issues`. In child order, the frontier is open, unassigned tickets with no open native blockers. Inspect blockers with `GET repos/synpoox/resurrected-launcher/issues/<ticket>/dependencies/blocked_by`.
- Claim a ticket by assigning the driving developer before work. Use only wayfinder labels on maps and decision tickets.
- Resolve by posting a resolution comment, closing the ticket, and appending its named link and one-line gist to the map's Decisions so far. Keep detailed findings in the ticket or its linked assets.
