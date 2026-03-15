# What is the plan for adding support for throwing exceptions?

## Why is a dedicated exception-throwing plan needed?

The parser, resolution stage, and attribute analysis already understand exception-related syntax well enough to parse `throw`, `throws`, and `try`/`catch`/`finally`.
However, the compiler still does not actually emit bytecode for `throw` statements.

Today, [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs) validates that a `throw` operand is reference-typed and `Throwable`-compatible.
But [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs) still treats `Stmt::Throw(_)` as a no-op during statement emission.
That means rajac can accept a valid `throw` statement and then generate incorrect method bodies that fall through instead of executing `athrow`.

There is also a second gap around declared exceptions.
Method and constructor `throws` clauses are resolved into method signatures, but the generated class files still leave `throws` metadata empty, and there is no checked-exception analysis yet.

## What is the current implementation baseline?

The current compiler already has:

- lexer and parser support for `throw`, `throws`, `try`, `catch`, and `finally`
- AST nodes for `Stmt::Throw`, catch clauses, and declared thrown types
- resolution support for method and constructor `throws` types
- semantic validation that `throw` operands are `Throwable`-compatible
- bytecode instruction support for `athrow`

The current compiler does not yet have:

- bytecode emission for `Stmt::Throw`
- verification fixtures that prove `throw` lowers correctly
- classfile `Exceptions` attribute emission from declared `throws` clauses
- checked-exception flow analysis for whether thrown checked exceptions are declared or caught
- try/catch/finally lowering and exception table generation

## What should this plan include, and what should it leave out?

This plan should cover the minimum end-to-end work needed for rajac to correctly compile methods that explicitly `throw` an exception.
That means:

- semantic validation stays in place and is tightened only where needed
- bytecode emission loads the thrown value and emits `athrow`
- reachability behavior after `throw` remains correct
- verification covers both valid bytecode generation and invalid diagnostics around bad `throw` operands

This plan should not try to solve all exception handling in one milestone.
In particular, it should leave full checked-exception analysis and try/catch/finally lowering as explicit follow-up work unless the implementation proves they are trivial and already structurally supported.

## What should the first milestone own?

The first milestone should make standalone `throw` statements work correctly in method bodies.
That milestone should own:

- statement lowering for `Stmt::Throw`
- operand evaluation followed by JVM `athrow`
- stack-shape correctness for thrown object references
- verification that methods terminate by throwing where expected
- compatibility for constructors and ordinary methods that `throw`

The milestone should not require catch handlers, exception tables, or finally rewriting.

## How should `throw` bytecode be emitted?

`throw` lowering should be implemented as a dedicated statement-emission case in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs).

The lowering rules should be:

1. Emit bytecode for the thrown expression so the evaluated reference is on top of the operand stack.
2. Emit `athrow`.
3. Treat the statement as abrupt completion for control-flow assembly purposes.

The implementation should rely on the existing expression emitter rather than inventing a parallel path.
If the thrown expression can currently evaluate to a primitive because of an earlier bug, semantic analysis should keep rejecting it so bytecode emission can assume a reference-shaped value.

## What semantic checks should be part of this work?

The current `throw` semantic checks are already enough for the first emission milestone in one important area: they reject primitive operands and non-`Throwable` reference types.
That validation should remain the source of truth for the first implementation.

The first exception-throwing milestone should also confirm that:

- `throw null;` remains accepted and lowers to normal JVM behavior
- abrupt completion after `throw` continues to participate in unreachable-statement diagnostics
- constructor bodies may `throw` just like ordinary methods

This plan should defer full checked-exception analysis such as:

- requiring checked exceptions to be caught or declared
- validating catch reachability against thrown exception types
- modeling exception flow through method invocation and constructor invocation

## Should declared `throws` clauses be included?

Declared `throws` clauses should be addressed in this plan only if they can be emitted with modest, isolated work.
The parser and resolution pipeline already preserve that information, and classfile generation already has places where method metadata is assembled.

