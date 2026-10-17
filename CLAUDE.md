# Claude Code instructions

@AGENTS.md

`AGENTS.md`, imported above, is the canonical project and coding-agent guidance. Read `README.md` and `ROADMAP.md` at the start of a task. If instructions conflict, follow `AGENTS.md` and the user's current request.

## Review before committing

When a change is complete and its checks pass, run the `critic` agent (`.claude/agents/critic.md`) on it before committing. If that agent type is not available in the session, give a general-purpose agent the instructions in that file. Check each finding yourself, then fix it or say why not. Run the critic once per pull request, not after every edit.

## Working through the roadmap

The next work is the first unchecked item in the current milestone's task list in `ROADMAP.md`, skipping items that start with "Maintainer:", which only the maintainer can do. "Continue" means that item. Check `gh pr list` and the branches first, since earlier work may be open or merged.

Standing permission, so the work does not wait on routine questions:

- Once `scripts/check.sh` passes (`--app` or `--ui` for app changes) and the critic's findings are handled, commit, push the branch, and open a pull request without asking. Tick the item in `ROADMAP.md` in the same pull request.
- Then start the next item. If the pull request it depends on is still open, branch from that pull request's branch and say so in the new pull request. Before opening a pull request on top of another, check the earlier one's CI with `gh pr checks`; if it fails, fix that first.
- Never merge a pull request, push to `main`, force-push, or delete a branch you did not create in the session.
- When an accepted ADR or the task list already settles the design, implement it; do not run a brainstorming or design-question step first.

Stop and ask only when:

- a design decision is not settled by an ADR, the roadmap, or an obvious convention;
- the work would leave the current milestone or change the roadmap;
- an action is destructive or outward-facing beyond pushing a feature branch and opening its pull request;
- checks fail and the cause is outside the change, or two attempts at a fix have not worked.
