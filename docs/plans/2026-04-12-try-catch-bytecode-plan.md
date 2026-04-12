# What is the plan for implementing `try`/`catch` bytecode generation?

## Why is this the right next milestone?

`try`/`finally` lowering is already implemented, and the frontend already understands `catch` clauses.
That makes backend `try`/`catch` lowering the clearest next step: the remaining gap is concentrated in bytecode generation rather than spread across parsing, resolution, and semantic analysis.

This is also a good scope boundary.
It unlocks an important part of everyday Java control flow without taking on unrelated backend work such as `synchronized` lowering or broader frontend type-system improvements.

## What is the current implementation baseline?

The current codebase already has:

- parser support for `try`, `catch`, `finally`, and `synchronized` statements in [stmt.rs](/data/projects/rajac/crates/parser/src/stmt.rs)
- AST support for `Stmt::Try` and `CatchClause` in [ast.rs](/data/projects/rajac/crates/ast/src/ast.rs)
- resolution support that walks catch parameter types and catch bodies in [resolution.rs](/data/projects/rajac/crates/compiler/src/stages/resolution.rs)
- attribute-analysis support for `try`/`catch` statement legality and completion behavior in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs)
- flow-analysis support for definite-assignment behavior across `try`, `catch`, and `finally` in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs)
- exception-table support and `try`/`finally` lowering infrastructure in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs) and the classfile generation modules under [crates/bytecode/src/classfile](/data/projects/rajac/crates/bytecode/src/classfile)

The remaining backend gap is explicit.
In [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs), `emit_try_statement(...)` still rejects any non-empty `catch` list with the unsupported-feature path for `try catch statements`.

## What should this milestone implement?

This milestone should implement bytecode generation for `try` with one or more `catch` clauses and no `finally` block.

That milestone should own:

- lowering `try { ... } catch (...) { ... }`
- lowering multiple sequential catch clauses
- emitting typed exception-table entries for each catch clause
- binding the caught exception value to the catch parameter local
- preserving correct normal-completion control flow from the `try` body and each catch body
- maintaining stack, local-slot, and label correctness in the generated method body

## What should remain out of scope for this milestone?

To keep the work reviewable and reduce control-flow risk, the first `catch` milestone should not also attempt to solve every combination at once.

The initial scope should leave these as explicit follow-up work:

- `try` with both `catch` and `finally`
- multi-catch syntax and union catch types
- try-with-resources desugaring
- `synchronized` lowering
- checked-exception analysis beyond what the frontend already does
- stack-map-frame work beyond what the current backend already requires

If some internal refactoring makes later `try`/`catch`/`finally` work easier, that is fine, but the completion bar for this plan should remain plain `try`/`catch`.

## How should `try`/`catch` lowering work?

The backend should lower `try`/`catch` as a protected region followed by one handler block per catch clause.

The recommended lowering model is:

1. create labels for the protected `try` range, each catch handler, and a shared end label
2. emit the `try` body inside the protected range
3. if the `try` body can complete normally, branch to the shared end label after it finishes
4. for each catch clause, add an exception-table entry covering the protected range with that clause's resolved catch type
5. bind the handler label, store the thrown exception into the catch parameter's local slot, and emit the catch body
6. if a catch body can complete normally, branch to the shared end label after it finishes
7. bind the shared end label when any preceding path can reach it

This structure matches the JVM model cleanly and reuses the exception-table infrastructure already added for `finally`.

## What backend infrastructure changes are likely needed?

The current bytecode generator already knows how to emit protected regions and exception handlers, but `catch` support needs handler emission that is typed and parameter-aware.

The implementation will likely need:

- a helper to resolve the catch parameter type into the correct exception-table catch type index
- a helper to allocate or look up the catch parameter local slot before emitting the catch body
- a small extension to `emit_try_statement(...)` so it can distinguish plain `try`, `try`/`catch`, and `try`/`finally` without becoming unreadable
- careful stack-state resets at handler entry points, since JVM handlers begin with the thrown exception value on the operand stack
- tests that cover multiple catch clauses and shared-end-label behavior

These additions should stay localized to bytecode generation.
They should not push backend lowering concerns back into semantic phases that already model the source semantics correctly.

## How should catch parameter locals be handled?

Each catch handler begins with the thrown exception object on the operand stack.
The handler should immediately store that value into the local allocated for the catch parameter so the body can read it through the existing local-variable paths.

The implementation should:

- use the resolved parameter type already attached to the catch parameter
- allocate a reference local slot for the caught exception
- register the local in whatever local-binding structure the generator already uses for parameters and locals
- ensure the local is scoped to the catch body rather than leaking across sibling handlers

