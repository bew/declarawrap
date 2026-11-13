# value-transforms — exploration

## Motivation

Two transform-ish features surfaced while the config surface moved to colon-namespace KDL ops:
args `transform "word-split"` and env `set`'s `eval "env-vars"`.
The direction is a generic *value transformation* mechanism that unifies them — an ordered,
composable list of named transforms, attachable per value and overridable by a document default.

Scope: the model and KDL syntax. Out of scope: each transform kind's runtime implementation.

## Findings

- Transform kinds differ in arity and target: `word-split` maps 1 string → N argv entries,
  while `expand-env` maps 1 string → 1 string.
- KDL v2 forbids duplicate property names on one node, so a list cannot be spelled as repeated
  `transform=` properties; a comma-joined string is the compact encoding.
- Implemented model: `ValueEntry { text, transforms }` carries the value plus its resolved
  transform list, used by `env:set`, `env:prepend`/`env:append`, and
  `args:prepend`/`args:append`; `ValueTransformation { WordSplit, ExpandEnv }` is the shared enum.

## Design crux

Model value transformations generically — one shared kind set, composable and applyable at both
per-value and document scope — without over-fitting to the env-vs-args split, while keeping the
default cascade unambiguous.

## Candidate directions

- (active) **Comma property + default node** — `transform=foo,bar` on value nodes, plus a
  `default-transform foo,bar` node supplying the list for values that carry none.
- (rejected) **Repeated `transform "…"` child nodes** — more KDL-idiomatic and lets a kind carry
  its own argument, but verbose for the common multi-kind case.
- (rejected) **Per-domain enums** — separate kind sets for env vs args.

## Decisions

- (settled) **Shared kind set** — one transform enum valid in both env and args domains; validity
  beyond name-checking is not enforced.
- (settled) **Composable list** — a value carries an ordered list of transforms, applied
  left-to-right.
- (settled) **Syntax** — a comma-separated `transform=` property on value nodes; a
  `default-transform <list>` node supplies the default.
- (settled) **Override semantics** — a value's own `transform=` replaces the default list
  entirely; defaults apply only when the value carries no `transform=`.
- (settled) **`eval "env-vars"` meaning** — substitute `$VAR` / `${VAR}` at runtime from the
  wrapper process's environment; modelled as kind `expand-env` (alias `env-vars`).
- (settled) **`word-split`** — retained as a transform kind (same shared set).
- (settled) **Typed enum** — a closed Rust enum (`WordSplit`, `ExpandEnv`); unknown names are
  rejected at parse.
- (settled) **Arg-less kinds** — no kind takes an argument; dialects, if ever needed, become
  separate kinds (e.g. a future `expand-shell`).
- (settled) **`default-transform` scope** — declared per op block; applies to that op's value
  nodes only.
- (settled) **Value carriers on sequence ops** — every env/args sequence op takes values from
  `-` child nodes and/or a `value` node (single-item spelling), mixed in textual order.
- (settled) **`env:set` separator** — `sep` is optional on `env:set` and required only with 2+
  values; `env:prepend`/`env:append` keep a required `sep`.

## Open threads

- Whether the kind set stays minimal or gains `read-file`/tilde/shell
  expansion in v1 (the old `@file` FIXME).
- Ordering once more than one kind composes (runtime concern).
- With override-only semantics a value cannot extend a default; confirm
  that is acceptable or add an explicit append form.
- `expand-env` interaction with `if-set` ordering, and with variables set
  earlier in the same config.
