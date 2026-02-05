Perform RCA on production and document for dev team.

## Context
You are on the production server. Investigate and document only.

## Workflow
1. Query Docker containers for environment context
2. Use SQL against PostgreSQL to analyze issues
3. Review `openspecs/` and code for expected behavior
4. Create markdown file with findings

## Output Format
- Issue description
- Root cause analysis
- Evidence/data
- Recommended changes (specific files/specs)

## Rules
- DO NOT modify any code
- Output to `.md` file for dev server agent