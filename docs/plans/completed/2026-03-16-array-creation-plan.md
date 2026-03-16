# What is the plan for implementing array creation bytecode generation?

## Why is a dedicated plan needed now?

The compiler frontend already accepts and types array creation expressions, but bytecode generation still treats them as unsupported.
That leaves a visible gap in the current pipeline: source programs containing `new` array expressions can pass parsing, resolution, and attribute analysis, yet generation still emits an unsupported-feature runtime stub instead of real JVM array bytecode.

This is a good next milestone because it is a narrow backend gap with clear existing frontend support and strong verification value.

## What is the current implementation baseline?

The current codebase already has:

- parsing support for `Expr::NewArray`
- resolution support that computes the resulting array type in [resolution.rs](/data/projects/rajac/crates/compiler/src/stages/resolution.rs)
- attribute-analysis checks that require array dimensions to be int-compatible in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs)
- bytecode emission for array access and `arraylength` in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs)
- pretty-print support for `newarray`, `anewarray`, and `multianewarray` in [pretty_print.rs](/data/projects/rajac/crates/bytecode/src/pretty_print.rs)

The current gap is in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs), where `AstExpr::NewArray { .. }` still calls the shared unsupported-feature helper instead of lowering the expression.

## What behavior should be implemented?

rajac should lower array creation expressions into the appropriate JVM instructions instead of emitting unsupported-feature stubs.

The first implementation should support:

- one-dimensional primitive arrays via `newarray`
- one-dimensional reference arrays via `anewarray`
- multi-dimensional arrays via `multianewarray` when the AST carries more than one dimension expression

The emitted bytecode should leave the created array reference on the operand stack, so array creation composes correctly with assignment, method arguments, return statements, and nested expressions.

## How should lowering choose the JVM instruction?

Instruction selection should be driven by the resolved expression type rather than by re-parsing surface syntax.

The bytecode generator should:

1. read the resolved type of the `Expr::NewArray` node
2. determine the total array rank from that type
3. evaluate each dimension expression in source order
4. choose the opcode that matches the element category and dimension count

The selection rules should be:

- use `newarray` for primitive element types when exactly one dimension is being allocated
- use `anewarray` for reference element types when exactly one dimension is being allocated
- use `multianewarray` when more than one dimension expression is present

This keeps the lowering rules aligned with JVM semantics and avoids ad hoc inspection of AST type syntax.

## How should type-to-bytecode mapping work?

The implementation should add a small helper in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs) for translating resolved rajac types into the constant-pool or enum forms needed by JVM array instructions.

That helper should cover:

- primitive element types mapped to the corresponding JVM `newarray` kind
- reference element types mapped to a class constant for `anewarray`
- full array descriptors mapped to a class constant for `multianewarray`

It should prefer using the existing descriptor helpers already present in the bytecode module rather than introducing a second descriptor-formatting path.

## What stack and constant-pool behavior should be validated?

Array creation is stack-sensitive, so the implementation should update stack accounting explicitly.

The code generator should:

- emit each dimension expression before the allocation instruction
- consume one stack operand per dimension
- push exactly one array reference result
- update `max_stack` consistently for nested array expressions

Constant-pool handling should:

- avoid placeholder indexes such as `0`
- create the correct class entry for `anewarray`
- create the correct array descriptor class entry for `multianewarray`

## What should remain out of scope for the first milestone?

This plan should focus on array allocation bytecode, not the full Java array feature set.

The first implementation should not expand scope into:

- array initializer lowering such as `new int[] { 1, 2, 3 }` unless the current AST already routes that through the same expression shape cleanly
- semantic validation beyond the dimension checks already owned by attribute analysis
- optimization differences versus `javac` that are not needed for semantic parity

If partially specified array dimensions exist in the AST for cases such as `new int[2][]`, the implementation should handle only the cases that the current frontend already resolves unambiguously and should document any deferred forms explicitly.

## What architecture changes should accompany the work?

The implementation should keep bytecode generation readable and avoid growing a single large `match` arm.

The recommended refactors are:

- add a dedicated helper such as `emit_new_array_expression(...)`
- add a helper for primitive `newarray` kind selection
- add a helper for computing the class or descriptor constant needed by `anewarray` and `multianewarray`
- add focused colocated tests near bytecode generation rather than relying only on end-to-end verification

If the new helper needs to store type names or messages, it should use `SharedString` where persistent struct fields are introduced.

## What is the recommended implementation order?

1. Audit the exact `Expr::NewArray` AST shape used by the parser for one-dimensional and multi-dimensional inputs.
2. Add a bytecode helper that lowers `Expr::NewArray` using resolved expression types.
3. Implement primitive one-dimensional array emission with `newarray`.
4. Implement reference one-dimensional array emission with `anewarray`.
5. Implement multi-dimensional array emission with `multianewarray`.
6. Add or update stack-accounting helpers so allocation instructions report correct stack effects.
7. Add colocated unit tests for primitive, reference, and multi-dimensional array creation.
8. Add valid verification fixtures under `verification/sources` that exercise the new bytecode paths.
9. Regenerate OpenJDK reference output with `./verification/compile.sh`.
10. Run `cargo run -p rajac-verification --bin verification`.
11. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should be colocated with bytecode generation and should verify instruction shape directly where practical.

The first colocated test set should include:

- primitive one-dimensional array creation emits `newarray`
- reference one-dimensional array creation emits `anewarray`
- multi-dimensional array creation emits `multianewarray`
- array creation leaves a reference result that can be returned or assigned without stack imbalance

Verification fixtures under `verification/sources` should include small valid examples such as:

- a method that returns `new int[n]`
- a method that returns `new String[n]`
- a method that returns `new int[x][y]`

These fixtures should stay small enough that pretty-printed class-file diffs clearly show the intended array instruction.

## What assumptions and risks should stay explicit?

This plan assumes:

- the current frontend already resolves array creation result types correctly for the supported forms
- the `ristretto_classfile` instruction model exposes the array opcodes needed without additional library work
- OpenJDK output for the chosen fixtures is stable enough to compare structurally in verification

The main risks are:

- array descriptor construction for `multianewarray` may be easy to get subtly wrong
- stack-effect accounting for nested array expressions may regress if it is handled ad hoc
- partially specified multi-dimensional forms may require a narrower first implementation than the syntax superficially suggests

If those risks materialize, the implementation should narrow scope explicitly in this plan rather than silently shipping partial support.

## What completion criteria should define success?

This array-creation milestone should be considered complete when:

- `Expr::NewArray` no longer falls back to the unsupported-feature helper for the supported forms
- primitive, reference, and multi-dimensional array allocation emit the appropriate JVM instructions
- colocated tests cover the core lowering paths
- valid verification fixtures demonstrate OpenJDK-compatible array allocation bytecode
- `cargo run -p rajac-verification --bin verification` passes
- `./scripts/check-code.sh` passes

That completion bar is now met for the supported array-creation forms covered by this plan.

## What checklist tracks the work?

- [x] Audit the current `Expr::NewArray` AST and resolved-type shapes.
- [x] Add a dedicated bytecode helper for array creation lowering.
- [x] Implement primitive one-dimensional array allocation with `newarray`.
- [x] Implement reference one-dimensional array allocation with `anewarray`.
- [x] Implement multi-dimensional array allocation with `multianewarray`.
- [x] Update stack accounting for array allocation instructions if needed.
- [x] Add colocated bytecode-generation tests for array creation.
- [x] Add valid verification fixtures for primitive, reference, and multi-dimensional arrays.
- [x] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
