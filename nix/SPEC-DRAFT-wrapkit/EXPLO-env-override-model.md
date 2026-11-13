# env-override-model — exploration

## Findings

- The env schema is per-toolkit and inconsistent (`{set,set-default}` vs plain `str`).
- Only `set`/`set-default` are modeled; `prefix` and `add-flags` are hand-emitted in each toolkit.
- The system revolves around a reusable **wrapper record**: the schema for one wrapper's whole surface — `env`, `flags`, `argv0`, `chdir`, `run` — in makeWrapper vocabulary.
- That record is *installed* at two kinds of site:
  - at the root of an eval, giving the **commons** (e.g. `config.wrapper.*`);
  - nested under a path, giving an **override** scoped to that consumer (e.g. `config.profiles.foo.wrapper.*`).
- The consumer is called a **result** (`profiles.foo.<result-using-us>`): something that builds a wrapped drv and consumes the effective wrapper record.
- The effective record for a result = commons merged with the result's scoped override.
- `makeWrapper` quotes `--set`, `--set-default`, `--prefix`, `--suffix` and `--add-flag` values with `${value@Q}`: literal at build time, no runtime expansion.
  Only `--run` (emitted verbatim) and `--add-flags`/`--append-flags` (also verbatim) reach the wrapper script as shell.
- Script order equals argument order, so a `--run` fragment interleaves predictably between two variable operations.
- `--prefix` removes a duplicate then prepends; `--suffix` skips a value already present.
  Per-element prefix emission must therefore walk the before-pivot run in reverse to keep written order.
- `--prefix-each` loops `for value in $values`, splitting on whitespace, so it is unsafe for values containing spaces.
- `lib.mkBefore`/`lib.mkAfter` apply to list-typed options only — a `str` option fails the eval — work nested under `attrsOf`, and reverse the default (latest-first) list order.
- The emitter target changes to declarawrap (`/home/bew/projects/project-ideas/declarawrap`), a KDL-configured wrapper system, format still in flux.
  Its own model is already an ordered action list: `env:prepend`, `env:append`, `env:set`, `env:unset`, `args:prepend`, `args:append`, `arg0:*`, `chdir`, applied in block order.
- declarawrap carries `ValueEntry { text, transforms }`; its `expand-env` transform substitutes `$VAR` at runtime, so cross-VAR composition is native there rather than generated shell.
- It exposes `if-present` (`keep`/`remove`/`error`/`skip`) and `if-set` (`replace`/`error`/`skip`) where makeWrapper hard-codes its dedup and overwrite rules.
- The makeWrapper facts above are history from here on: the output backend is declarawrap KDL, not makeWrapper arguments.
- declarawrap's `env:set` takes `values` + `sep` + `if_set` (`replace`/`error`/`skip`), so makeWrapper's `--set` vs `--set-default` distinction collapses into one `env:set` plus an `if-set` policy.
- declarawrap's `env:prepend`/`env:append` already carry before/after-ambient direction, so the ambient value (Model E's pivot) is implicit there rather than a list entry.
- The `mini-eval uses plain lib.evalModules` decision this file once carried is superseded by `topic:kit-half`'s `mini-eval-on-kitsys`; the mini-eval runs through `kitsys`.

## Design crux

The problem has two parts that are usually entangled, and should be split in the spec.
The wrapped-drv generator turns a (package, bin, env/arg spec) into a `makeWrapper` drv.
The env-override module system composes the env/arg spec from a commons plus scoped overrides.

What remains genuinely open:
- what a result *is*, concretely;
- whether an override is keyed by the result's name (global tree) or is implicit in the install site.

### Profiles are external

The profile system is a separate, future concept that wrapkit plugs into; wrapkit does not define `profiles.*`.
The profile system's shape, as sketched by the user:

```nix
wrapper.env.FOO.set = "hello";                    # the commons
profiles.standalone.wrapper.env.FOO.set = "over"; # this profile's override
profiles.standalone.pkg;                          # drv built from the effective wrapper
```

So a record option at the host root is the commons, one nested under a profile is that profile's override, and `profiles.<name>.pkg` is the wrapped drv built from the effective wrapper.
The host picks the field names; wrapkit only supplies the option.

