# templategraph-php

## JSON output schema

`templategraph scan --format json` produces a graph document whose shape is
documented as a JSON Schema at
[`schemas/output-graph-v1.schema.json`](schemas/output-graph-v1.schema.json).
The current `schema_version` is `1`.

`templategraph scan` writes any unresolved include findings (dynamic
arguments and missing files) to stderr as `warning:` lines but still exits
with status `0`; consumers can pipe stdout straight into a downstream
processor without filtering.

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
