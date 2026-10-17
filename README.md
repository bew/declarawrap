# Declarawrap - Declarative binary wrapper


In Nix, we can build wrappers of another binary using these 2 helpers:

- `makeWrapper` will take actions via cli & generate a _shell script_ that will apply all changes to
  the env/flags/args (through particulary ugly & unreadable shell tricks to avoid escaping issues..)
  before calling the original executable.

  Why I don't like this solution:
  - Medium-hard to generate the wrapper (to make sure there's no escaping issues..)
  - Can be introspected after the build (but generated shell script isn't very readable..)
  - Not great to execute with the overhead of the shell and its (ugly) env/flags/args
    initialization/sanitization.

- `makeBinaryWrapper` will take actions via cli & generate a C program to do the same thing as
  `makeWrapper`.

  Why I don't like this solution:
  - Hard to generate the wrapper (very error-prone)
  - Cannot be introspected after the build (other than using a non-trivial `strings` invocation to
    extract the initial cli actions given to generate the wrapper)

These solutions do not fullfill my need to fast & easily-introspectable wrapper needs.

What I want:
- Fast wrapper with very little overhead
- Readable configuration for the wrapper, both for the human & the machine
- No issue with string escapes
- Flexible env/flags/args wrapping logic

---

## DESIGN

The wrapper is a script with my wrapper system as shebang, the rest of the file is the configuration
of the wrapper in KDL, a readable configuration language.

On Nix side we can have a nice _lib_ to generate this config.

See [Full Sample Config][sample-full-config].

[sample-full-config]: ./sample-full-config.kdl


### Why not HCL for the language?

- Does not support `foo "bar"` directive.
  HCL _requires_ a block like `foo "bar" { … }`, which makes the config way too verbose.
- Does not support variadic params for a directive like `foo "bar" "baz" "other" { … }`.
  HCL _requires_ a block & an explicit list like `foo { items = ["bar", "baz", "other"] }`, which is
  annoying when the format could be _simpler_.

<!-- vim:set sw=2: -->