Wrapkit's pluggability surface is a **function that runs a small module system** and returns its `output`:
- `mkWrapkitWrapperOption { }` — the record option (the full makeWrapper surface: `env`, `flags`, `argv0`, `chdir`, `run`), usable like any other option definition, at any site and under any name.
- `mkWrapkitBin { wrappedBin, ... }` — applies the given wrapper records to a small wrapkit module system and returns its `output` (the wrapped drv).

`mkWrapkitBin` takes the wrapper record in one of two ways:

- **inline mode** — `wrapper = { ... }` written on the call: one definitions record, merged by ordinary module priority;
- **layered mode** — `wrappers = [ <wrapper record> … ]`, an ordered list of already-resolved records (e.g. `config.wrapper` plus a profile's) normalised and then applied one definition apiece.

The rest of the inputs:
- `wrappedBin` — the bin to wrap: either a path string (`"${somepkg}/bin/foo"`) or a derivation, resolved with `lib.getExe`.
- `binName` — the command name; a non-null value (default derived from `wrappedBin`) selects the `$out/bin/<binName>` layout, `null` selects the single-file `$out` layout.

Rough draft:

```nix
mkWrapkitBin =
  { wrappedBin
  , binName ? <derived from wrappedBin>
  , wrapper ? null      # inline mode
  , wrappers ? null     # layered mode
  , ...
  }@args:
  let
    records = if wrappers != null then wrappers else [ wrapper ];
  in
  (lib.evalModules {
    modules = [ ./wrapkit-base.module.nix ]
      ++ map (r: { wrapper = normalise r; }) records;
    specialArgs = { inherit wrappedBin binName; };
  }).config.output;
```

The base module declares `wrapper` (the full surface) and an `output` option; `config.output` builds the drv.
Each record enters as its own definition of the `wrapper` option, so the second module application does the merging: lists concatenate, and a scalar or `sep` set by two records is a genuine conflict the eval rejects.
Inline mode is that same path with a single record, where `lib.mkForce` is available because the record is written as definitions.

The one non-module step is `normalise`, which runs in layered mode only.
A resolved record carries every field, most of them `null` (and `unset = false`), and each of those would register as a definition and collide with another record's real one.
Dropping the null fields before the application leaves only what a layer actually said.

A host then wires the external profile system to it.
The host's `config.wrapper` and `config.profiles.<name>.wrapper` are resolved *values* in the host eval, so they cross the boundary through layered mode:

```nix
profiles.<name>.pkg = mkWrapkitBin {
  wrappedBin = "${pkgs.tmux}/bin/tmux";
  wrappers = [ config.wrapper config.profiles.<name>.wrapper ];
};
```

Because the second application is ordinary module merge, layer *order* decides list order only.
A scalar cannot be overridden by a later layer — precedence is expressed by the action (`setDefault` in the commons, `set` in the profile) — and a same-field clash across layers is an authoring error.

Inline mode is the form for a call that owns its record outright:

```nix
mkWrapkitBin {
  wrappedBin = "foo/tmux";
  wrapper = {
    env.FOO.set = "hello";
  };
};
```

A module *reference* cannot cross that boundary: the module system merges definitions, and offers no way to read an option's definitions back out of a resolved `config.*`.
What crosses is a value, which is why layered mode normalises first instead of merging module references.

### One ordered list per VAR

A single ordered `val` list per VAR makes position explicit.
The placeholder entry is the pivot — the variable's ambient value — so everything before it lands in front and everything after behind.
That removes `prepend`/`append`/`beforeArgs`/`afterArgs` from the schema without losing the distinction they encode.

Two consequences follow.
Cross-VAR entries are the only backend-sensitive case: makeWrapper would need generated shell, while declarawrap already spells them as a value entry with the `expand-env` transform.
And module-priority ordering exists only where definitions share one eval — but a `val` list still carries its own order as a value, so a resolved record crosses a layer boundary intact.
Layered mode is a boundary concern, not an ordering one: each layer arrives ordered, layers concatenate in layer order, and the existing limits stand — a later layer cannot place a value before an earlier layer's, nor clear it.
Lifting that would be a deliberate marker, not a side effect of `mkBefore`/`mkAfter`.

## Candidate directions

- **Model A — record option constructor.** Exports a reusable `wrapperType` plus `mkWrapkitWrapperOption`, which returns the record option itself — the shape of any other option definition. _(deferred)_

  ```nix
  wrapperType = ty.submodule {
    options = {
      env = lib.mkOption { type = ty.attrsOf envVarType; default = null; };
      flags = ...; argv0 = ...; chdir = ...; run = ...;
    };
  };

  mkWrapkitWrapperOption = { }: lib.mkOption {
    type = wrapperType;
    default = null;
  };
  ```

  - root: `{ options.wrapper = wrapkit.mkWrapkitWrapperOption { }; }` gives `config.wrapper`.
  - nested: `{ options.profiles.foo.wrapper = wrapkit.mkWrapkitWrapperOption { }; }` gives `config.profiles.foo.wrapper`.
  - The field name is the host's choice: the constructor takes no path, so `{ options.profiles.foo.cfg = wrapkit.mkWrapkitWrapperOption { }; }` is equally valid.
  - Override is implicit in *where* the option is declared; the result merges its site's record over the commons.

- **Model B — global results tree.** A single `wrapkit` option tree; results are first-class and own their override. _(deferred)_

  ```nix
  wrapkit.wrapper = { /* commons wrapperType */ };
  wrapkit.results.<name> = {
    wrapper = { /* wrapperType override, keyed by result name */ };
    package = pkgs.tmux;
    bin = "tmux";
  };
  wrapkit.results.<name>.out  # the resulting wrapped drv
  ```

  - The same `wrapperType` backs `wrapkit.wrapper` and `wrapkit.results.<name>.wrapper`.
  - A profile points at or defines result entries.
  - Override is keyed by the result name.

- **Model C — type + normaliser only.** Wrapkit exports `wrapperType` and the normaliser; nothing is auto-installed. _(deferred)_

  ```nix
  normaliseWrapper = record: /* record minus null fields and `unset = false` */;
  ```

  - Every host installs its own record options, at whatever sites it likes, and runs the same module application itself.
  - Most explicit, least magic; the host repeats the install boilerplate Model A would generate.

- **Model D — `mkWrapkitBin` over a small internal module system (user's current direction).** A function applies wrapper records and returns the eval's `output`. _(active)_

  ```nix
  mkWrapkitBin : { wrappedBin, binName ? <derived>, wrapper ? null, wrappers ? null, ... } -> <derivation>;
  # inline mode: one `wrapper` record; layered mode: `wrappers = [ … ]`
  ```

  - `wrappedBin`: a path string, or a derivation resolved with `lib.getExe`.
  - Two record inputs: inline mode (one `wrapper` record, module priority, `lib.mkForce` available) and layered mode (`wrappers` list, normalised then applied, lists concatenate, scalar clashes error).
  - The mini-eval's base module declares `wrapper` (full makeWrapper surface) and `output`; `config.output` builds the wrapped drv.
  - `profiles.*` stays external; the host calls `mkWrapkitBin` from its own profile config, passing its commons record and the profile's record as values.
  - Open: whether the mini-eval is built with `kitsys` or plain `lib.evalModules`.
  - Models A/B/C remain as alternatives.

- **Model E — unified `val` list with placeholder entries.** One ordered list per VAR; a placeholder entry names the ambient value (pivot) or another variable; `flags` take the same shape with the user's own args as pivot. _(active)_

  ```nix
  env.FOO = { sep = ":"; val = [ "/a" { env = "FOO"; } { env = "BAR"; } ]; };
  flags = { val = [ "--verbose" { args = true; } ]; };
  ```

  - The ordered list is the sequence carrier; entries before the pivot lower to declarawrap `env:prepend`, entries after it to `env:append`.
  - `{ env = "BAR"; }` under `FOO` imports another variable's value, spelled in declarawrap as a `ValueEntry` with the `expand-env` transform — no generated shell.
  - `lib.mkBefore`/`lib.mkAfter` order the runs contributed by different records.
  - Open: how `-contents` entries are marked.

- (active) **Merged conflict control** — one per-VAR control covering `ifSet`, `ifPresent`, and `unset`, instead of three separate fields. Floated; the unified semantic and its name are unresolved.

## Decisions

### (settled) `declarawrap-kdl-backend` — the output backend is declarawrap KDL

related-to: `topic:wrapped-drv-generator` & `topic:wrapper-schema`

The wrapped bin is no longer built by `makeWrapper`. `output` becomes a wrapper script whose
shebang is the declarawrap binary and whose body is generated KDL; the wrapper record is a
declarawrap-op model, not a makeWrapper-argv model. The `makeWrapper` facts in this file and in
`SPEC.md` become history.

Consequences: `SPEC.md`'s "Wrapper schema" and "makeWrapper semantics the assembler must respect"
sections are rewritten; `wrapper-model.nix`'s `mkWrapperArgs` is replaced by a KDL emitter;
`wrapkit-base.module.nix`'s builders stop calling `makeWrapper`; `nix/wrapkit/tests.nix`'s
arg-assembly assertions are rewritten against KDL output; `MILESTONES.md` M3/M4/M7 text is
re-derived.

### (settled) `env-val-list-with-dedicated-scalars` — one ordered `val` list per VAR, scalars dedicated

related-to: `declarawrap-kdl-backend`

The per-VAR env schema carries its sequence content in one ordered `val` list (Model E's shape).
Everything that is not sequence content stays a dedicated per-VAR field and is emitted as its own
KDL directive for that var, rather than as a `val` entry: `sep`, `ifSet`, `setDefault`, and `unset`.

#### Example

```nix
wrapper.env.FOO = {
  sep = ":";
  ifSet = "replace";
  setDefault = "fallback";
  unset = null;
  val = [ "/a" { env = "FOO"; } { env = "BAR"; } ];
};
```

### (settled) `env-val-no-pivot-replaces` — a `val` with no pivot lowers to `env:set`

related-to: `env-val-list-with-dedicated-scalars`

A `val` list with no pivot entry lowers to declarawrap `env:set` — a replace — so `set` folds into
`val` and there is no separate `set` field. A pivot entry introduces the ambient value into the
sequence, so entries before it lower to `env:prepend` and entries after it to `env:append`.

### (settled) `record-carries-whole-surface` — the record carries the whole wrapper surface

The record carries `env`, `flags`, `argv0`, `chdir`, `run`; there are no top-level `env` / `flags`
fields on `mkWrapkitBin`.

### (settled) `override-merge-per-action` — override merge is per action

A record that sets one action leaves the other actions of the same VAR inherited.

### (settled) `lists-merge-scalars-conflict` — lists merge, scalars do not

Every accumulating field concatenates across records in layer order (`env.<VAR>.val`,
`flags.beforeArgs`/`afterArgs`, `run`). A scalar (`env.<VAR>.sep`/`set`/`setDefault`/`unset`,
`argv0`, `chdir`) set by two records is a genuine conflict and the eval rejects it.
Precedence comes from the action, not from layer order: `setDefault` in the commons, `set` in the
profile.

### (settled) `two-record-inputs` — `mkWrapkitBin` takes one of two mutually exclusive record inputs

Inline mode `wrapper = { … }` (one definitions record, ordinary module priority, `lib.mkForce`
available) and layered mode `wrappers = [ … ]` (ordered already-resolved records, normalised, then
applied one definition apiece).

### (settled) `schema-no-defaults` — the schema has no defaults

`sep`, `unset` and the list fields all default to `null`. The effective defaults (`":"`, `false`,
`[]`) are applied by the emitter at assembly time, where "absent" means "use the default".

### (settled) `normalizer-strips-nulls` — the normaliser strips nulls

In layered mode only, every `null` field (and `unset = false`) is dropped before the second module
application; a resolved record otherwise registers every field as a definition and collides with
the other records.

### (settled) `values-cross-eval-boundary` — values, not module references, cross the eval boundary

The host holds resolved `config.*` records, and the module system cannot read an option's
definitions back, so a module-reference input is not available.

### (settled) `layered-no-clear-or-replace` — layered mode cannot clear or replace

A later record cannot undo an earlier one's scalar or drop an inherited list entry; a replacement
story is deferred.

### (settled) `profiles-external` — `profiles.*` is external

wrapkit plugs in via a function that runs its own mini-eval.

## Open threads

- Is `ifSet` / `ifPresent` / `unset` one merged per-VAR control or three fields?
- Do `-contents` entries become tagged list entries, a separate field, or disappear?
- What are the placeholder entry forms (`{ env = …; }`, `{ args = …; }`), and do they share one type?
- Does layered mode survive once ordering is a definition-level mechanism?
- Does the emitter generate shell for cross-VAR values, or does declarawrap's `expand-env` make it moot?
- Can one `val` list express both a replace and a sequence (set-then-prepend), or is that out of reach?
