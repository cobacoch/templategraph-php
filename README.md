# templategraph-php

## JSON output schema

`templategraph scan --format json` produces a graph document whose shape is
documented as a JSON Schema at
[`schemas/output-graph-v1.schema.json`](schemas/output-graph-v1.schema.json).
The current `schema_version` is `1`.

`templategraph scan` exit codes:

| Code | Meaning                                                                  |
|------|--------------------------------------------------------------------------|
| `0`  | Clean success — every include resolved.                                  |
| `1`  | Fatal error — no graph was produced (config / I/O / etc.).               |
| `2`  | Warning-success — graph produced, but at least one include is unresolved.|

When the exit code is `2`, the unresolved findings are written to stderr as
`warning:` lines; stdout still holds the full graph, so consumers can pipe
it into downstream tooling regardless of code.

**Schema evolution policy.** Schema files are frozen on publish. Every object
in the schema uses `additionalProperties: false`, so any change to the JSON
wire shape — breaking *or* non-breaking — ships as a new
`output-graph-vN.schema.json` alongside the existing files rather than as an
in-place edit. Consumers can pin to a specific version with confidence and
branch on `schema_version` to migrate between versions.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the
development workflow, branch naming, commit message conventions, and how
contributions are licensed.
