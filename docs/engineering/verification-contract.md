# Verification contract

Kimix changes are accepted only when the evidence answers four separate questions:

1. **Correctness:** do the intended behaviors pass, including the failure path?
2. **Regression:** did unrelated behavior remain unchanged?
3. **Resource cost:** did the change add measurable work in token, wall-clock, memory, network, or build time?
4. **Operational safety:** can cancellation, shutdown, persistence, permissions, and network boundaries still reach a terminal state?

A green unit-test suite is evidence for only the assertions it executes. It is not evidence that an integration path, cancellation path, or performance budget is healthy.

## Change loop

For a bug or regression:

1. Reproduce the failure with the smallest deterministic case available.
2. Identify the observable outcome that is wrong.
3. Add a regression test at the narrowest stable seam.
4. Make the smallest implementation change that fixes that outcome.
5. Run the complete verification gates.
6. Measure any claimed performance or resource improvement against the same baseline and workload.
7. Remove abstractions or work that do not improve a measured outcome.

## Quantitative evidence

When a PR claims a performance improvement, report at least:

- workload and environment
- baseline and candidate values
- sample count or repeated runs
- median for latency-like metrics
- peak or high-percentile values when tail behavior matters
- correctness/pass rate alongside the performance number

Do not use token reduction, line-count reduction, a single successful test, or a single local timing as a proxy for overall quality.

## CI feedback contract

The CI gate runner executes all independent gates even after one fails and reports the duration of each gate in the job summary. This separates **failure discovery** from **failure propagation**: a broken clippy check should not hide a failing test or dependency audit.

The contract intentionally does not impose arbitrary performance thresholds until a reproducible workload baseline exists. Thresholds without a baseline create false confidence and encourage optimizing the measurement instead of the product.
