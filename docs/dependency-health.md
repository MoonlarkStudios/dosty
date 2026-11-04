# Dependency health

`Cargo.toml` and `Cargo.lock` define the application's dependency graph. Run
`mise run audit` against current RustSec data; advisory results can change without
a repository change. A previous passing run does not establish current safety.

## Audit policy

The [policy file](../.cargo/audit-exceptions.json) is authoritative for approved
advisories, exact package versions, reasons, and review deadlines. Exceptions are
limited maintenance decisions, not blanket advisory ignores. Dependency changes
require reviewing their full-platform paths and the applicable exception.

The audit rejects unapproved findings, changed versions, expired or stale
exceptions, vulnerability/unsoundness findings, yanked releases, and external
ignore lists. Policy tests run through `mise run check-repo`. The audit's output
keeps approved findings visible.

```sh
mise run audit
mise exec -- cargo tree --locked --target all -i PACKAGE
```

`mise run check-deps` checks unused direct application dependencies. Clippy checks
application warnings. Neither replaces advisory review or proves every reachable
code path necessary or correct.

## Vendored patches

[vendor/README.md](../vendor/README.md) records patch provenance, checksums,
licenses, scope, validation, and removal criteria. Preserve those records when
rebasing a dependency; retain upstream formatting and keep application lint
cleanup out of imported source.

The [Yrs pending-update repair](yrs-pending-updates.md) documents the correctness
invariant and deterministic regression needed during rebases. Renderer and
highlighter manifest patches remain governed by the vendor policy.

The macOS [Source accessibility bridge](accessibility.md) also uses a narrow
window-runtime patch to install AccessKit before a native window becomes visible.
Its provenance and removal criteria remain in the vendor policy. The bridge's
macOS dependencies do not enable Windows accessibility or Unix adapter runtime
features; the lockfile still records AccessKit's declared Windows dependencies.

## Evidence

The [2026-11-03 parser audit](validation/dependencies/pulldown-cmark-0.13.6-audit.txt)
and [earlier audit capture](validation/dependencies/audit.txt) are dated records,
not live status. [Dependency-update measurements](performance/dependencies.md)
retain the matching profiles and limitations.

The [2026-11-04 prototype audit](validation/accessibility/audit.txt) belongs to
that recorded initial implementation. It is historical evidence rather than a
statement about the current advisory database.
