# What is the plan for implementing robust type checking and error reporting?

## Why is a dedicated plan needed now?

The compiler pipeline now includes an attribute analysis stage, but that stage is still mostly a placeholder.
Resolution assigns some expression types and resolves declared types, but it does not yet provide robust local binding, assignment checking, overload validation, or diagnostic quality expected from a real Java front end.

The current implementation also leaks semantic responsibility into bytecode generation.
For example, bytecode emission still maintains a local variable map to recover slot and kind information that should ultimately come from a completed semantic analysis phase.

## What is the current state of type checking?

The parser builds declared local variable types into `Stmt::LocalVar`.
The resolution stage resolves declared types and assigns some `TypedExpr.ty` values to expressions such as literals, unary expressions, binary expressions, method calls, and field access.

However, local identifiers are not yet robustly bound as symbols, local scopes are not modeled as first-class semantic environments, and there is no comprehensive pass that checks:

- assignment compatibility
- local variable lookup and shadowing
- method applicability and overload selection quality
- boolean condition requirements
- return statement compatibility
- numeric conversions
- invalid use of `void`
- diagnostic spans and messages for semantic errors

## What should the type-checking stage own?

The attribute analysis stage should become the canonical semantic analysis phase for source programs after parsing, collection, and structural resolution.
It should own:

- local and parameter binding
- expression typing
- statement typing rules
- assignment compatibility
- method invocation checking
- field access checking
- constructor call checking
- boolean condition validation
- return checking
- cast and `instanceof` validation
- constant expression folding and classification
- semantic diagnostics with precise source locations

Resolution should remain focused on resolving declared names and populating symbol/type references needed before semantic checking.
Attribute analysis should consume that information and produce a semantically typed AST.

## What architecture should be introduced first?

The first implementation step should be a real semantic environment model for locals and parameters.
Without that, every later typing rule becomes ad hoc.

The attribute stage should introduce:

1. A scoped environment stack for locals, parameters, and enclosing class context.
2. A per-expression result structure that records computed type, constant value when available, and any resolved symbol or conversion metadata needed later.
3. A diagnostic emitter helper specialized for semantic errors so error construction is consistent across the stage.

This should be implemented in `crates/compiler/src/stages/attribute_analysis.rs`, but the logic should be split into smaller internal structs and helper functions rather than kept as one traversal file.

## How should local binding work?

Local binding should be explicit and source-order aware.

The attribute stage should:

- enter method parameters before typing the method body
- enter local variables only after their initializer has been checked when Java rules require it
- support nested block scopes
- support `for` initializer scopes
- support shadowing rules for locals and parameters
- reject duplicate local declarations within the same scope
- distinguish local variables from fields, types, and packages during identifier lookup

The result should be that `Expr::Ident` is resolved through a local-first lookup path before any fallback to fields or types.

## How should expression typing be structured?

Expression typing should use a single recursive entry point that returns a semantic result for every expression node.

That result should include at least:

- computed `TypeId`
- whether the expression is assignable
- whether the expression is a constant expression
- the constant value if available

The implementation order should be:

1. literals and parenthesized expressions
2. local identifiers and parameters
3. unary expressions
4. binary numeric expressions
5. assignment expressions
6. comparison and boolean expressions
7. ternary expressions
8. field access
9. method calls and constructor calls
10. casts, `instanceof`, arrays, `this`, and `super`

## What statement checks are required?

Statement typing should validate the rules that depend on expression types and local environments.

The first statement rules should cover:

- expression statements
- local variable declarations with initializer compatibility
- `if`, `while`, `do-while`, and `for` condition expressions requiring `boolean`
- `return` statements matching the enclosing method return type
- `throw` expressions having throwable reference type
- block scoping

After that, the stage should expand to:

- `switch`
- `break` and `continue`
- try/catch/finally typing
- synchronized blocks

## How should assignment compatibility be implemented?

Assignment compatibility should be a dedicated helper instead of being inlined into every statement or expression case.

That helper should model:

- identity conversion
- primitive widening
- reference assignability
- null assignment to reference types
- assignment to arrays and class types
- rejection of assignment to non-assignable expressions

It should also support targeted diagnostics that explain both sides of the mismatch.
For example, diagnostics should identify the found type and required type, not just state that the assignment is invalid.

## How should method invocation checking be implemented?

Method calls should be typed in two phases:

1. Resolve the candidate receiver type and collect argument types.
2. Select the best applicable method and report useful diagnostics when selection fails.

