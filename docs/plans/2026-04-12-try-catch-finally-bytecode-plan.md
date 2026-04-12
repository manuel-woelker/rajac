# What is the plan for implementing `finally` clauses on `try`/`catch` statements?

## Why is a dedicated plan needed now?

rajac already lowers `try`/`finally` and plain `try`/`catch`, but the mixed form remains explicitly unsupported in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs).
That makes `try` with both `catch` and `finally` the remaining exception-lowering gap in the current backend milestone sequence.

This is the right next step because the surrounding infrastructure already exists:
typed catch handlers, catch parameter locals, protected-region exception tables, and finally-exit thunks are all present.
What is missing is the control-flow composition that makes them work together without double-running or skipping the `finally` body.

## What is the current implementation baseline?

The current codebase already has:

- parser support for `try`, `catch`, and `finally` in [stmt.rs](/data/projects/rajac/crates/parser/src/stmt.rs)
- AST support for `Stmt::Try` and `CatchClause` in [ast.rs](/data/projects/rajac/crates/ast/src/ast.rs)
- resolution support that walks catch parameter types and finally blocks in [resolution.rs](/data/projects/rajac/crates/compiler/src/stages/resolution.rs)
- attribute-analysis and flow-analysis support for `try` / `catch` / `finally` semantics in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs) and [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs)
- bytecode lowering for plain `try` and `try`/`finally` in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs)
- bytecode lowering for plain `try`/`catch` in the same file
- verification fixtures for standalone `try`/`finally` and standalone `try`/`catch` under [verification/sources/verify/controlflow](/data/projects/rajac/verification/sources/verify/controlflow)

The remaining backend gap is explicit.
`emit_try_statement(...)` currently rejects non-empty `catches` when `finally_block.is_some()` with the unsupported-feature path for `try catch finally statements`.

## What should this milestone implement?

This milestone should implement bytecode generation for `try` statements that include both one or more `catch` clauses and a `finally` block.

That milestone should own:

- lowering `try { ... } catch (...) { ... } finally { ... }`
- lowering multiple catch clauses followed by a shared `finally`
- executing the `finally` body after normal completion of the `try` body
- executing the `finally` body after normal completion of a catch body
- executing the `finally` body on abrupt exits that leave either the `try` body or a catch body
- preserving typed catch dispatch and catch parameter local binding
- maintaining exception-table, stack, local-slot, and label correctness in the generated method body

## What should remain out of scope for this milestone?

To keep the work tight, this milestone should not expand into all remaining exception-related features.

The initial scope should leave these as explicit follow-up work:

- multi-catch syntax and union catch types
- try-with-resources desugaring
- `synchronized` lowering
- checked-exception analysis beyond what the frontend already does
- stack-map-frame work beyond what the current backend already requires

If some internal cleanup makes a later feature easier, that is fine, but the completion bar should stay focused on mixed `catch` plus `finally`.

## How should `try`/`catch`/`finally` lowering work?

The backend should treat `finally` as a post-handler control-flow obligation, not as a separate competing lowering mode.

The recommended lowering model is:

1. mark the protected bytecode range for the `try` body
2. emit the `try` body
3. if the `try` body can complete normally, emit the `finally` body before branching to a shared end label
4. emit one typed catch handler per catch clause for exceptions thrown from the protected `try` range
5. in each catch handler, store the thrown exception into the catch parameter local, emit the catch body, and if it can complete normally emit the `finally` body before branching to the shared end label
6. for abrupt exits from either the `try` body or a catch body (`return`, `throw`, `break`, `continue`), route control through the existing finally-exit thunk machinery
7. add a synthetic catch-all handler covering the protected ranges whose exceptions must still execute `finally` before rethrowing

The key idea is that catch dispatch happens first, then `finally` runs after the selected path leaves the `try` statement.
The implementation should not treat catch handlers as bypassing `finally`.

## Which protected regions should be covered by synthetic finally handlers?

This is the main place where the mixed form gets tricky.

The implementation should explicitly handle two protected regions:

- the `try` body range, so exceptions that are not handled by a typed catch still execute `finally`
- each catch body range whose exceptions should also execute `finally` before propagating

That second point matters because a catch body can itself throw or return.
If the synthetic finally coverage only wraps the original `try` body, exceptions thrown inside catch handlers would skip the `finally` block, which would be wrong.

## How should normal completion interact with `finally`?

Normal completion should remain explicit in the emitted code.

The implementation should ensure:

- a normally completing `try` body runs `finally` exactly once before leaving the statement
- a normally completing catch body runs `finally` exactly once before leaving the statement
- the shared end label is only bound when reachable from at least one normal path

Avoid clever shared helper paths that obscure which branch actually owns the `finally` execution.
This is one of those places where “DRY” can become “mysterious” fast.

## How should abrupt control flow interact with `finally`?

