# Track progress

Each alpha track (see the [alpha plan](../../alpha/plan.md) and
[workspace instructions](https://github.com/zhuowcui/open-ui/blob/29104c1575a21d9818ea60db238cf16ff125f03e/AGENTS.md)) keeps one progress file here,
`<track>.md`: `setup.md`, `engine.md`, `framework.md`, `components.md` and,
later, `docs.md`. A track edits only its own file, so parallel PRs do not
conflict. The history before the alpha tracks stays in the archived Open UI
repository.

Add the newest entry at the top. Each entry has a dated heading and states:

- what changed, and the PR that merged it;
- the owning subsystem and exact source;
- each verification command, where it ran (Windows, WSL or CI) and its
  result;
- what remains failing or unmeasured.

Put a long record of one change in its own dated file under
`docs/progress/` and link it from the entry.
