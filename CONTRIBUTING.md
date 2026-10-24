# Contributing to BrewSquirt

Thanks for helping make BrewSquirt faster, smaller and more correct. This guide covers everything
you need to get a pull request merged.

## Workflow

1. Branch from an up-to-date `main`:

   ```sh
   git switch main
   git pull
   git switch -c feat/optional-catch-binding
   ```

   Use a prefix that matches the change: `feat/...`, `fix/...`, `docs/...`, `refactor/...`,
   `perf/...`, `test/...`, `ci/...`, `chore/...`.

2. Make your change, add tests, and run the [local checks](#local-checks).
3. Add an entry to `CHANGELOG.md` under `## [Unreleased]`.
4. Push the branch and open a pull request against `main`.
5. Wait for a green CI run and an approving review from the code owner, then it gets squash-merged.

## Pull request titles

Pull requests are **squash-merged only**, so the PR title becomes the commit message on `main`.
Titles must follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) and are
checked by the `pr-title` job.

```text
<type>(<optional scope>): <lowercase summary>
```

Allowed types: `feat`, `fix`, `docs`, `chore`, `refactor`, `perf`, `test`, `ci`, `build`, `style`, `revert`.

Examples:

```text
feat(parser): support optional catch binding
fix(vm): keep upvalues alive after closure returns
perf(compiler): fold constant string concatenation
docs: document module resolution order
feat!: change process.argv layout
```

Use `!` after the type, or a `BREAKING CHANGE:` footer in the description, for breaking changes.
Individual commit messages on your branch are not checked, since they disappear in the squash.

## Required checks

A pull request can be merged only when all of these pass:

- `ci-ok`: aggregates formatting, clippy, tests on Linux, macOS and Windows, beta toolchain,
  MSRV (Rust 1.86), docs and examples.
- `pr-title`: the title is a valid Conventional Commit.
- `changelog`: `CHANGELOG.md` was changed in the PR.
- An approving review from the code owner (@00000kkkkk), as defined in `.github/CODEOWNERS`.

## Changelog

Every user-visible change gets a line in `CHANGELOG.md` under `## [Unreleased]`, in the matching
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) section: `Added`, `Changed`,
`Deprecated`, `Removed`, `Fixed` or `Security`.

```markdown
## [Unreleased]

### Fixed

- `for...in` no longer visits deleted keys.
```

Changes with nothing to tell users, such as CI tweaks or internal refactors, can skip this by
asking a maintainer to apply the `no-changelog` label. Dependabot pull requests are exempt.

## Code style

- **No comments.** Code must be self-explanatory: no `//`, no `/* */`, no doc comments. Express
  intent through names, types, small functions and tests. Pull requests that add comments will be
  asked to remove them.
- Format with `cargo fmt`. The configuration is in `rustfmt.toml` (edition 2024, max width 100).
- `cargo clippy --all-targets --all-features -- -D warnings` must be clean. Do not silence lints
  with `allow` attributes unless there is no reasonable alternative.
- Keep the MSRV at Rust 1.86. Do not use newer standard library APIs or language features.
- New dependencies must use a license allowed in `deny.toml` and come from crates.io.
- Prefer returning errors over panicking. A `.jsq` program must never be able to crash the VM.

## Adding a test

Language behavior is tested with script tests. Each test is a pair of files in `tests/scripts/`:

- `tests/scripts/<name>.jsq`: the program to run.
- `tests/scripts/<name>.out`: the exact expected stdout.

For example, `tests/scripts/nullish_assign.jsq`:

```js
let config = { retries: null }
config.retries ??= 3
config.timeout ??= 1000
console.log(config.retries, config.timeout)
```

and `tests/scripts/nullish_assign.out`:

```text
3 1000
```

The test harness runs every `.jsq` file in `tests/scripts/` with `jsq` and compares stdout with
the matching `.out` file. Run them with:

```sh
cargo test --all-features
```

Guidelines:

- Name the files after the behavior under test, in `snake_case`.
- Keep each script focused on one feature, with edge cases included.
- When fixing a bug, add a script that fails before the fix and passes after it.
- Compare against Node.js when in doubt: for supported features, output should match.

Rust unit tests are welcome for internals such as the lexer, parser or bytecode encoding.

## Local checks

Run the same checks as CI before pushing:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --locked
cargo +1.86 check --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

Optional, also run in CI:

```sh
cargo deny check
typos
cargo build --release
for f in examples/*.jsq; do ./target/release/jsq "$f" || echo "FAILED: $f"; done
```

## Signed commits

Signing your commits is recommended. See the GitHub guide on
[signing commits](https://docs.github.com/en/authentication/managing-commit-signature-verification/signing-commits)
for SSH or GPG setup.

## Reporting bugs and requesting features

Use the issue forms on GitHub. For security problems, follow [SECURITY.md](SECURITY.md) and do not
open a public issue.

## License

By contributing, you agree that your contributions are licensed under the
[Apache License, Version 2.0](LICENSE).
