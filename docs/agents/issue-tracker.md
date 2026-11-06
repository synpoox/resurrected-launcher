# Issue tracker: GitHub

Issues and specs live in synpoox/launcher-clean on GitHub. Use the gh CLI.

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