That makes a reasonable second step:

- emit the classfile `Exceptions` attribute for methods and constructors whose signatures declare thrown types

However, this should be treated as metadata emission, not as full exception checking.
The compiler can emit correct `throws` metadata before it knows how to enforce checked-exception legality.

## What architecture changes should accompany the work?

The implementation should keep exception support layered by responsibility:

- attribute analysis owns operand legality for `throw`
- bytecode emission owns `athrow` lowering
- classfile generation owns `Exceptions` attribute emission
- future flow analysis should own checked-exception propagation and catch analysis

If bytecode generation needs any new helper for abrupt-exit statements, it should be shared with existing `return`, `break`, and `continue` lowering patterns instead of becoming a one-off exception path.

## What is the recommended implementation order?

1. Confirm the current semantic validation coverage for `Stmt::Throw` with colocated tests if anything is missing.
2. Implement `Stmt::Throw` bytecode emission using expression emission plus `athrow`.
3. Add valid verification fixtures that exercise thrown exceptions in methods and constructors.
4. Run verification and inspect the pretty-printed class files for `athrow`.
5. Emit `Exceptions` attributes from declared `throws` clauses if the method-signature path is already stable enough.
6. Add verification fixtures that confirm declared `throws` metadata when emitted.
7. Run repository-wide checks.

## What tests and verification fixtures should be added?

Tests should be colocated where the behavior is implemented.

The first colocated test set should include:

- semantic acceptance of `throw new RuntimeException();`
- semantic rejection of `throw 1;`
- semantic rejection of `throw` on a non-`Throwable` reference type
- reachability after `throw`

The first valid verification fixtures should include:

- a method that always throws a newly created runtime exception
- a constructor that throws after evaluating a simple guard path
- a method that conditionally throws and otherwise returns normally
- `throw null;` if rajac intends to match JVM behavior there now

If declared `throws` metadata is emitted, add fixtures that verify:

- a method with one declared thrown type
- a constructor with one declared thrown type

Invalid verification fixtures are only needed if semantic diagnostics change beyond what is already covered.
If rajac starts reporting better throw-related diagnostics than OpenJDK, add overrides in [verification_main.rs](/data/projects/rajac/crates/verification/src/verification_main.rs) while preserving OpenJDK line anchors.

## What assumptions and follow-up boundaries should stay explicit?

This plan assumes:

- `Stmt::Throw` remains a statement in the current AST rather than becoming part of a desugared exception IR first
- checked-exception analysis is still out of scope for the current compiler milestone
- try/catch/finally bytecode lowering remains separate work because it requires exception tables and handler ranges
- stack map frame generation is not newly required beyond what the current bytecode backend already does for simple methods

If any of those assumptions turns out to be false during implementation, the plan should be updated instead of silently broadening scope.

## What completion criteria should define success?

This exception-throwing milestone should be considered complete when:

- rajac emits correct bytecode for standalone `throw` statements
- valid verification fixtures demonstrate `athrow` in generated methods and constructors
- abrupt completion after `throw` behaves consistently with existing semantic reachability checks
- declared `throws` metadata is emitted if included in scope
- `cargo run -p verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [x] Confirm or extend colocated semantic tests for `throw` operand validation.
- [x] Implement `Stmt::Throw` bytecode emission with `athrow`.
- [x] Add valid verification fixtures for thrown exceptions in methods.
- [ ] Add valid verification fixtures for thrown exceptions in constructors.
- [x] Verify abrupt completion after `throw` remains semantically correct.
- [x] Inspect verification output to confirm `athrow` appears in generated bytecode.
- [ ] Emit classfile `Exceptions` attributes from declared `throws` clauses if kept in scope.
- [ ] Add or update verification fixtures for declared `throws` metadata if emitted.
- [ ] Add or update invalid verification fixtures and error-message overrides if throw diagnostics change.
- [x] Run `cargo run -p verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
