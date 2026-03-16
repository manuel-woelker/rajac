# What is the plan for implementing enum support?

## Why is a dedicated plan needed now?

The compiler already recognizes enum syntax and carries enum declarations through parts of the frontend, but enums are not implemented end to end.
That makes enums a larger milestone than recent backend-only features such as `instanceof`: the remaining work spans symbol collection, semantic modeling, naming, and classfile generation.

A dedicated plan is needed so the first enum milestone stays narrow, testable, and aligned with JVM enum lowering rules instead of becoming an open-ended language feature bucket.

## What is the current implementation baseline?

The current codebase already has:

- lexer support for the `enum` keyword
- parser support for top-level and nested `enum` declarations in [parser.rs](/data/projects/rajac/crates/parser/src/parser.rs)
- AST nodes for `EnumDecl`, `EnumEntry`, and `ClassMember::NestedEnum` in [ast.rs](/data/projects/rajac/crates/ast/src/ast.rs)
- resolution and attribute-analysis traversal hooks for enum declarations in [resolution.rs](/data/projects/rajac/crates/compiler/src/stages/resolution.rs) and [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs)
- classfile access-flag support for `ACC_ENUM` in [attributes.rs](/data/projects/rajac/crates/bytecode/src/classfile/attributes.rs)

The current implementation still has several structural gaps:

- symbol collection explicitly skips top-level enums in [collection.rs](/data/projects/rajac/crates/compiler/src/stages/collection.rs)
- nested enum naming is skipped in [naming.rs](/data/projects/rajac/crates/bytecode/src/classfile/naming.rs)
- classfile generation only iterates `ClassDecl` members and does not synthesize enum constant fields, `$VALUES`, `values()`, or `valueOf(String)` in [builder.rs](/data/projects/rajac/crates/bytecode/src/classfile/builder.rs)
- the enum parser currently records entry bodies as `None`, so constant-specific class bodies are not implemented yet in [parser.rs](/data/projects/rajac/crates/parser/src/parser.rs)

## What behavior should the first milestone implement?

The first enum milestone should support simple Java enums that are useful and structurally complete on the JVM.

The supported source shape should include:

- top-level enums
- nested enums
- enum constants without arguments
- enum constants with constructor arguments
- enum fields, methods, and constructors declared after the constant list

The implementation should produce class files that behave like ordinary Java enums for:

- `EnumName.CONSTANT` field access
- `EnumName.values()`
- `EnumName.valueOf(String)`
- ordinal/name initialization through the synthetic enum constructor path

## What should remain out of scope for the first milestone?

The first milestone should stay focused on core enum class generation.

It should not expand scope into:

- enum constant-specific class bodies such as `A { ... }`
- enums with implemented abstract methods that require per-constant anonymous subclasses
- exhaustive enum-switch semantics
- every reflection-visible classfile detail if a smaller compatible subset is enough to match OpenJDK output for the chosen fixtures

Those are better follow-up milestones once the base enum model is stable.

## How should symbol collection change?

Enums need to exist as real named types in the symbol table before later stages can work reliably.

Collection should:

1. stop skipping top-level enums
2. register enums with an appropriate symbol kind
3. ensure nested enums are discoverable in the same way nested classes are

If the current `SymbolKind` model does not distinguish enums from classes, the first implementation can treat enums as a class-like symbol as long as later stages can still determine enum semantics from the AST kind or type metadata.
That tradeoff should be explicit in the implementation.

## How should enum types be modeled during resolution?

Resolution should establish the minimum semantic facts needed for backend generation.

The first implementation should ensure:

- enum declarations receive a concrete `TypeId`
- enum types behave like class types for member lookup
- enum declarations implicitly extend `java.lang.Enum`
- enum constant constructor arguments are resolved like ordinary constructor-call arguments
- enum members declared after the constant list are resolved in the enum type context

The implementation should prefer attaching enum-specific facts to existing class-type structures when practical rather than introducing a separate second type hierarchy for the first milestone.

## How should parsing and AST representation evolve?

The parser already captures the basic enum declaration shape, but the AST needs to stay aligned with the first milestone’s scope.

The plan should preserve the current `EnumDecl` and `EnumEntry` representation for now, with only small extensions if needed for generation.
If generation requires storing synthesized constructor metadata or enum-entry source ordering, that should be added in a minimal way.

Constant-specific class bodies should remain explicitly deferred unless the parser work turns out to be trivial and clearly isolated.

## How should bytecode generation lower enums?

Enums are a backend-heavy feature because Java source enums map to a specific synthetic class structure.

The first implementation should generate:

- one `static final` field per enum constant
- a synthetic private static final `$VALUES` array field
- a static initializer that instantiates each constant in declaration order and populates `$VALUES`
- a `values()` method that returns a clone of `$VALUES`
- a `valueOf(String)` method delegating to `java/lang/Enum.valueOf`
- an enum constructor shape that passes `(String name, int ordinal, ...)` to `java/lang/Enum.<init>`

