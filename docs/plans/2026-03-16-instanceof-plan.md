# What is the plan for implementing `instanceof` bytecode generation?

## Why is a dedicated plan needed now?

`instanceof` is already accepted and type-checked by the frontend, but bytecode generation still lowers it to an unsupported-feature runtime stub.
That makes it a good next milestone: the semantic shape already exists, the backend gap is narrow, and the resulting verification signal is easy to interpret.

## What is the current implementation baseline?

The current codebase already has:

- lexer support for the `instanceof` keyword
- parser support for `Expr::InstanceOf`
- resolution support that assigns boolean result type information
- attribute-analysis checks that require a reference operand and a reference target type in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs)
- pretty-print support for the JVM `instanceof` instruction in [pretty_print.rs](/data/projects/rajac/crates/bytecode/src/pretty_print.rs)

The current backend gap is in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs), where `AstExpr::InstanceOf { .. }` still calls the unsupported-feature helper instead of emitting `Instruction::Instanceof`.

## What behavior should be implemented?

rajac should lower supported `instanceof` expressions into standard JVM bytecode.

The first milestone should support ordinary reference checks such as:

- `obj instanceof String`
- `this instanceof Object`
- `array instanceof Object`
- `null instanceof Object`

The emitted bytecode should:

1. evaluate the left-hand expression
2. resolve the target reference type to the correct class constant
3. emit `instanceof`
4. leave a boolean-compatible int result on the operand stack

This should work both in statement position and in boolean-producing expression contexts such as `if` conditions and returns.

## How should lowering choose the target constant?

Lowering should use the resolved target type attached to the AST type node rather than reconstructing the target from source spelling alone.

The backend should:

1. read the resolved `TypeId` from the `AstType`
2. translate that type into the correct JVM class name or array descriptor
3. add the class entry to the constant pool
4. emit `Instruction::Instanceof`

For ordinary class and interface targets, the class constant should use the internal name such as `java/lang/String`.
For array targets, the class constant should use the array descriptor form such as `[I` or `[Ljava/lang/String;`.

## What stack behavior should be validated?

`instanceof` has simple but important stack behavior.

The code generator should:

- emit the receiver expression first
- consume one reference operand
- push one int-like boolean result
- keep `max_stack` correct for nested expressions and branching conditions

This work should not introduce custom stack hacks if the existing `stack_effect` helper already models `Instruction::Instanceof` correctly.
If it does not, the helper should be updated as part of this work.

## What should remain out of scope for the first milestone?

This plan should stay focused on backend support for the existing `instanceof` expression shape.

The first implementation should not expand scope into:

- pattern matching forms such as `x instanceof String s`
- richer semantic checks beyond the current attribute-analysis rules
- unrelated backend gaps such as `try` statements or `synchronized`

If the current frontend accepts edge cases that are not yet fully modeled in the backend, the implementation should narrow support explicitly rather than silently emitting incorrect bytecode.

## What architecture changes should accompany the work?

The implementation should keep the `emit_expression` match arm small.

The recommended refactors are:

- add a dedicated helper such as `emit_instanceof_expression(...)`
- reuse existing class-name or descriptor helpers instead of introducing a second descriptor-formatting path
- replace the current unsupported-feature test with focused tests that assert real `instanceof` instruction emission

If new persistent struct fields are introduced, string fields should use `SharedString`.

## What is the recommended implementation order?

1. Audit the exact resolved type information available on `Expr::InstanceOf` and its target `AstType`.
2. Add a dedicated bytecode helper for `instanceof` lowering.
3. Translate resolved class and array target types into the correct constant-pool class entry.
4. Emit `Instruction::Instanceof` and validate stack accounting.
5. Replace the current unsupported-feature bytecode test with positive lowering tests.
6. Add valid verification fixtures that exercise class, interface, array, and `null` cases.
7. Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
8. Run `cargo run -p rajac-verification --bin verification`.
9. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should be colocated with bytecode generation and should verify instruction shape directly.

The first colocated test set should include:

- `instanceof` against a class target emits `Instruction::Instanceof`
- `instanceof` against an array target uses the correct array descriptor class constant
- `instanceof` leaves a boolean result that can be consumed by statement-result cleanup or branching code

Valid verification fixtures under `verification/sources` should include small, focused examples such as:

- a method that returns `value instanceof String`
- a method that returns `value instanceof Runnable`
- a method that returns `value instanceof int[]`
- a method that returns `null instanceof Object`

These fixtures should stay small enough that class-file diffs clearly show the intended `instanceof` instruction.

## What assumptions and risks should stay explicit?

This plan assumes:

- attribute analysis already rejects primitive targets and primitive operands for the supported cases
- the classfile library supports `Instruction::Instanceof` without further library work
- existing type-to-descriptor helpers already cover both class and array targets correctly

The main risks are:

- accidentally using Java source names instead of JVM internal names or array descriptors in the constant pool
- missing stack accounting updates if `Instruction::Instanceof` is not currently modeled correctly
- discovering frontend edge cases where the resolved target type is still `TypeId::INVALID`

If those risks materialize, the implementation should narrow scope explicitly and keep the first milestone centered on well-resolved reference targets.

## What completion criteria should define success?

This `instanceof` milestone should be considered complete when:

- `AstExpr::InstanceOf { .. }` no longer falls back to the unsupported-feature helper for supported forms
- bytecode generation emits `Instruction::Instanceof` with the correct class constant for class and array targets
- colocated tests cover the main lowering paths
- verification fixtures demonstrate OpenJDK-compatible output for valid `instanceof` examples
- `cargo run -p rajac-verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [ ] Audit the current resolved-type shape for `Expr::InstanceOf` targets.
- [ ] Add a dedicated bytecode helper for `instanceof` lowering.
- [ ] Implement class-target `instanceof` emission.
- [ ] Implement array-target `instanceof` emission.
- [ ] Update stack accounting if `Instruction::Instanceof` is not modeled correctly today.
- [ ] Replace the unsupported-feature bytecode test with positive lowering tests.
- [ ] Add valid verification fixtures for class, interface, array, and `null` `instanceof` cases.
- [ ] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [ ] Run `cargo run -p rajac-verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
