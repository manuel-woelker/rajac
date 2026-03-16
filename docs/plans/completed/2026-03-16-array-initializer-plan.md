# What is the plan for implementing array initializer support?

## Why is a dedicated plan needed now?

rajac now supports array allocation expressions such as `new int[n]`, `new String[n]`, and `new int[x][y]` through parsing, semantic analysis, bytecode generation, and verification.
The next obvious gap in the array feature set is array initializer syntax such as `new int[] { 1, 2, 3 }` and nested forms such as `new int[][] { { 1 }, { 2, 3 } }`.

That gap matters because array initializers are a common Java surface feature and they exercise all major compiler stages:

- parser support for a new expression shape
- semantic checks for element compatibility and nesting
- bytecode generation for allocation plus element stores
- verification against OpenJDK output

## What is the current implementation baseline?

The current codebase already has:

- AST and parser support for `Expr::NewArray` with explicit dimension expressions
- resolution support for array result types in [resolution.rs](/data/projects/rajac/crates/compiler/src/stages/resolution.rs)
- attribute-analysis checks for array dimensions, indexing, `.length`, and array assignment compatibility in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs)
- bytecode generation for array allocation with `newarray`, `anewarray`, and `multianewarray` in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs)
- verification fixtures for array allocation bytecode under `verification/sources/verify/arrays`

The current AST does not appear to model array initializer expressions yet.
`Expr::NewArray` only stores a base type plus a list of dimension expressions in [ast.rs](/data/projects/rajac/crates/ast/src/ast.rs), so array initializer syntax will need an AST design decision before implementation can proceed.

## What behavior should be implemented?

rajac should accept Java array initializer expressions and lower them into standard JVM bytecode.

The first milestone should support:

- `new int[] { 1, 2, 3 }`
- `new String[] { "a", "b" }`
- nested reference and primitive cases such as `new int[][] { { 1 }, { 2, 3 } }`

The implementation should also support array initializers in variable declarations, field initializers, return statements, and method arguments so long as those forms reuse the same expression node and typing rules cleanly.

## What AST shape should be introduced?

The AST should model array initializers explicitly rather than encoding them indirectly through missing dimensions or parser side effects.

The cleanest first design is likely one of these:

1. extend `Expr::NewArray` to carry either dimensions or an initializer payload
2. add a sibling expression such as `Expr::ArrayInitializer { ty, elements }`

The implementation should prefer the option that keeps later semantic analysis and bytecode generation simpler.
The important requirement is that nested initializers can be represented recursively without ambiguity.

## How should parsing work?

The parser should distinguish between:

- dimension-based allocation such as `new int[n]`
- initializer-based allocation such as `new int[] { 1, 2 }`

It should also parse nested initializer lists recursively for multidimensional arrays.

The parser should reject malformed mixes that the initial milestone does not intend to support.
For example, if partially specified mixed forms such as `new int[2] { 1, 2 }` are not modeled in the first pass, the plan should keep that restriction explicit rather than leaving the behavior accidental.

## What semantic rules should attribute analysis own?

Attribute analysis should validate array initializers as a source-level typing feature, not leave compatibility checking to bytecode generation.

The first semantic pass should cover:

- element compatibility with the declared array element type
- nested initializer compatibility for multidimensional arrays
- empty initializer lists
- assignment compatibility when the array initializer result is used in a declaration, return, or assignment

The diagnostics should identify both the found element type and the required element type when compatibility fails.

## How should lowering work in bytecode generation?

Bytecode generation should lower array initializers into explicit allocation plus element stores.

The general lowering shape should be:

1. push the array length
2. allocate the array with `newarray` or `anewarray`
3. duplicate the array reference as needed
4. push the element index
5. emit the element value or nested initializer
6. store the element with the correct array store instruction

Nested initializers for multidimensional arrays should recursively emit sub-array creation before storing each sub-array reference into the outer array.

The implementation should keep stack accounting explicit, because repeated `dup` and store sequences are easy to get wrong.

## What should remain out of scope for the first milestone?

The first implementation should stay focused on explicit `new T[] { ... }` forms.

It should not expand scope into:

- shorthand declaration-only initializers such as `int[] xs = { 1, 2, 3 };` unless the parser and semantic model can support them cleanly as part of the same design
- annotation element values
- constant-folding optimizations for initializer contents
- every legal JLS combination of dimensions and initializer clauses in one step