The implementation should use existing bytecode helpers where possible, but it will likely need dedicated enum-specific lowering helpers in the classfile builder layer instead of forcing all synthesis into the statement bytecode generator.

## What classfile and naming changes are required?

Enum generation will require builder-level work beyond ordinary methods and fields.

The first implementation should:

- ensure enum classfiles use the correct `ACC_ENUM` access flag
- emit the correct superclass entry for `java/lang/Enum`
- include nested enums in inner-class naming and `InnerClasses` metadata
- synthesize enum methods and fields before final classfile emission

If synthetic flags or descriptors are needed for generated members, they should be added deliberately rather than copied ad hoc from OpenJDK output.

## What architecture changes should accompany the work?

This feature spans collection, resolution, and classfile generation, so the implementation should avoid hiding enum behavior inside unrelated code paths.

The recommended refactors are:

- add focused enum helpers in collection and resolution rather than open-coding more `match` arms
- add classfile-builder helpers for synthesized enum members and `<clinit>` lowering
- keep ordinary statement bytecode generation separate from enum class synthesis
- add colocated tests close to the builder and resolution logic that gains enum-specific behavior

If new persistent string fields are introduced, they should use `SharedString`.

## What is the recommended implementation order?

1. Decide the exact first-milestone enum surface area and keep constant-specific class bodies out of scope.
2. Teach collection to register top-level enums instead of skipping them.
3. Teach nested-class naming and inner-class metadata collection to include nested enums.
4. Ensure resolution assigns usable enum `TypeId`s and models `java.lang.Enum` as the implicit superclass.
5. Add classfile-builder support for enum class emission, including enum-specific flags and superclass wiring.
6. Synthesize enum constant fields, `$VALUES`, `values()`, and `valueOf(String)`.
7. Synthesize enum construction and `<clinit>` lowering for constants with and without constructor args.
8. Add colocated tests for symbol collection, enum naming, and classfile shape.
9. Add small verification fixtures for simple enums, constructor-argument enums, and nested enums.
10. Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
11. Run `cargo run -p rajac-verification --bin verification`.
12. Run `./scripts/check-code.sh`.

## What tests and verification fixtures should be added?

Tests should be colocated with the code that gains enum-specific logic.

The first colocated test set should include:

- collection registers top-level enums in the symbol table
- nested enum naming includes nested enums in generated internal names
- enum class generation emits enum constant fields and `$VALUES`
- generated `values()` and `valueOf(String)` methods have the expected descriptors

Valid verification fixtures under `verification/sources` should include small examples such as:

- a simple enum with two constants
- an enum with a private constructor argument and a getter
- a class containing a nested enum

These fixtures should stay small enough that pretty-printed classfile mismatches are easy to interpret.

## What assumptions and risks should stay explicit?

This plan assumes:

- the current type system can model enum types as class-like types without major restructuring
- `java.lang.Enum` is available through the existing classpath and symbol mechanisms
- the classfile library already supports the instructions and access flags needed for synthetic enum members

The main risks are:

- underestimating the amount of synthesis needed to match OpenJDK enum class structure
- mixing enum-specific generation into general statement bytecode code paths and making them harder to maintain
- discovering that constructor or member resolution for enums needs a deeper symbol-model change than expected
- accidentally broadening scope into constant-specific subclass bodies too early

If those risks materialize, the implementation should narrow scope explicitly and keep the first milestone centered on plain enums without per-constant bodies.

## What completion criteria should define success?

This first enum milestone should be considered complete when:

- top-level enums are no longer skipped during collection
- simple enums compile into class files with enum constant fields, `$VALUES`, `values()`, and `valueOf(String)`
- enum constructors and static initialization work for the supported constructor-argument forms
- nested enums are named and emitted correctly
- colocated tests cover the core enum-specific collection and generation behavior
- verification fixtures demonstrate OpenJDK-compatible output for the supported enum forms
- `cargo run -p rajac-verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [ ] Define the first-milestone enum scope and keep constant-specific class bodies out of scope.
- [ ] Teach collection to register top-level enums.
- [ ] Teach nested naming and inner-class metadata collection to include nested enums.
- [ ] Ensure resolution models the enum type and implicit `java.lang.Enum` superclass.
- [ ] Add classfile-builder support for enum class emission.
- [ ] Synthesize enum constant fields and `$VALUES`.
- [ ] Synthesize `values()` and `valueOf(String)`.
- [ ] Add enum constructor and `<clinit>` lowering for supported constant forms.
- [ ] Add colocated tests for enum collection, naming, and classfile generation.
- [ ] Add valid verification fixtures for simple, constructor-argument, and nested enums.
- [ ] Regenerate OpenJDK reference outputs with `./verification/compile.sh`.
- [ ] Run `cargo run -p rajac-verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