The mixed form must preserve the same abrupt-exit guarantees already implemented for plain `try`/`finally`.

The lowering should ensure that `finally` executes exactly once when any of these leave the `try` statement from either the `try` body or a catch body:

- `return`
- `throw`
- `break`
- `continue`

This likely means catch handlers need to participate in the same `finally_contexts` machinery used by the existing `try`/`finally` implementation rather than reinventing a parallel exit path.

## How should the implementation be structured?

The top-level `emit_try_statement(...)` helper in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs) should stay readable.

The recommended structure is:

- keep `emit_try_statement(...)` as the dispatch point
- extract a dedicated helper such as `emit_try_catch_finally_statement(...)`
- reuse the existing catch-type and catch-handler helpers where practical
- reuse the existing finally-exit thunk machinery rather than cloning it for catch bodies
- add a small helper for synthetic catch-all finally handlers that can wrap both the `try` body and catch bodies

If the current helper boundaries fight this design, refactor them before adding more branches.
A slightly larger but clear refactor is better than jamming one more special case into an already stateful function.

## What is the recommended implementation order?

1. Audit the current `try`/`finally` and `try`/`catch` helpers to identify which parts should be shared and which should stay specialized.
2. Add a dedicated helper for mixed `try`/`catch`/`finally` lowering.
3. Reuse typed catch-handler emission so catch parameter locals still flow through the normal local-variable path.
4. Extend the finally machinery so normal completion from catch bodies runs the `finally` block before the shared exit.
5. Extend synthetic finally handler coverage so exceptions thrown from both the `try` body and catch bodies execute `finally` before rethrowing.
6. Route abrupt exits from catch bodies through the existing finally-exit thunk machinery.
7. Add colocated bytecode-generation tests for single-catch and multi-catch mixed forms.
8. Add valid verification fixtures under `verification/sources` for supported `try`/`catch`/`finally` examples.
9. Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
10. Run `cargo run -p rajac-verification --bin verification`.
11. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should be colocated with bytecode generation and should target the parts most likely to regress.

The first colocated test set should include:

- `try`/`catch`/`finally` lowering no longer produces an unsupported-feature diagnostic
- a catch body that completes normally runs `finally` before the common exit
- a catch body that throws still executes `finally` before rethrow
- a catch body that returns routes through `finally`
- multiple catch clauses with a shared `finally` emit the expected handler table structure

Valid verification fixtures should stay small and explicit, for example:

- a method that throws in the `try` body, catches the exception, and then runs `finally`
- a method whose catch body returns while `finally` performs a side effect
- a method whose catch body throws again while `finally` still executes
- a method with two catch clauses and one shared `finally`

These fixtures should be simple enough that pretty-printed classfile diffs reveal whether handler coverage and finalizer routing match OpenJDK.

## What assumptions and risks should stay explicit?

This plan assumes:

- the existing `finally_contexts` design can be extended to catch bodies without redesigning unrelated control-flow code
- the current exception-table model can represent both typed catch handlers and synthetic catch-all finally handlers cleanly
- the frontend behavior for mixed `try` / `catch` / `finally` is already stable enough that the remaining work is backend-only

The main risks are:

- double-running the `finally` block on catch paths
- accidentally skipping `finally` for exceptions thrown inside catch bodies
- generating overlapping protected ranges that serialize but do not match `javac` behavior
- making `emit_try_statement(...)` so branchy that later debugging becomes miserable

If those risks show up, the implementation should keep the scope explicit and record any structural refactor in the plan rather than broadening the milestone informally.

## What completion criteria should define success?

This milestone should be considered complete when:

- `Stmt::Try { .. }` no longer falls back to the unsupported-feature helper for supported `try`/`catch`/`finally` forms
- `finally` executes correctly after normal completion of both `try` bodies and catch bodies
- abrupt exits from both `try` bodies and catch bodies execute `finally` exactly once before continuing the original control transfer
- the backend emits the required typed and synthetic exception-table entries for the supported mixed forms
- colocated tests cover the core mixed-path lowering behavior
- verification fixtures cover the supported `try`/`catch`/`finally` examples
- `cargo run -p rajac-verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [ ] Audit the current `try`/`finally` and `try`/`catch` helpers for reusable mixed-form infrastructure.
- [ ] Add a dedicated helper for `try`/`catch`/`finally` lowering.
- [ ] Reuse typed catch-handler lowering while preserving catch parameter local binding.
- [ ] Implement `finally` execution on normal completion from catch bodies.
- [ ] Extend synthetic finally handler coverage so exceptions from catch bodies still execute `finally`.
- [ ] Route abrupt exits from catch bodies through the existing finally-exit thunk machinery.
- [ ] Add colocated bytecode-generation tests for supported `try`/`catch`/`finally` paths.
- [ ] Add valid verification fixtures for supported mixed-form examples.
- [ ] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [ ] Run `cargo run -p rajac-verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
