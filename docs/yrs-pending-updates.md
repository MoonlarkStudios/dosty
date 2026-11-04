# Yrs pending-update repair

The pinned Yrs patch retries an operation when its actual missing dependency
arrives. Yrs can integrate independent operations beyond a clock gap; recording
only that client's highest clock can leave a dependent operation pending even
after the earlier gap is filled.

The [patch](../vendor/yrs.patch) records the missing operation ID and checks its
presence before retrying, including origins, parent items, and optional weak
references. It preserves public method signatures, update V1/V2 encoding, text
indexing, and garbage-collection policy. The cached UTF-16 traversal optimization
is a separate part of the patch.

## Regression and maintenance

The deterministic regression creates `a😀` in one shared text, `z` in an
independent text, then appends `b` to the first. Delivery of the independent
transaction and append before the first transaction exposes the stranded update.
It checks both update formats, all delivery orders, duplicate pending delivery,
final text, and the converged state vector.

```sh
mise exec -- cargo test --manifest-path vendor/yrs/Cargo.toml --locked --features sync --lib pending_dependency_retries_after_filling_an_earlier_clock_hole
mise run check-yrs
```

Keep this regression when rebasing Yrs. Remove the retry patch when a compatible
upstream release includes the repair, following the [vendor policy](../vendor/README.md).
Use separate Cargo target directories for patched/unpatched reproductions;
same-version source trees can otherwise reuse a test executable.

The [unpatched failure](validation/dependencies/yrs-regression-before.txt),
[repeated patched runs](validation/dependencies/yrs-fixed-runs.txt), and
[original diagnostics](validation/repository/yrs-array.txt) preserve the
2026-10-25 evidence. Dosty's bounded plain-text import schema is narrower than
that generic upstream reproduction; the reproduction alone does not establish
a user-visible Dosty data-loss case.
