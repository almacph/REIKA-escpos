Gather development context for implementing: $ARGUMENTS

## Objective
Analyze the user story and collect all relevant context needed to generate a proper change spec.

## Workflow

### 1. Understand the Story
- Parse the user story/request
- Identify affected domains, features, and components

### 2. Read Core Documentation
- `claude.md` - Available tools and conventions
- `agents.md` - Agent capabilities
- `tasks.md` - Current task status and priorities
- `design.md` - Architecture and design decisions

### 3. Find Related Specs
- Search `openspecs/` for relevant specifications
- Identify any existing changes in `openspecs/changes/` that relate
- Note dependencies or conflicts

### 4. Identify Affected Code
- List files/components that will likely need modification
- Read key files to understand current implementation
- Check for related tests

### 5. Check Skills
- Identify which skills apply (Svelte, API, database, etc.)
- Note any skill-specific guidelines relevant to this story

## Output
Produce a structured summary:
```
## User Story
{restated story}

## Affected Areas
- Components: ...
- Specs: ...
- Files: ...

## Current State
{brief summary of how it works now}

## Relevant Constraints
{from design.md, specs, or skills}

## Questions/Clarifications Needed
{anything ambiguous before proceeding}

## Recommended Approach
{suggested implementation strategy}
```

## Rules
- ASK user to clarify ambiguous requirements before finalizing context
- Do NOT create changes yet—this is context gathering only
- Flag any conflicts with existing specs or design