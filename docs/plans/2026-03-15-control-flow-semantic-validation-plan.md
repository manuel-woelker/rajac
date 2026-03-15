# What is the plan for implementing semantic validation for control flow structures?

## Why is a dedicated control-flow plan needed?

The attribute analysis stage already validates boolean conditions for `if`, `while`, `do-while`, and `for`, and it already checks `return` statements against the enclosing method type.
However, most control-flow-specific semantic rules are still missing.

Today, `Stmt::Break` and `Stmt::Continue` are accepted without checking whether they target a valid enclosing statement.
`Stmt::Switch` is traversed, but its selector type, case-label compatibility, duplicate labels, and `default` cardinality are not validated.
Labeled statements are also only traversed, so there is no semantic enforcement for labeled `break` and `continue`.

That gap matters because bytecode lowering already supports structured control flow.
If semantic validation stays incomplete, rajac can emit control-flow bytecode for programs that should have been rejected earlier with clear diagnostics.

## What is the current semantic baseline?

The current attribute pass in [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs) already covers:

- boolean-condition checks for `if`, `while`, `do-while`, `for`, and ternary expressions
- scoped traversal for blocks, loop initializers, and catch clauses
- `return` compatibility with the enclosing method or constructor
- `throw` operand validation as a reference-like throwable path

The current pass does not yet cover:

- unlabeled `break` validity outside loops or `switch`
- unlabeled `continue` validity outside loops
- labeled `break` target lookup
- labeled `continue` target lookup restricted to iteration statements
- duplicate label declarations within an enclosing statement chain
- `switch` selector typing rules
- `case` label compatibility with the selector type
- duplicate `case` constants or multiple `default` labels
- basic reachability checks around abrupt completion in control-flow-heavy blocks

## What should semantic validation for control flow own?

The attribute analysis stage should become the canonical owner of statement-level control-flow legality.
For this milestone, that should include:

- target resolution for `break` and `continue`
- label environment management for nested labeled statements
- `switch` selector and label validation
- statement-form restrictions that depend on control-flow kind
- diagnostics for invalid non-local exits
- minimal reachability checks that do not require full definite-assignment analysis

This work should stay in attribute analysis rather than leaking into parsing, resolution, or bytecode generation.
Resolution can keep labels as syntactic identifiers.
Attribute analysis should decide whether a use is legal in context.

## What validation steps should be implemented first?

The implementation should be staged so each step introduces one new semantic context and immediately uses it.

1. Add a control-flow context stack that records whether the current statement nesting permits `break`, `continue`, or both.
2. Add a label environment that records active labeled statements and the statement kind each label wraps.
3. Validate unlabeled `break` and `continue` against the active control-flow context.
4. Validate labeled `break` and `continue` by resolving the target label and checking target kind restrictions.
5. Validate `switch` selector types and analyze each case label against the selector type.
6. Reject duplicate `case` constants and multiple `default` labels within the same `switch`.
7. Add a minimal abrupt-completion model so obviously unreachable statements after unconditional `break`, `continue`, or `return` can be diagnosed where the AST structure makes that reliable.

## How should loop and branch exits be validated?

Loop-related validation should be driven by an explicit semantic context, not by ad hoc parent inspection.

The control-flow context should distinguish at least:

- iteration contexts that allow both `break` and `continue`
- `switch` contexts that allow `break` but not unlabeled `continue`
- plain blocks that allow neither

The first diagnostics should cover:

- `break` outside loop or `switch`
- `continue` outside loop
- `break label;` where `label` does not exist
- `continue label;` where `label` does not exist
- `continue label;` where `label` names a non-iteration statement

The context model should also work for nested `switch` inside loops and loops inside `switch`, because those combinations determine whether an unlabeled `break` or `continue` is legal.

## How should labeled statements be modeled?

Labeled statements should push a semantic label entry before analyzing the wrapped statement and pop it afterward.
Each entry should record:

- the label name
- the wrapped statement kind
- whether the wrapped statement is an iteration statement
- marker information for diagnostics

That should allow:

- `break label;` to target any enclosing labeled statement
- `continue label;` to target only enclosing labeled iteration statements
- useful diagnostics that point at both the jump site and, when helpful, the label declaration

The implementation should treat nested labels with the same name as invalid in the same active label chain rather than silently shadowing them.
If Java behavior needs a narrower rule after verification, the diagnostics can be adjusted, but the plan should start with explicit duplicate-label handling instead of leaving the behavior implicit.

## How should `switch` statements be validated?

The first `switch` validation milestone should focus on classic statement `switch`, matching the AST that already exists.

The attribute pass should:

- type the selector expression once and classify its semantic kind
- accept selector types that the current compiler intends to support in this milestone
- reject selector types that are not yet supported with a clear diagnostic
- evaluate each `case` label expression and check that it is assignment-compatible with the selector type or otherwise follows the supported `switch` rules
- require case labels to be compile-time constants in the forms supported by the compiler
- reject duplicate constant case values within one `switch`
- reject more than one `default` label

