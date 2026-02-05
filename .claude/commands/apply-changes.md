Implement changes defined in `openspec/changes/$ARGUMENTS`.

## Workflow
1. Read `claude.md` and `agents.md` for context and tools
2. Review the change spec in the specified changes folder
3. For Svelte files, read the Svelte skill before modifying
4. Apply changes following spec and skill guidelines
5. Verify alignment with `tasks.md` and `design.md`

## Rules
- ASK before deciding on ambiguity or branching decisions
- If implementation deviates from `tasks.md` or `design.md`, inform user first
- Always consult relevant skill files for specific file types

## On Compaction
Instruct continuing agent to re-read `tasks.md` and `design.md`.