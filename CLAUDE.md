# Claude Code instructions

@AGENTS.md

`AGENTS.md`, imported above, is the canonical project and coding-agent guidance. Read `README.md` and `ROADMAP.md` at the start of a task. If instructions conflict, follow `AGENTS.md` and the user's current request.

## Review before committing

When a change is complete and its checks pass, run the `critic` agent (`.claude/agents/critic.md`) on it before proposing a commit. Check each finding yourself, then fix it or say why not. Run the critic once per pull request, not after every edit.
