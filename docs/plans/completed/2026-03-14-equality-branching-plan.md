# What is the plan for implementing equality, branches, label management, and backpatching?

## What did verification show?

The original verification run showed that equality was not yet lowered into JVM branch bytecode.
Primitive and object equality methods evaluated operands and returned immediately without emitting comparison instructions.
That run also exposed a separate method call issue: `String.equals` was emitted with a `void` descriptor instead of a `boolean` return type.

## What is the current status?

The bytecode generator now has label-based branch assembly, conditional lowering for equality, boolean materialization, `Stmt::If` emission, and loop emission for `Stmt::While`, `Stmt::DoWhile`, and `Stmt::For`.
Verification coverage now includes equality fixtures plus control-flow fixtures for `if`, `while`, `do while`, and `for`.
On 2026-03-15, `cargo run -p rajac-verification --bin verification` passed with 27 valid files matching and 11 invalid files verified, and `./scripts/check-code.sh` passed with 127 tests.
The remaining implementation work is no longer centered on control-flow emission.
The bytecode generator now supports `break`, `continue`, labeled statements, and `switch` lowering in addition to equality, `if`, and loops.
Verification coverage also includes fixtures for unlabeled and labeled control flow plus both `tableswitch` and `lookupswitch`.

## What needs to be built first?

A real control-flow assembly layer was the first dependency for the rest of this work.
That layer is now in place, and structured control-flow context has been added for non-local exits such as `break`, `continue`, and labeled statements.

## What is the implementation plan?

1. Introduce a label abstraction such as `LabelId` in `crates/bytecode/src/bytecode.rs`.
2. Add an internal emitted-instruction representation that can store unresolved branches to labels.
3. Add an assembly pass that computes byte offsets, resolves label positions, and patches branch operands.
4. Replace the current ad hoc `patch_branch` flow used by `&&` and `||` with the new label-based system.
5. Split expression lowering into two forms: one that produces a value and one that emits conditional control flow.
6. Implement `==` and `!=` lowering in conditional form for primitive and reference types.
7. Materialize boolean results with `iconst_1`, `iconst_0`, and `goto` when a comparison is used as an expression value.
8. Implement `Stmt::If` lowering on top of the new conditional emission path.
9. Implement `Stmt::While`, `Stmt::DoWhile`, and `Stmt::For` using the same label and branch infrastructure.
10. Add a control-flow stack for `break`, `continue`, and labeled statements so loop and switch support has a stable foundation.
11. Fix method call descriptor generation so `emit_method_call` uses the resolved return type instead of always producing `(... )V`.
12. Expand verification coverage for equality inside expressions and inside real control-flow statements.
13. Implement `Stmt::Break`, `Stmt::Continue`, and `Stmt::Label` emission on top of the control-flow stack.
14. Implement `Stmt::Switch` lowering once labeled control-flow infrastructure exists.

## How should equality be lowered?

Primitive equality should use JVM comparison instructions that match the operand kind.

- Integer-like primitives should use `if_icmpeq` and `if_icmpne`.
- `long` should use `lcmp` followed by `ifeq` or `ifne`.
- `float` should use `fcmpl` or `fcmpg` followed by `ifeq` or `ifne`.
- `double` should use `dcmpl` or `dcmpg` followed by `ifeq` or `ifne`.
- Reference equality should use `if_acmpeq` and `if_acmpne`.
- Comparisons with `null` should use `ifnull` and `ifnonnull` when possible.

## Why should method calls be included in this plan?

The new object equality example uses `String.equals`, and verification showed that method invocation descriptors are still incomplete.
Even after branch support is added, `String.equals` will remain wrong until method call return descriptors are emitted correctly.

## What is actually left to do?

The original remaining bytecode gap was `Stmt::Break`, `Stmt::Continue`, `Stmt::Label`, and `Stmt::Switch`.
Those statements are now emitted in `crates/bytecode/src/bytecode.rs` with structured control-flow context and OpenJDK-matching verification fixtures.
Future work in this area is now about deeper semantic validation and broader switch coverage rather than basic bytecode emission.

## What is the recommended implementation order?

1. Add label and backpatch infrastructure.
2. Migrate existing `&&` and `||` lowering to the new infrastructure.
3. Implement conditional emission for boolean expressions.
4. Implement `==` and `!=` lowering.
5. Implement `Stmt::If`.
6. Implement loops.
7. Fix method call descriptors and return typing.
8. Add more verification cases and run the verification suite.
9. Add structured control-flow context for `break`, `continue`, and labels.
10. Implement `break`, `continue`, labeled statements, and then `switch`.

## What checklist tracks the work?

- [x] Add `LabelId` and label binding support in `crates/bytecode/src/bytecode.rs`.
- [x] Add unresolved branch recording and a final resolution pass based on bytecode offsets.
- [x] Convert `&&` and `||` lowering to the new branch infrastructure.
- [x] Add a dedicated conditional emission path for boolean expressions.
- [x] Implement primitive `==` and `!=` lowering.
- [x] Implement reference `==` and `!=` lowering.
- [x] Optimize `null` comparisons to `ifnull` and `ifnonnull`.
- [x] Implement boolean materialization for comparison expressions.
- [x] Implement `Stmt::If` bytecode emission.
- [x] Implement `Stmt::While` bytecode emission.
- [x] Implement `Stmt::DoWhile` bytecode emission.
- [x] Implement `Stmt::For` bytecode emission.
- [x] Add a control-flow stack for `break`, `continue`, and labeled statements.
- [x] Implement `Stmt::Break` bytecode emission.
- [x] Implement `Stmt::Continue` bytecode emission.
- [x] Implement `Stmt::Label` bytecode emission.
- [x] Implement `Stmt::Switch` bytecode emission.
- [x] Fix method invocation descriptor generation to include return types.
- [x] Add verification sources that exercise equality inside `if` statements and loops.
- [x] Add verification sources for `break`, `continue`, labeled statements, and `switch` when implemented.
- [x] Run `cargo run -p rajac-verification` and compare the output.
- [x] Run `./scripts/check-code.sh`.
