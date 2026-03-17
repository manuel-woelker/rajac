# What problem is this plan solving?

The compiler now enforces `final` flow rules for locals and parameters.
The next semantic gap is blank `final` field initialization, especially for instance fields assigned in constructors.

Without this milestone, rajac can still accept classes that fail to initialize required `final` fields on every constructor path or that assign them more than once.
That leaves an important part of Java object-initialization semantics unimplemented.

# What is the current gap?

The current flow-analysis stage in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs) tracks path-sensitive assignment for locals and parameters only.
It does not yet model:

- blank `final` instance fields declared without initializer
- assignment obligations across constructor bodies
- reassignment of `final` fields after initialization
- constructor-path joins for field initialization

The parser, resolution, and attribute-analysis pipeline already carries field modifiers and constructor bodies, so the missing work is primarily semantic modeling and verification.

# What implementation approach should be used?

Extend the flow-analysis stage so constructor analysis can track blank `final` instance fields alongside local state.

The first milestone should stay deliberately narrow:

- support instance fields only
- support direct assignment inside constructors
- require every constructor path to initialize each blank `final` instance field exactly once unless it is definitely initialized before the constructor body

This work should build on the existing flow-analysis framework instead of introducing separate constructor-only checks elsewhere.

# What should remain out of scope for this milestone?

The first implementation should not try to solve every Java field-initialization rule.

The following should remain out of scope unless they fall out naturally from the implementation:

- static blank `final` field initialization
- full `this(...)` constructor-chaining semantics
- subtle initialization-order rules involving instance initializers and field initializers beyond the supported subset
- inner-class capture and synthetic field initialization interactions
- all JLS corner cases for exception-driven constructor completion

If constructor chaining turns out to be necessary for soundness in the supported subset, it should be added deliberately rather than implicitly.

# How should field initialization state be modeled?

Constructor flow needs a second tracked state alongside locals.

The first implementation should add:

- a representation of tracked blank `final` instance fields for the current class
- per-constructor assignment state for those fields
- helpers that distinguish first assignment from illegal reassignment
- merge logic for constructor branches that remains conservative

Field tracking should prefer stable field identity from the AST or resolved field information instead of using only raw names when practical.
If the first implementation uses names, that limitation should stay explicit and narrowly scoped to the current class.

# Which fields should be tracked?

The first milestone should track only blank `final` instance fields declared in the current class.

That means:

- `final` instance fields with an initializer do not need constructor assignment
- non-`final` fields are out of scope
- static fields are out of scope
- inherited fields are out of scope unless later work proves they must participate

The goal is to implement the common Java rule: every blank `final` instance field declared in the class must be definitely assigned on normal completion of each constructor.

# How should constructor analysis enforce the rule?

The first rules should include:

- constructor parameters remain definitely assigned at entry as today
- each blank `final` instance field starts unassigned at constructor entry unless already covered by a supported initializer path
- the first assignment to a blank `final` field is allowed
- any second assignment to that field within the constructor flow is an error
- normal constructor completion requires all tracked blank `final` fields to be definitely assigned
- abrupt completion does not by itself satisfy field-initialization requirements

Assignments in both branches of an `if` should count as successful initialization when the join proves the field is assigned on all normal paths.

# How should field writes be detected?

The first implementation should support the direct field-write forms already represented in the AST.

That should include:

- simple assignment through `this.field = expr`
- simple assignment through unqualified `field = expr` when it resolves to the current instance field

The milestone should reject duplicate assignment through those forms before expanding to more complex cases.
If increment/decrement of final fields is reachable in the supported subset, it should also be treated as assignment and rejected.

# How should control-flow joins behave?

The field-tracking merge rules should match the conservative local-flow strategy.

The first merge rules should be:

- a field is definitely assigned after a join only if all normal-completion paths assign it
- assigning the field in both branches of an `if` is valid
- assigning the field in only one branch leaves it maybe uninitialized
- loops should remain conservative unless the analysis can prove assignment on all normal paths
- a second assignment on any path should emit an error at the second assignment site

This may reject some edge cases initially, but it must not accept constructors that can complete without initializing a blank `final` field.

# How should diagnostics be designed?

Diagnostics should stay close to OpenJDK where practical so verification remains straightforward.

The first diagnostic set should cover:

- variable `<field>` might not have been initialized
- cannot assign a value to final variable `<field>`

If OpenJDK uses field-specific wording that differs materially, the implementation can either match it directly or add verification overrides in [verification_main.rs](/data/projects/rajac/crates/verification/src/verification_main.rs).

# What architecture changes should accompany the work?

This milestone should reinforce `flow_analysis` as the owner of path-sensitive initialization checks.

The implementation should:

- extend the flow-analysis state model rather than adding constructor-field rules to attribute analysis
- keep any new constructor/field helper types close to [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs)
- avoid broad parser or resolution refactors unless they are required for stable field identity
- add concise comments only where a constructor-flow transfer rule is non-obvious

# What order should the implementation follow?

1. Add this plan in `docs/plans`.
2. Confirm what field identity information is available during constructor flow analysis.
3. Extend flow-analysis state to track blank `final` instance fields for the current class.
4. Detect supported constructor field writes and mark first assignment.
5. Reject reassignment of tracked blank `final` fields.
6. Require all tracked blank `final` fields to be definitely assigned on normal constructor completion.
7. Preserve conservative branch and loop merge behavior for constructor field state.
8. Add colocated tests for valid constructor assignment, missing assignment, and duplicate assignment.
9. Add valid verification fixtures for successful assignment across constructor branches.
10. Add invalid verification fixtures for missing blank `final` field initialization and duplicate assignment.
11. Add or update verification error-message overrides if needed.
12. Run `cargo run -p rajac-verification --bin verification`.
13. Run `./scripts/check-code.sh`.
14. Move the completed plan to `docs/plans/completed`.

# What assumptions matter for this work?

- The flow-analysis stage remains the right place for constructor field-initialization checks.
- The current AST and resolution data are sufficient to recognize assignments to current-class instance fields in the supported subset.
- Conservative handling is acceptable for the first milestone if it prevents unsound acceptance.
- Verification fixtures under `verification/sources` and `verification/sources_invalid/typecheck` are the right compatibility mechanism for this work.

# How will this work be verified?

Verification should cover both focused constructor-flow behavior and end-to-end compiler output.

The expected verification work is:

- add colocated unit tests for constructor field initialization flow
- add valid verification fixtures for successful assignment of blank `final` fields across different constructor branches
- add invalid verification fixtures for missing initialization and duplicate assignment
- run targeted crate tests or `cargo test` for the affected compiler code
- run `cargo run -p rajac-verification --bin verification`
- run `./scripts/check-code.sh`

# What concrete work items are planned?

- [x] Create this plan in `docs/plans`.
- [x] Extend flow-analysis state to track blank `final` instance fields in constructors.
- [x] Detect supported writes to current-class blank `final` fields.
- [x] Reject reassignment of tracked blank `final` fields.
- [x] Require tracked blank `final` fields to be definitely assigned on normal constructor completion.
- [x] Preserve conservative merge behavior for constructor field state across branches and loops.
- [x] Add colocated tests for valid and invalid constructor blank `final` field flow.
- [x] Add valid verification fixtures for successful assignment across constructor branches.
- [x] Add or update invalid verification fixtures under `verification/sources_invalid/typecheck`.
- [x] Add or update verification error-message overrides if needed.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
