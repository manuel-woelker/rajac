# What is the plan for implementing multi-catch support?

## Why is multi-catch the right next milestone?

rajac now supports plain `try`/`catch`, plain `try`/`finally`, and mixed `try`/`catch`/`finally`.
The next remaining exception-handling gap that builds directly on that work is Java's multi-catch syntax, where a single catch clause matches multiple exception types.

This is a good next step because the backend already knows how to:

- emit typed catch handlers
- bind catch parameters through the normal local-variable path
- compose catch handlers with `finally`

What is missing is the frontend representation and the lowering rule that turns one source catch clause into multiple typed exception-table entries that share one handler body.

## What is the current implementation baseline?

The current codebase already has:

- parser support for `try`, `catch`, and `finally` in [stmt.rs](/data/projects/rajac/crates/parser/src/stmt.rs)
- AST support for `Stmt::Try` and `CatchClause` in [ast.rs](/data/projects/rajac/crates/ast/src/ast.rs)
- resolution support for catch parameter types and catch bodies in [resolution.rs](/data/projects/rajac/crates/compiler/src/stages/resolution.rs)
- attribute-analysis and flow-analysis support for existing `try` / `catch` / `finally` forms in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs) and [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs)
- bytecode lowering for plain `try`/`catch` and mixed `try`/`catch`/`finally` in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs)
- verification fixtures for current catch and finally forms under [verification/sources/verify/controlflow](/data/projects/rajac/verification/sources/verify/controlflow)

The current frontend gap is explicit.
`parse_try_stmt()` in [stmt.rs](/data/projects/rajac/crates/parser/src/stmt.rs) reads exactly one catch type before the parameter name, so syntax like `catch (IOException | SQLException err)` is not representable yet.

## What should this milestone implement?

This milestone should implement Java multi-catch syntax for catch clauses that name multiple alternative exception types and share a single catch body.

That milestone should own:

- parsing multi-catch parameter type lists
- representing multi-catch in the AST
- resolving each listed catch type
- validating the basic semantic shape of multi-catch clauses conservatively
- lowering one multi-catch clause into multiple typed exception-table entries that target one shared handler block
- preserving the existing catch parameter local binding model
- supporting multi-catch in both plain `try`/`catch` and mixed `try`/`catch`/`finally` forms

## What should remain out of scope for this milestone?

To keep the implementation reviewable, this milestone should not attempt to finish every advanced semantic rule around Java exceptions.

The initial scope should leave these as explicit follow-up work if they prove non-trivial:

- full Java reachability and subtype-redundancy checks for catch alternatives
- precise enforcement of the Java requirement that multi-catch alternatives be disjoint
- advanced checked-exception analysis beyond the current frontend baseline
- try-with-resources desugaring
- `synchronized` lowering

If some conservative semantic checks are easy to add, that is fine, but the completion bar should stay focused on end-to-end parsing, lowering, and verification.

## How should multi-catch be represented in the AST?

The current `CatchClause` stores one `ParamId`, and `Param` stores one `AstTypeId`.
That shape does not express multi-catch cleanly.

The recommended model is to represent the catch parameter as one named parameter plus a list of one or more declared exception types.
For example, the catch clause should be able to express:

- one parameter name
- one or more alternative exception type ids
- one body

This can be modeled either by extending `CatchClause` directly with a `types: Vec<AstTypeId>` field or by introducing a small dedicated catch-parameter struct.
Prefer the simpler option unless another part of the frontend clearly benefits from a new reusable abstraction.

## How should parsing work?

The parser should continue to parse ordinary single-type catch clauses, but it should also accept `|`-separated type alternatives before the parameter name.

The recommended parsing model is:

1. parse the first catch type
2. while the next token is `|`
3. consume the separator and parse another catch type
4. parse the shared parameter name once after the type list
5. parse the catch body as today

This means the parser likely needs a small helper dedicated to parsing catch type alternatives rather than trying to shoehorn the syntax into the existing general type parser.

## How should resolution work?

Resolution should resolve every listed catch type independently.
The catch parameter name should still bind once for the whole catch body.

The implementation should ensure:

- each alternative type receives a resolved `TypeId`
- unresolved alternatives are reported consistently with existing catch-type resolution failures
- the catch body sees the same parameter local regardless of which exception alternative matched

If a multi-catch clause contains a type the current compiler cannot resolve, the implementation should fail clearly rather than partially lowering the clause.

## What semantic validation should be added?

The minimum semantic bar is that each alternative be a reference type that is usable as a catch type under the current compiler model.

The first implementation should at least reject obviously invalid shapes such as:

- primitive types in a catch alternative list
- empty alternative lists
- duplicate alternatives within the same clause when they are trivially identical after resolution

If full Java subtype-disjointness checking is too much for this milestone, say so explicitly in the plan and keep the first implementation conservative.

## How should bytecode lowering work?

The backend should lower one multi-catch source clause into multiple typed exception-table entries that all target the same handler label.