If declaration shorthand is deferred, the plan should say so clearly and treat it as a follow-up milestone.

## What architecture changes should accompany the work?

This feature will span multiple stages, so the implementation should introduce small, explicit helpers rather than pushing more complexity into giant match arms.

The expected changes are:

- AST updates for array initializer representation
- parser helpers for initializer lists
- attribute-analysis helpers for recursive element validation
- bytecode helpers for array initializer lowering and array store instruction selection
- colocated tests in parser, attribute analysis, and bytecode generation

Any new persistent struct fields that store strings should use `SharedString`.

## What is the recommended implementation order?

1. Define the AST shape for array initializer expressions.
2. Add parser support for `new T[] { ... }` and nested initializer lists.
3. Add parser regression tests for primitive, reference, and nested array initializers.
4. Resolve and type array initializer expressions in resolution and attribute analysis.
5. Add semantic diagnostics for incompatible initializer element types.
6. Lower primitive and reference one-dimensional array initializers in bytecode generation.
7. Lower nested multidimensional array initializers recursively.
8. Add colocated bytecode-generation tests for initializer lowering.
9. Add valid and invalid verification fixtures for array initializer behavior.
10. Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
11. Run `cargo run -p rajac-verification --bin verification`.
12. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should be colocated with the implementation and should cover each stage that gains logic.

The first parser tests should include:

- `return new int[] { 1, 2, 3 };`
- `return new String[] { "a", "b" };`
- `return new int[][] { { 1 }, { 2, 3 } };`

The first semantic tests should include:

- compatible primitive element initialization
- compatible reference element initialization
- incompatible element type in a primitive array initializer
- incompatible nested element type in a multidimensional initializer

The first bytecode tests should include:

- primitive initializer emits allocation plus primitive array stores
- reference initializer emits allocation plus reference array stores
- nested initializer emits recursive sub-array construction

Verification fixtures under `verification/sources` should include small valid examples for primitive, reference, and nested initializers.
Invalid fixtures under `verification/sources_invalid/typecheck` should cover stable, OpenJDK-comparable type errors if rajac starts emitting array-initializer diagnostics that can be matched by line number.

## What assumptions and risks should stay explicit?

This plan assumes:

- the current compiler should continue to support explicit dimension-based `new` array expressions unchanged
- the AST can be extended without destabilizing recently completed array allocation work
- `ristretto_classfile` already exposes the array store instructions needed for lowering

The main risks are:

- choosing an awkward AST shape that makes nested initializers difficult to type or lower
- stack-accounting bugs in the repeated `dup` and store pattern
- accidental expansion into Java's declaration-shorthand initializer rules before the core expression form is stable

If those risks start to materialize, the implementation should narrow scope explicitly and keep the first milestone centered on explicit `new T[] { ... }` expressions.

## What completion criteria should define success?

This array-initializer milestone should be considered complete when:

- rajac parses explicit array initializer expressions into a stable AST shape
- attribute analysis validates initializer element compatibility for the supported forms
- bytecode generation lowers supported array initializer expressions without unsupported-feature stubs
- colocated tests cover parser, semantic, and bytecode behavior
- verification fixtures demonstrate OpenJDK-compatible output for valid initializer cases
- `cargo run -p rajac-verification --bin verification` passes
- `./scripts/check-code.sh` passes

That completion bar is now met for the explicit array-initializer forms covered by this plan.

## What checklist tracks the work?

- [x] Define the AST representation for array initializer expressions.
- [x] Add parser support for explicit array initializer syntax.
- [x] Add parser regression tests for primitive, reference, and nested initializers.
- [x] Resolve and type array initializer expressions.
- [x] Add semantic diagnostics for incompatible initializer element types.
- [x] Lower primitive array initializer expressions in bytecode generation.
- [x] Lower reference array initializer expressions in bytecode generation.
- [x] Lower nested multidimensional array initializer expressions.
- [x] Add colocated bytecode-generation tests for initializer lowering.
- [x] Add valid verification fixtures for array initializer bytecode.
- [x] Add invalid verification fixtures and overrides if new semantic diagnostics need OpenJDK-compatible coverage.
- [x] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
