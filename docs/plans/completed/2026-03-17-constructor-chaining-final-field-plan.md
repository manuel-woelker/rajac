# What problem is this plan solving?

The compiler now enforces blank `final` instance-field initialization within individual constructor bodies.
The next semantic gap is constructor chaining through `this(...)`.

Without this milestone, rajac cannot correctly reason about blank `final` fields when one constructor delegates initialization work to another constructor in the same class.
That leaves an important correctness hole in Java object-initialization semantics.

# What is the current gap?

The current flow-analysis stage in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs) tracks blank `final` instance fields only within the current constructor body.
It does not yet model:

- constructor delegation through `this(...)`
- field initialization facts transferred from one constructor to another
- duplicate assignment caused by both the delegated constructor and the delegating constructor assigning the same blank `final` field
- constructor-chaining cycles or unsupported delegation shapes

This means the compiler is currently only sound for constructors that initialize blank `final` fields directly in their own body.

# What implementation approach should be used?

Extend flow analysis so constructor checks understand same-class delegation via `this(...)`.

The first milestone should focus on:

- detecting whether a constructor begins with a delegated `this(...)` call
- transferring blank `final` field initialization facts from the target constructor
- rejecting duplicate assignment when both constructors write the same blank `final` field
- preserving the existing conservative branch-sensitive field analysis after delegation

This work should stay in `flow_analysis` rather than spreading constructor-specific logic across resolution, attribute analysis, or generation.

# What should remain out of scope for this milestone?

The first implementation should stay narrowly focused on same-class constructor delegation.

The following should remain out of scope unless they become necessary for correctness in the supported subset:

- richer `super(...)`-driven field-initialization reasoning across inheritance
- field initialization through helper methods or arbitrary call effects
- all exception-sensitive constructor-chaining rules
- record-specific constructor initialization semantics
- advanced overload-selection issues beyond what current resolution already provides

If unsupported constructor-chaining shapes remain after the core implementation, they should fail explicitly rather than compile unsoundly.

# How should delegated constructor information be modeled?

Flow analysis needs a constructor-level model in addition to statement-local flow state.

The first implementation should add:

- a way to identify constructors in the current class
- detection of an initial `this(...)` call in the constructor body
- a constructor-summary or recursive analysis path that captures blank `final` field assignment facts from the target constructor
- explicit cycle detection or recursion guards for constructor delegation

The design should prefer stable constructor identity over name-only matching when practical.

# How should `this(...)` delegation affect blank `final` field state?

The first supported rule set should be:

- if a constructor begins with `this(...)`, the delegated constructor's field-initialization effects happen first
- any blank `final` field definitely assigned by the delegated constructor should enter the delegating constructor body as definitely assigned
- if the delegating constructor assigns the same tracked field again, that is an error
- if the delegated constructor leaves a tracked field uninitialized, the delegating constructor still has to assign it on all normal paths

This preserves the Java rule that a blank `final` field must be assigned exactly once across the whole constructor chain.

# How should constructor cycles and unsupported cases be handled?

Constructor delegation introduces recursive structure, so the implementation should make unsupported or cyclic states explicit.

The first implementation should:

- detect direct and indirect `this(...)` cycles
- surface a clear diagnostic or controlled unsupported-feature error for cycles if the compiler cannot already reject them earlier
- keep delegation support limited to cases where the target constructor is resolved reliably

The compiler should not silently ignore delegated initialization or guess at recursive behavior.

# How should diagnostics be designed?

Diagnostics should stay aligned with OpenJDK where practical.

The first diagnostic set should cover:

- variable `<field>` might not have been initialized
- variable `<field>` might already have been assigned

If the constructor-chaining implementation needs an explicit unsupported or cycle diagnostic for cases outside the supported subset, that wording should be stable and verification-aware.

# What architecture changes should accompany the work?

This milestone should deepen the flow-analysis stage without broadening unrelated stages.

The implementation should:

- extend constructor analysis in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs)
- keep any constructor-summary or recursion-guard structures local to the flow-analysis stage
- avoid duplicating constructor-field logic in attribute analysis
- add concise comments only where delegation ordering or recursion handling is non-obvious

# What order should the implementation follow?

1. Add this plan in `docs/plans`.
2. Identify how delegated `this(...)` constructor calls appear in the resolved AST.
3. Add constructor-level analysis support for detecting and following `this(...)` delegation.
4. Transfer blank `final` field assignment state from delegated constructors into delegating constructors.
5. Reject duplicate blank `final` field assignment across a delegated constructor and the delegating constructor body.
6. Require all tracked blank `final` fields to be initialized across the full constructor chain on normal completion.
7. Add recursion guards or cycle handling for constructor delegation.
8. Add colocated tests for valid delegated initialization, duplicate assignment, and missing assignment through `this(...)`.
9. Add valid verification fixtures for successful blank `final` field initialization via delegated constructors.
10. Add invalid verification fixtures for missing initialization and for a `final` field being initialized twice across a constructor and `this()`.
11. Add or update verification error-message overrides if needed.
12. Run `cargo run -p rajac-verification --bin verification`.
13. Run `./scripts/check-code.sh`.
14. Move the completed plan to `docs/plans/completed`.

# What assumptions matter for this work?

- Resolution already provides enough information to recognize `this(...)` constructor calls reliably in the current subset.
- Flow analysis remains the correct owner for constructor-chaining initialization checks.
- Conservative handling is acceptable for the first milestone if it avoids unsound acceptance.
- Verification fixtures under `verification/sources` and `verification/sources_invalid/typecheck` are the right compatibility mechanism for this work.

# How will this work be verified?

Verification should cover both focused constructor-chaining behavior and end-to-end compiler output.

The expected verification work is:

- add colocated unit tests for constructor delegation and blank `final` field state transfer
- add valid verification fixtures for successful blank `final` field initialization through `this(...)`
- add invalid verification fixtures for missing initialization through constructor chaining
- add invalid verification for a `final` field being initialized twice in a constructor and `this()`
- run targeted crate tests or `cargo test` for the affected compiler code
- run `cargo run -p rajac-verification --bin verification`
- run `./scripts/check-code.sh`

# What concrete work items are planned?

- [x] Create this plan in `docs/plans`.
- [x] Add constructor-level flow support for detecting delegated `this(...)` calls.
- [x] Transfer tracked blank `final` field state from delegated constructors.
- [x] Reject duplicate blank `final` field assignment across `this(...)` chains.
- [x] Require tracked blank `final` fields to be initialized across normal completion of the full constructor chain.
- [x] Add recursion guards or cycle handling for constructor delegation.
- [x] Add colocated tests for valid and invalid blank `final` field constructor chaining.
- [x] Add valid verification fixtures for successful initialization via `this(...)`.
- [x] Add or update invalid verification fixtures under `verification/sources_invalid/typecheck`, including a case where a `final` field is initialized twice across a constructor and `this()`.
- [x] Add or update verification error-message overrides if needed.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