The recommended lowering model is:

1. compute the shared handler label for the catch clause
2. emit one exception-table entry per resolved alternative catch type, all pointing at that handler
3. emit the shared handler body once
4. store the thrown exception into the existing catch-parameter local slot
5. continue with the existing catch-body lowering path

This matches the JVM model directly and should fit naturally into the typed catch-entry helpers already present in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs).

## How should multi-catch interact with `finally`?

Multi-catch should reuse the mixed `try`/`catch`/`finally` lowering path rather than inventing a separate branch.

That means:

- one multi-catch clause may contribute multiple typed entries to the same handler
- the shared handler body should still participate in the current `finally` machinery
- the same synthetic catch-all finally handling should apply after the selected multi-catch handler begins executing

If the current mixed-form helper assumes one catch type per clause, refactor that assumption directly instead of layering special cases on top.

## How should the implementation be structured?

The recommended structure is:

- add a parser helper for catch type alternatives
- extend the AST catch-clause shape to carry multiple declared types
- update resolution and semantic analysis to walk all alternative types
- refactor the catch-type helper in the bytecode generator so one catch clause can yield multiple typed entries
- keep catch-parameter local binding exactly once per catch clause, not once per alternative type

Avoid copying catch-body lowering per alternative type.
The whole point of multi-catch is that multiple exception types share one handler body, and the implementation should preserve that directly.

## What is the recommended implementation order?

1. Audit the current parser, AST, resolution, and bytecode catch representations to identify the smallest coherent shape change.
2. Extend the AST catch-clause representation to carry one or more catch types.
3. Update the parser to accept `|`-separated catch type alternatives.
4. Update resolution to resolve every alternative catch type.
5. Add or extend semantic checks for obviously invalid multi-catch alternatives.
6. Refactor bytecode catch-entry generation so one catch clause can emit multiple typed exception-table entries pointing to one handler.
7. Ensure mixed `try`/`catch`/`finally` lowering reuses the same multi-catch handler-entry generation.
8. Add colocated parser and bytecode-generation tests for multi-catch.
9. Add valid and invalid verification fixtures for multi-catch.
10. Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
11. Run `cargo run -p rajac-verification --bin verification`.
12. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should cover both the new syntax and the shared-handler lowering behavior.

The first colocated test set should include:

- parser coverage for a single catch clause with multiple alternatives
- resolution or semantic coverage for at least one invalid alternative shape
- bytecode-generation coverage showing that one multi-catch clause emits multiple typed entries with one shared handler body
- mixed-form coverage showing multi-catch still works with `finally`

Verification fixtures should stay focused, for example:

- a plain multi-catch method such as `catch (IllegalArgumentException | RuntimeException err)`
- a multi-catch plus `finally` method
- an invalid-source fixture for a clearly unsupported alternative shape if rajac now rejects it explicitly

These fixtures should stay small so classfile and diagnostic comparisons remain easy to interpret.

## What assumptions and risks should stay explicit?

This plan assumes:

- the existing catch-parameter local binding model can remain one-local-per-clause even when a clause has multiple alternative types
- the current exception-table generation model can emit multiple typed entries for one shared handler without larger classfile changes
- the parser can recognize `|` in catch clauses without colliding with existing expression parsing behavior

The main risks are:

- overcomplicating the AST shape for what should be a small syntax extension
- accidentally emitting one handler body per alternative type instead of one shared handler
- adding semantic checks that look correct but diverge from Java rules in edge cases
- breaking mixed `try`/`catch`/`finally` lowering by assuming each catch clause has exactly one type

If those risks materialize, keep the first implementation conservative and explicit instead of pretending the full Java rule set is already done.

## What completion criteria should define success?

This milestone should be considered complete when:

- the parser accepts `|`-separated catch type alternatives
- the AST and resolution stages preserve all alternative catch types
- one multi-catch clause lowers to multiple typed exception-table entries that share one handler body
- multi-catch works in both plain `try`/`catch` and mixed `try`/`catch`/`finally` forms
- colocated tests cover parsing and lowering of multi-catch
- verification fixtures cover the supported multi-catch examples
- `cargo run -p rajac-verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [ ] Audit the current catch-clause representation across parser, AST, resolution, and bytecode generation.
- [ ] Extend the AST catch-clause shape to carry multiple catch types.
- [ ] Update the parser to accept `|`-separated catch type alternatives.
- [ ] Update resolution to resolve every alternative catch type.
- [ ] Add conservative semantic checks for obviously invalid multi-catch alternatives.
- [ ] Refactor bytecode catch-entry generation so one catch clause can emit multiple typed entries that share one handler.
- [ ] Ensure mixed `try`/`catch`/`finally` lowering reuses the same multi-catch handler-entry generation.
- [ ] Add colocated tests for multi-catch parsing and bytecode lowering.
- [ ] Add verification fixtures for supported multi-catch examples.
- [ ] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [ ] Run `cargo run -p rajac-verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