The first implementation should stay deliberately narrow:

- exact parameter count matching
- exact type matching plus primitive widening where already supported
- instance versus static dispatch validation

Once that works reliably, the stage can extend toward:

- boxing and unboxing
- varargs
- generic inference
- better tie-breaking for overload resolution

## How should diagnostics be designed?

Semantic diagnostics should be treated as a first-class output of type checking, not as incidental strings.

The plan should use the existing `rajac_diagnostics::Diagnostic` infrastructure and add helper constructors for common semantic failures.
Each diagnostic should carry:

- a short stable message
- one primary source chunk for the error location
- optional secondary annotations for related declarations or expected types

The first diagnostic set should cover:

- cannot find symbol for local or member lookup
- incompatible types in assignment
- incompatible return type
- non-boolean loop or `if` condition
- invalid operand types for unary or binary operator
- method not found or no applicable overload
- duplicate local variable declaration

The verification workflow already supports improved error message overrides.
That should be used when rajac's semantic errors become better than OpenJDK's wording.

## What changes should be avoided in the first implementation?

The first robust type-checking pass should not try to solve the whole Java language.
It should avoid:

- full generic type inference
- boxing and unboxing everywhere
- wildcard capture
- complete annotation semantics
- full definite assignment analysis

Those are better treated as follow-up milestones once local binding, expression typing, and diagnostics are stable.

## What refactors should accompany the implementation?

Several refactors should happen alongside the attribute pass so the compiler architecture becomes cleaner.

- Move semantic helpers out of bytecode generation where they currently compensate for missing typing.
- Introduce reusable scope and environment types instead of open-coded hash maps inside the stage.
- Keep `lib.rs` and stage module wiring thin, and place new semantic structs in dedicated files if they grow large.
- Add RustDoc on any new semantic context or result structs, including fields that will matter for maintenance.

## What is the recommended implementation order?

1. Define semantic environment and scope data structures for locals and parameters.
2. Add a semantic diagnostic helper layer in attribute analysis.
3. Implement local and parameter lookup in the attribute stage.
4. Type local declarations and assignment expressions.
5. Type unary and binary expressions with operator validation.
6. Validate boolean conditions in `if`, `while`, `do-while`, and `for`.
7. Validate method return statements against enclosing method type.
8. Type field access and method invocation using resolved receiver information.
9. Move bytecode generation off ad hoc local typing where semantic results are now available.
10. Expand tests and verification fixtures for semantic error reporting and successful typing.

## What tests should be added?

Tests should be colocated with the implementation and should prefer data-driven structure where possible.

The first test set should include:

- valid local binding in nested blocks
- duplicate local declaration errors
- assignment type mismatch errors
- non-boolean condition errors
- invalid unary and binary operand combinations
- return type mismatch errors
- unresolved local and member symbol errors
- successful typing of arithmetic, comparison, and assignment expressions

Verification fixtures should also be added for semantic failures where rajac now reports precise diagnostics.

## What completion criteria should define success?

The first robust type-checking milestone should be considered complete when:

- attribute analysis performs real semantic checking for locals, assignments, conditions, returns, and basic method calls
- local identifier expressions are typed through scoped lookup
- bytecode generation no longer needs to invent local typing behavior that should come from semantic analysis
- semantic diagnostics are precise enough to add or update verification overrides where appropriate
- `cargo run -p rajac-verification --bin verification` remains green
- `./scripts/check-code.sh` remains green

## What checklist tracks the work?

- [x] Add scoped local and parameter environments to attribute analysis.
- [x] Add semantic diagnostic helpers for type errors and missing symbol errors.
- [x] Bind `Expr::Ident` against locals, parameters, and members in a deterministic lookup order.
- [x] Type local variable declarations and validate initializer compatibility.
- [x] Type assignment expressions and reject assignment to non-assignable expressions.
- [x] Type unary expressions and validate operand kinds.
- [x] Type binary arithmetic, comparison, and boolean expressions.
- [x] Validate `if`, `while`, `do-while`, and `for` conditions as boolean.
- [x] Validate `return` statements against enclosing method return type.
- [x] Type field access and basic method invocation applicability.
- [x] Record constant-expression results where that simplifies later passes.
- [x] Remove bytecode-generation workarounds that duplicate semantic local typing.
- [x] Add colocated tests for semantic typing and diagnostics.
- [x] Add or update verification fixtures and error message overrides for semantic diagnostics.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
