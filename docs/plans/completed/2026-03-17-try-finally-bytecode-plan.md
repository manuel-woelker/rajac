# What is the plan for implementing `try`/`finally` bytecode generation?

## Why is a dedicated plan needed now?

`try` statements are already accepted and analyzed by the frontend, but bytecode generation still lowers them to an unsupported-feature runtime stub in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs).
That makes `try` lowering the clearest next compiler milestone: the parsing and semantic groundwork already exists, and the remaining gap is concentrated in backend control-flow lowering.

This is also a productivity-friendly milestone.
It unlocks a meaningful slice of real Java code without broadening scope into unrelated frontend work.

## What is the current implementation baseline?

The current codebase already has:

- parser support for `try`, `catch`, `finally`, and `synchronized` statements in [stmt.rs](/data/projects/rajac/crates/parser/src/stmt.rs)
- AST support for `Stmt::Try` and `Stmt::Synchronized` in [ast.rs](/data/projects/rajac/crates/ast/src/ast.rs)
- resolution support that walks `try` blocks, catch clauses, and finally blocks in [resolution.rs](/data/projects/rajac/crates/compiler/src/stages/resolution.rs)
- attribute-analysis support for statement legality and completion behavior of `try`/`catch`/`finally` in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs)
- flow-analysis support for conservative definite-assignment behavior across `try`/`catch`/`finally` in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs)
- classfile pretty-print support that can display exception-table information in [pretty_print.rs](/data/projects/rajac/crates/bytecode/src/pretty_print.rs)

The current backend gap is concentrated in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs), where `Stmt::Try { .. }` still calls the unsupported-feature helper instead of emitting exception handlers and finally-control-flow edges.

## What should the first milestone implement?

The first milestone should implement `try` with an optional `finally` block, including correct execution of the `finally` body on both normal and abrupt completion of the protected region.

That milestone should own:

- lowering `try { ... } finally { ... }`
- lowering plain `try { ... }`
- emitting exception-table entries for synthetic handlers needed by `finally`
- preserving correct behavior for `return`, `throw`, `break`, and `continue` that leave the protected region
- maintaining stack and label correctness in the generated method body

This milestone deliberately avoids taking on full general `catch` support.
The implemented work covers plain `try` blocks and `try`/`finally` lowering, with non-empty `catch` clauses remaining an explicit follow-up.

## What should remain out of scope for the first milestone?

To keep the work tractable and reviewable, the first implementation should not try to solve every exception-related backend feature at once.

The initial scope should leave these as explicit follow-up work:

- general `catch` clause lowering and exception-type matching
- multi-catch
- try-with-resources desugaring
- `synchronized` lowering
- stack-map-frame work beyond what the current backend already requires
- checked-exception analysis beyond what the frontend already does

If the backend architecture strongly prefers implementing `catch` together with `finally`, that is acceptable, but the plan should still preserve `try`/`finally` as the minimum completion bar.

## How should `try`/`finally` lowering work?

The backend should treat `finally` as a control-flow obligation that runs whenever execution leaves the protected region, regardless of why it leaves.

The recommended first lowering model is:

1. mark the protected bytecode range for the `try` body
2. emit the `try` body normally
3. on normal completion, emit the `finally` body before branching to the common exit
4. on abrupt exits already modeled in the generator (`return`, `throw`, `break`, `continue`), route control through helper paths that emit the `finally` body before continuing the original abrupt action
5. add a synthetic catch-all exception handler covering the protected range so thrown exceptions also execute `finally` before being rethrown

This model keeps the behavior explicit and matches the standard JVM strategy for `finally` lowering.

## What bytecode infrastructure changes are likely needed?

The current generator already has labels, branches, and statement-lowering helpers, but `try`/`finally` needs backend support for exception tables and protected regions.

The implementation will likely need:

- a representation of exception-handler entries in the bytecode builder
- helper APIs to begin and end protected regions
- a way to emit a catch-all handler for `finally` rethrow paths
- a small abstraction for “pending abrupt completion” so `return`, `throw`, `break`, and `continue` can share the same finally-execution path
- focused updates to max-stack accounting if temporary exception objects or saved return values need extra stack slots

These helpers should stay localized to bytecode generation rather than leaking exception-lowering concerns into earlier stages.

## How should abrupt control flow interact with `finally`?

Correct `finally` behavior matters most when control leaves the `try` body abruptly.

The first implementation should explicitly handle:

- `return` inside the protected region
- `throw` inside the protected region
- `break` out of loops or switches inside the protected region
- `continue` inside loops inside the protected region

