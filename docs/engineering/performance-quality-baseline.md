# Performance and Quality Baseline

## Problem redefinition

Kimix is a multi-layer Rust application. Performance work should optimize user-visible work rather than isolated microbenchmarks.

> Minimize user-visible latency and resource consumption while preserving rendering correctness, agent behavior, state integrity, and failure recovery.

## MECE decomposition

1. **Interactive path**: input handling, state transitions, command dispatch, and first-response latency.
2. **Rendering path**: per-frame work, layout, diffing, allocation, and terminal output.
3. **Agent/runtime path**: tool orchestration, streaming, subprocess/network waits, and cancellation.
4. **Resource path**: allocations, cloning, memory growth, I/O volume, and concurrency overhead.
5. **Correctness/reliability**: state consistency, terminal behavior, errors, cancellation, and degraded environments.

## Measurement rules

Use deterministic fixtures where possible. Separate one-time initialization from steady-state work. Report median and tail latency, not only mean. For UI rendering, measure per-frame cost independently from startup. For agent operations, separate local CPU time from external wait time.

## P1 optimization gate

A P1 change requires:

- evidence that the target dominates CPU, wall-clock, memory, or tail latency
- a falsifiable optimization hypothesis
- before/after or ablation measurements
- unchanged observable behavior on a representative fixture set
- regression coverage for the affected state/path

Existing benchmarks should be extended when possible instead of creating isolated one-off numbers.

## Experiment loop

`baseline -> profile -> hypothesis -> single-variable change -> ablation -> correctness regression -> scaling/tail test -> retain/revert`

The goal is to make performance claims comparable and falsifiable across future Kimix changes.