If the current local-binding model does not make catch-parameter scoping explicit, fix that in the generator rather than open-coding name lookups per handler.

## How should multiple catches behave?

The first implementation should support multiple catch clauses as long as each clause names a single resolved catch type.

Handlers should be emitted in source order, with one exception-table entry per clause.
That preserves Java's first-match semantics because the JVM also evaluates handlers in table order.

This milestone should not silently accept unsupported multi-catch forms.
If the parser or resolver can already represent those forms, they should stay explicitly unsupported until lowered correctly.

## How should the implementation be structured?

The top-level `emit_statement` match in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs) should stay readable.
Most of the new work should live in focused helpers.

The recommended structure is:

- keep `emit_try_statement(...)` as the entry point for `Stmt::Try`
- split plain `try`, `try`/`catch`, and `try`/`finally` lowering into separate helpers where that improves readability
- add a helper for emitting a single catch handler block
- add a helper for computing the catch type entry used in the exception table

Avoid forcing the existing `finally` lowering path and the new `catch` lowering path into one overgeneralized abstraction too early.
That is the kind of cleanup that looks elegant for ten minutes and then becomes a pain to debug.

## What is the recommended implementation order?

1. Audit the current `emit_try_statement(...)` path and identify which existing helpers from `try`/`finally` lowering can be reused directly.
2. Add a dedicated helper to compute the exception-table catch type for a catch clause from its resolved parameter type.
3. Add a dedicated helper to emit a single catch handler block, including storing the exception into the catch parameter local.
4. Implement plain `try`/`catch` lowering for a single catch clause with no `finally`.
5. Extend the lowering to support multiple catch clauses in source order.
6. Keep mixed `catch` plus `finally` forms explicitly unsupported instead of partially compiling them.
7. Add colocated bytecode-generation tests for single-catch and multi-catch control flow.
8. Add valid verification fixtures under `verification/sources` for supported `try`/`catch` examples.
9. Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
10. Run `cargo run -p rajac-verification --bin verification`.
11. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should be colocated with the bytecode-generation code and should focus on failure-prone handler details.

The first colocated test set should include:

- plain `try`/`catch` lowering no longer produces an unsupported-feature diagnostic
- a catch handler stores the thrown exception into a local before the body runs
- multiple catch clauses produce multiple typed exception-table entries in source order
- normal completion from the `try` body branches past the handlers
- normal completion from a catch body branches to the shared end label

Valid verification fixtures should stay small and targeted, for example:

- a method that catches `RuntimeException` and returns a fallback value
- a method with two catch clauses for distinct exception types
- a method where the catch body reads the caught exception variable

These fixtures should be simple enough that pretty-printed classfile diffs make handler ranges and handler targets obvious.

## What assumptions and risks should stay explicit?

This plan assumes:

- catch parameter types are already resolved to usable `TypeId` values before bytecode generation
- the existing exception-table machinery is sufficient for typed catch entries, not only catch-all handlers
- existing local-slot allocation can be extended to cover catch parameters cleanly

The main risks are:

- handler stack-state bugs caused by forgetting that the thrown exception starts on the operand stack
- emitting incorrect protected ranges when the `try` body cannot complete normally
- leaking catch-parameter locals across handlers or into the shared continuation path
- discovering that mixed `catch` and `finally` forms are more entangled with the current implementation than expected

If those risks materialize, the implementation should keep the scope explicit and avoid broadening into full `try`/`catch`/`finally` support without a separate plan update.

## What completion criteria should define success?

This milestone should be considered complete when:

- `Stmt::Try { .. }` no longer falls back to the unsupported-feature helper for supported `try`/`catch` forms without `finally`
- the backend emits typed exception-table entries for each supported catch clause
- catch parameters are readable inside the catch body through normal local-variable lowering
- colocated tests cover the core single-catch and multi-catch lowering paths
- verification fixtures cover the supported `try`/`catch` examples
- `cargo run -p rajac-verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [ ] Audit the current `try` lowering path for reusable handler infrastructure.
- [ ] Add a helper for computing typed catch entries from resolved catch parameter types.
- [ ] Add a helper for emitting catch handler blocks and storing caught exceptions into locals.
- [ ] Implement plain `try`/`catch` lowering for a single catch clause.
- [ ] Extend the lowering to support multiple sequential catch clauses.
- [ ] Keep mixed `catch` plus `finally` forms explicitly unsupported in this milestone.
- [ ] Add colocated bytecode-generation tests for `try`/`catch` lowering.
- [ ] Add valid verification fixtures for supported `try`/`catch` examples.
- [ ] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [ ] Run `cargo run -p rajac-verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
