# CI jobs this package owns

WP-09 owns `crates/liyasa-cli/` and nothing else, and `.github/workflows/`
belongs to whoever owns the repository root. CLI-32 and CLI-35 are both CI
jobs, so the scripts live here and the two workflow steps that call them are a
handoff.

Whoever adds them wants, in `.github/workflows/release.yml`:

```yaml
      - name: Release smoke
        run: crates/liyasa-cli/ci/release-smoke.sh target/release/liyasa

      - name: Artifact budgets
        env:
          LIYASA: target/release/liyasa
        run: crates/liyasa-cli/ci/artifact-budgets.sh artifacts/ previous-sizes.json
```

`artifact-budgets.sh` writes `artifact-sizes.json`, which the next release
passes back as its `previous-sizes.json`; that is how CLI-35's 10% growth rule
is evaluated. The budgets themselves are in `src/budget.rs` and reach the
script through `liyasa budgets --json`, so neither file carries its own copy of
a number.

`release-smoke.sh` needs no network: it scaffolds, builds, and validates a
project in a temporary directory, then checks the linkage.