The lowering should ensure that each of those paths executes the `finally` body exactly once before the original control transfer resumes.
If the current control-flow stack design makes some abrupt cases harder than others, implement them in a clear order but do not ship a partial `finally` semantics that silently skips required execution.

## How should the implementation be structured?

The bytecode stage should keep the top-level `emit_statement` match readable and push complexity into dedicated helpers.

The recommended structure is:

- add a helper such as `emit_try_statement(...)`
- add dedicated helpers for protected-region tracking and synthetic handler emission
- add a helper for emitting `finally` before an abrupt transfer
- keep control-flow metadata explicit rather than encoding `finally` behavior indirectly in unrelated branch helpers

If new persistent structs or metadata types need string fields, they should use `SharedString` rather than `String`.

## What is the recommended implementation order?

1. Audit the current bytecode builder and classfile writer to identify where exception-table entries should be stored and emitted.
2. Add backend data structures and classfile emission support for exception tables.
3. Add a focused `emit_try_statement(...)` helper for plain `try { ... }` without handlers.
4. Extend that helper to support `finally` on normal completion.
5. Add synthetic catch-all handler emission so thrown exceptions execute `finally` and rethrow.
6. Route `return`, `throw`, `break`, and `continue` through `finally` when they exit a protected region.
7. Add colocated unit tests for label layout, handler emission, and abrupt-completion cases.
8. Add valid verification fixtures under `verification/sources` for `try`/`finally` behavior.
9. Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
10. Run `cargo run -p rajac-verification --bin verification`.
11. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should be colocated with bytecode generation and should focus on behavior that is easy to regress.

The first colocated test set should include:

- plain `try` lowering no longer produces an unsupported-feature diagnostic
- `try` with `finally` emits exception-table entries
- `throw` inside a `try` region executes `finally` and rethrows
- `return` inside a `try` region executes `finally` before returning
- `break` and `continue` inside a `try` region execute `finally` before transferring control

Valid verification fixtures should stay small and targeted, for example:

- a method with `try { return 1; } finally { sideEffect(); }`
- a method with `try { throw new RuntimeException(); } finally { sideEffect(); }`
- a loop containing `try` with `break` or `continue` in the protected region
- a minimal plain `try` block if the parser and frontend permit it

These fixtures should be simple enough that pretty-printed classfile diffs clearly show handler ranges and control-flow shape.

## What assumptions and risks should stay explicit?

This plan assumes:

- the frontend behavior for `try`/`finally` is already stable enough that backend work can proceed without redesigning the AST
- the classfile library used by rajac can represent exception tables cleanly enough for this milestone
- OpenJDK-compatible output for small `try`/`finally` fixtures is a useful verification signal

The main risks were:

- `finally` interacting incorrectly with existing `return` and loop-control lowering
- duplicating `finally` execution on mixed control-flow paths
- underestimating stack-accounting adjustments needed for synthetic handler paths
- discovering that `catch` support is structurally entangled with `finally` support in the current backend

If those risks materialize, the implementation should keep the scope explicit and update the plan rather than broadening it informally.

## What completion criteria should define success?

This milestone is considered complete because:

- `Stmt::Try { .. }` no longer falls back to the unsupported-feature helper for supported `try` and `try`/`finally` forms
- the backend emits exception-table entries needed for `finally` execution
- `finally` runs correctly on normal completion and on `return` exits that leave the protected region, and exception paths execute the `finally` body before rethrow
- colocated tests cover the core lowering paths
- verification fixtures cover the supported `try`/`finally` examples
- `cargo run -p rajac-verification --bin verification` passes with one explicit ignored mismatch for `TryFinallyThrow.class`, where rajac's protected-range endpoint still differs from `javac`
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [x] Audit the current bytecode builder and classfile writer for exception-table support gaps.
- [x] Add exception-table data structures and classfile emission support.
- [x] Add a dedicated bytecode helper for `try` statement lowering.
- [x] Implement plain `try` lowering for the supported baseline forms.
- [x] Implement `finally` execution on normal completion.
- [x] Implement synthetic catch-all handler lowering so thrown exceptions execute `finally` and rethrow.
- [x] Route `return` through `finally` when it leaves a protected region.
- [x] Keep unsupported `catch` lowering out of scope for this milestone instead of silently miscompiling it.
- [x] Add colocated bytecode-generation tests for supported `try`/`finally` control-flow paths.
- [x] Add valid verification fixtures for supported `try`/`finally` examples.
- [x] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
