# Repository Guidelines

## Project Structure & Module Organization
This repository is currently uninitialized: there are no source, test, or tooling files yet. Keep the layout simple as the project takes shape:

- `src/` for application code
- `tests/` for automated tests
- `assets/` for static files such as images or fixtures
- `docs/` for design notes, architecture decisions, and onboarding material

Prefer small, focused modules. Mirror `src/` structure inside `tests/` when possible so ownership stays obvious.

## Build, Test, and Development Commands
No build system is configured yet. When adding one, document the minimum local workflow in the project root and keep commands consistent across contributors. A reasonable baseline is:

- `make dev` or `npm run dev` to start local development
- `make test` or `npm test` to run the default test suite
- `make lint` or `npm run lint` to enforce style and static checks
- `make build` or `npm run build` to produce a releasable artifact

If you introduce a new toolchain, update this file in the same change.

## Coding Style & Naming Conventions
Match the dominant style once the codebase exists. Until then:

- Use 2 spaces for YAML/JSON/Markdown and 4 spaces for Python
- Use descriptive names: `user_service.py`, `order-summary.ts`, `tests/test_auth.py`
- Prefer `snake_case` for Python files and functions, `camelCase` for JavaScript/TypeScript variables, and `PascalCase` for classes/components

Adopt formatter and linter configs early and commit them with the first implementation files.

## Testing Guidelines
Add tests with each behavior change. Name tests after the behavior they protect, for example `test_rejects_empty_email` or `auth.spec.ts`. Keep fast unit tests under `tests/` and reserve slower integration checks for clearly labeled suites.

Run the smallest relevant test set before opening a PR, then run the project’s default full test command before merging.

## Commit & Pull Request Guidelines
There is no commit history yet, so use concise Conventional Commit messages such as `feat: add exchange rate parser` or `fix: reject invalid currency codes`.

PRs should include:

- a short problem statement
- the chosen approach and any tradeoffs
- verification steps with exact commands
- screenshots or sample output when UI or CLI behavior changes

## Security & Configuration Tips
Do not commit secrets, local env files, or production data. Keep configuration in documented environment variables and provide a sanitized example file such as `.env.example` when configuration is introduced.