Given the current implementation maturity, the first supported selector set should likely be:

- integral primitive types
- `char`
- enums if resolution already exposes enough information to compare case constants safely

String-switch validation can be deferred unless the compiler already has the constant and equality machinery needed to make it robust.

## What reachability checks belong in this plan?

This plan should add only a narrow reachability layer.
It should not attempt full Java definite-assignment or all JLS reachability rules in one step.

The first reachability pass should detect only straightforward cases such as:

- statements that appear after `return` in the same block
- statements that appear after unconditional `break` or `continue` in the same block segment
- statements inside a `switch` case body after an unconditional abrupt-completion statement, until the next structural boundary

This should be implemented with a small statement-result enum or similar helper that reports whether a statement can complete normally.
That helper can later support richer flow-sensitive analysis without forcing a redesign.

## What architecture changes should accompany the work?

This work should keep [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs) from becoming more monolithic.
If the control-flow logic grows, it should be split into dedicated helper types or sibling modules under the attribute-analysis stage.

The implementation should introduce:

- a control-flow context type for `break` and `continue` legality
- a label environment entry type
- a small statement-outcome model for abrupt completion
- shared semantic diagnostic helpers for control-flow-specific errors

Any new structs should use `SharedString` for stored string fields, consistent with repository guidance.

## What is the recommended implementation order?

1. Introduce control-flow context and label-environment data structures.
2. Add semantic diagnostics for invalid `break`, `continue`, and missing labels.
3. Validate unlabeled `break` and `continue`.
4. Validate labeled statements plus labeled `break` and `continue`.
5. Add `switch` selector validation.
6. Add `case` constant validation, duplicate detection, and `default` validation.
7. Add a minimal statement-outcome model and diagnose straightforward unreachable statements.
8. Expand colocated tests and verification fixtures for the new diagnostics.
9. Run verification and repository-wide checks.

## What tests and verification fixtures should be added?

Tests should be colocated with the attribute-analysis implementation and should prefer compact, data-driven examples where that keeps the coverage readable.

The first colocated test set should include:

- `break` outside loop or `switch`
- `continue` outside loop
- labeled `break` to an enclosing label
- labeled `continue` to an enclosing loop label
- invalid labeled `continue` to a non-loop label
- missing-label diagnostics for `break` and `continue`
- `switch` on a supported selector type
- `switch` on an unsupported selector type
- duplicate `case` labels
- multiple `default` labels
- unreachable statement after `return`
- unreachable statement after unconditional `break` or `continue`

Verification fixtures should also be added under `verification/sources_invalid/typecheck` for diagnostics that should remain compatible with OpenJDK by line number.
If rajac produces clearer wording than OpenJDK, add overrides in [verification_main.rs](/data/projects/rajac/crates/verification/src/verification_main.rs) while preserving the OpenJDK line anchor.

## What assumptions and follow-up boundaries should stay explicit?

This plan assumes the current AST shape for statement `switch` remains stable during implementation.
If switch expressions are introduced later, they should get a separate semantic milestone rather than being folded into this one midstream.

This plan also assumes:

- full definite-assignment analysis remains out of scope
- try/finally transfer rules that affect reachability can stay conservative for now
- pattern matching in `switch` is out of scope
- string-switch semantics can be deferred if constant support is not ready
- enum-switch validation depends on how reliably enum constants are resolved today

If any of those assumptions change during implementation, the plan should be updated rather than silently expanded.

## What completion criteria should define success?

This control-flow semantic-validation milestone should be considered complete when:

- invalid `break` and `continue` statements are rejected in attribute analysis
- labeled control transfer is validated against real enclosing targets
- `switch` statements are checked for selector and label legality within the supported subset
- obvious unreachable statements after unconditional abrupt completion are diagnosed
- colocated tests cover both valid and invalid control-flow semantics
- invalid verification fixtures cover the new diagnostics where OpenJDK comparison is practical
- `cargo run -p verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [ ] Add a control-flow context stack to attribute analysis.
- [ ] Add a label environment for active labeled statements.
- [ ] Add semantic diagnostic helpers for invalid `break`, `continue`, and missing labels.
- [ ] Reject unlabeled `break` outside loop or `switch`.
- [ ] Reject unlabeled `continue` outside loops.
- [ ] Resolve labeled `break` targets.
- [ ] Resolve labeled `continue` targets and reject non-loop targets.
- [ ] Detect duplicate active labels where the language subset requires rejection.
- [ ] Validate `switch` selector types.
- [ ] Validate `case` labels against the selector type and supported constant-expression rules.
- [ ] Reject duplicate `case` constants.
- [ ] Reject multiple `default` labels.
- [ ] Add a minimal statement-outcome model for abrupt completion.
- [ ] Diagnose straightforward unreachable statements after unconditional abrupt completion.
- [ ] Add colocated tests for control-flow semantic diagnostics.
- [ ] Add or update invalid verification fixtures and any error-message overrides.
- [ ] Run `cargo run -p verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
