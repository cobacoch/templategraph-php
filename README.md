# templategraph-php

## JSON output schema

`templategraph scan --format json` produces a graph document whose shape is
documented as a JSON Schema at
[`schemas/output-graph-v1.schema.json`](schemas/output-graph-v1.schema.json).
The current `schema_version` is `1`. Future breaking changes to the wire
shape will bump the version and ship as `output-graph-vN.schema.json`
alongside the existing files.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the
development workflow, branch naming, commit message conventions, and how
contributions are licensed.
