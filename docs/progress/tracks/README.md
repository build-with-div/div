# Track progress

Each alpha track (see the [alpha plan](../../alpha/plan.md) and
[workspace instructions](https://github.com/zhuowcui/open-ui/blob/1b73689055aa8c398c14b2eeea636ebe0bec04b3/AGENTS.md)) keeps one progress file here,
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
