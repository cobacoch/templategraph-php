# Contributing to templategraph-php

Thank you for your interest in contributing! This document describes the
workflow, conventions, and expectations for contributions to this project.

## Reporting Issues

- Search [existing issues](https://github.com/cobacoch/templategraph-php/issues)
  before opening a new one.
- For bug reports, include reproduction steps, expected behavior, actual
  behavior, and your environment (OS, PHP version, Composer version).
- For feature requests, describe the use case and the problem you are trying
  to solve, not just the proposed solution.

## Development Workflow

This project follows **GitHub Flow**:

1. Fork the repository and create a topic branch from `main`.
2. Make your changes with focused, atomic commits.
3. Push your branch and open a Pull Request against `main`.
4. Address review feedback if any.
5. Once approved and CI passes, the PR will be squash-merged.

`main` is always kept in a buildable, test-passing state.

## Branch Naming

Use the format `<type>/<short-description>`, where `<type>` matches one of
the Conventional Commits types:

| type | purpose |
|---|---|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `refactor` | Code change without behavior change |
| `test` | Adding or updating tests |
| `chore` | Build, config, dependency updates |
| `perf` | Performance improvement |
| `ci` | CI configuration |
| `release` | Release preparation |

`<short-description>` should be in kebab-case. Examples:

- `feat/scan-subcommand`
- `fix/cyclic-deps-handling`
- `docs/add-contributing`

## Commit Messages

Each commit message follows this format:

```
<gitmoji> <subject>

[optional body explaining the why]

[optional footer]
```

The Conventional Commits `<type>:` prefix is intentionally **omitted** because
the gitmoji already conveys the type.

### Subject conventions

- Imperative, present tense (`Add`, `Fix`, `Update` — not `Added`, `Fixes`)
- Uppercase first letter (the word right after the gitmoji)
- No trailing period
- 50 characters or fewer when possible

### Gitmoji to type mapping

| type | gitmoji | code | usage |
|---|---|---|---|
| feat | ✨ | `:sparkles:` | New feature |
| fix | 🐛 | `:bug:` | Bug fix |
| docs | 📝 | `:memo:` | Documentation |
| docs (legal) | 📄 | `:page_facing_up:` | LICENSE / NOTICE etc. |
| refactor | ♻️ | `:recycle:` | Code restructuring |
| test | ✅ | `:white_check_mark:` | Tests |
| chore | 🔧 | `:wrench:` | Build / config |
| chore (deps) | ⬆️ | `:arrow_up:` | Dependency updates |
| perf | ⚡ | `:zap:` | Performance |
| ci | 👷 | `:construction_worker:` | CI configuration |
| release | 🔖 | `:bookmark:` | Version tag |
| init | 🎉 | `:tada:` | Initial commit |

### Examples

```
✨ Add scan subcommand with --format option
🐛 Handle cyclic dependencies in graph builder
📄 Add LICENSE-MIT and LICENSE-APACHE
🔧 Configure cargo-deny with allow list
✅ Add fixture for unresolved deps
```

## Pull Requests

- PR titles follow the same `<gitmoji> <subject>` format as commit messages
  (the squash-merge commit will inherit the title).
- The PR body should include a short summary of the change, the motivation,
  and a checklist for testing.
- Reference related issues with `Closes #N` or `Refs #N` where applicable.

## License of Contributions

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual-licensed as `MIT OR Apache-2.0`, without any
additional terms or conditions.
