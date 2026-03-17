//! # Flow Analysis Stage
//!
//! This module performs path-sensitive semantic checks after attribute analysis
//! and before bytecode generation.
//!
//! ## What does this stage own?
//!
//! The first flow-analysis milestone focuses on definite assignment for locals
//! and parameters. It uses a conservative control-flow model so the compiler can
//! reject reads of maybe-uninitialized locals without relying on backend
//! behavior or spreading flow checks across earlier semantic passes.

/* 📖 # Why keep definite-assignment checking in a dedicated stage?
Attribute analysis already owns typing and statement legality. Definite
assignment is path-sensitive and grows into a separate body of logic for joins,
loops, and abrupt completion. Keeping that work in its own stage preserves the
 documented pipeline and avoids making attribute analysis even more monolithic.
*/

use crate::CompilationUnit;
use rajac_ast::{
    Ast, AstArena, ClassDeclId, ClassMember, ClassMemberId, Expr, ExprId, ForInit, Method, Param,
    ParamId, Stmt, StmtId, SwitchCase, SwitchLabel,
};
use rajac_base::file_path::FilePath;
use rajac_base::logging::instrument;
use rajac_base::shared_string::SharedString;
use rajac_diagnostics::{Annotation, Diagnostic, Diagnostics, Severity, SourceChunk, Span};
use std::collections::HashMap;

/// Performs flow analysis on compilation units and returns the diagnostics
/// produced by the stage.
#[instrument(
    name = "compiler.phase.flow_analysis",
    skip(compilation_units),
    fields(compilation_units = compilation_units.len())
)]
pub fn analyze_flows(compilation_units: &mut [CompilationUnit]) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();

    for compilation_unit in compilation_units {
        analyze_compilation_unit(compilation_unit, &mut diagnostics);
    }

    diagnostics
}

fn analyze_compilation_unit(compilation_unit: &mut CompilationUnit, diagnostics: &mut Diagnostics) {
    let mut analyzer = FlowAnalyzer::new(
        &compilation_unit.source_file,
        compilation_unit.ast.source.as_str(),
        &compilation_unit.arena,
    );
    analyzer.analyze_ast(&compilation_unit.ast);

    let flow_diagnostics = analyzer.finish();
    compilation_unit
        .diagnostics
        .extend(flow_diagnostics.iter().cloned());
    diagnostics.extend(flow_diagnostics);
}

struct FlowAnalyzer<'a> {
    source_file: &'a FilePath,
    source: &'a str,
    arena: &'a AstArena,
    diagnostics: Diagnostics,
    ident_occurrences: HashMap<SharedString, usize>,
}

#[derive(Clone, Debug, Default)]
struct FlowState {
    scopes: Vec<HashMap<SharedString, LocalState>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct LocalState {
    definitely_assigned: bool,
}

#[derive(Clone, Debug)]
struct FlowOutcome {
    state: FlowState,
    completes_normally: bool,
}

impl FlowOutcome {
    fn normal(state: FlowState) -> Self {
        Self {
            state,
            completes_normally: true,
        }
    }

    fn abrupt(state: FlowState) -> Self {
        Self {
            state,
            completes_normally: false,
        }
    }
}

impl FlowState {
    fn new() -> Self {
        Self::default()
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_local(&mut self, name: SharedString, definitely_assigned: bool) {
        let scope = self
            .scopes
            .last_mut()
            .expect("flow-analysis scope stack must not be empty");
        scope.insert(
            name,
            LocalState {
                definitely_assigned,
            },
        );
    }

    fn lookup_local(&self, name: &SharedString) -> Option<LocalState> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn assign_local(&mut self, name: &SharedString) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(local) = scope.get_mut(name) {
                local.definitely_assigned = true;
                return true;
            }
        }
        false
    }

    fn intersect(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for (merged_scope, other_scope) in merged.scopes.iter_mut().zip(&other.scopes) {
            for (name, state) in merged_scope.iter_mut() {
                state.definitely_assigned &= other_scope
                    .get(name)
                    .is_some_and(|other_state| other_state.definitely_assigned);
            }
        }
        merged
    }
}

impl<'a> FlowAnalyzer<'a> {
    fn new(source_file: &'a FilePath, source: &'a str, arena: &'a AstArena) -> Self {
        Self {
            source_file,
            source,
            arena,
            diagnostics: Diagnostics::new(),
            ident_occurrences: HashMap::new(),
        }
    }

    fn finish(self) -> Diagnostics {
        self.diagnostics
    }

    fn analyze_ast(&mut self, ast: &Ast) {
        for class_id in &ast.classes {
            self.analyze_class_decl(*class_id);
        }
    }

    fn analyze_class_decl(&mut self, class_id: ClassDeclId) {
        let class = self.arena.class_decl(class_id).clone();

        for entry in class.enum_entries {
            for member_id in entry.body.unwrap_or_default() {
                self.analyze_class_member(member_id);
            }
        }

        for member_id in class.members {
            self.analyze_class_member(member_id);
        }
    }

    fn analyze_class_member(&mut self, member_id: ClassMemberId) {
        match self.arena.class_member(member_id).clone() {
            ClassMember::Method(method) => self.analyze_method(&method),
            ClassMember::Constructor(constructor) => {
                self.analyze_callable_body(constructor.body, &constructor.params)
            }
            ClassMember::StaticBlock(stmt_id) => {
                let mut state = FlowState::new();
                state.push_scope();
                let _ = self.analyze_stmt(stmt_id, state);
            }
            ClassMember::NestedClass(class_id)
            | ClassMember::NestedInterface(class_id)
            | ClassMember::NestedRecord(class_id)
            | ClassMember::NestedAnnotation(class_id)
            | ClassMember::NestedEnum(class_id) => self.analyze_class_decl(class_id),
            ClassMember::Field(_) => {}
        }
    }

    fn analyze_method(&mut self, method: &Method) {
        self.analyze_callable_body(method.body, &method.params);
    }

    fn analyze_callable_body(&mut self, body: Option<StmtId>, params: &[ParamId]) {
        let Some(body) = body else {
            return;
        };

        let mut state = FlowState::new();
        state.push_scope();
        for param_id in params {
            let Param { name, .. } = self.arena.param(*param_id).clone();
            self.note_identifier_occurrence(&name.name);
            state.declare_local(name.name, true);
        }
        let _ = self.analyze_stmt(body, state);
    }

    fn analyze_stmt(&mut self, stmt_id: StmtId, state: FlowState) -> FlowOutcome {
        match self.arena.stmt(stmt_id).clone() {
            Stmt::Empty => FlowOutcome::normal(state),
            Stmt::Block(statements) => self.analyze_block(statements, state),
            Stmt::Expr(expr_id) => FlowOutcome::normal(self.analyze_expr(expr_id, state)),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => self.analyze_if_stmt(condition, then_branch, else_branch, state),
            Stmt::While { condition, body } => self.analyze_while_stmt(condition, body, state),
            Stmt::DoWhile { body, condition } => self.analyze_do_while_stmt(body, condition, state),
            Stmt::For {
                init,
                condition,
                update,
                body,
            } => self.analyze_for_stmt(init, condition, update, body, state),
            Stmt::Switch { expr, cases } => self.analyze_switch_stmt(expr, cases, state),
            Stmt::Return(expr) => {
                let state = if let Some(expr_id) = expr {
                    self.analyze_expr(expr_id, state)
                } else {
                    state
                };
                FlowOutcome::abrupt(state)
            }
            Stmt::Break(_) | Stmt::Continue(_) => FlowOutcome::abrupt(state),
            Stmt::Label(_, stmt_id) => self.analyze_stmt(stmt_id, state),
            Stmt::Try {
                try_block,
                catches,
                finally_block,
            } => self.analyze_try_stmt(try_block, catches, finally_block, state),
            Stmt::Throw(expr_id) => FlowOutcome::abrupt(self.analyze_expr(expr_id, state)),
            Stmt::Synchronized { expr, block } => {
                let state = if let Some(expr_id) = expr {
                    self.analyze_expr(expr_id, state)
                } else {
                    state
                };
                self.analyze_stmt(block, state)
            }
            Stmt::LocalVar {
                name, initializer, ..
            } => self.analyze_local_var_stmt(name.name, initializer, state),
        }
    }

    fn analyze_block(&mut self, statements: Vec<StmtId>, mut state: FlowState) -> FlowOutcome {
        state.push_scope();
        let mut outcome = FlowOutcome::normal(state);
        for stmt_id in statements {
            if !outcome.completes_normally {
                break;
            }
            outcome = self.analyze_stmt(stmt_id, outcome.state);
        }
        outcome.state.pop_scope();
        outcome
    }

    fn analyze_if_stmt(
        &mut self,
        condition: ExprId,
        then_branch: StmtId,
        else_branch: Option<StmtId>,
        state: FlowState,
    ) -> FlowOutcome {
        let state = self.analyze_expr(condition, state);
        let then_outcome = self.analyze_stmt(then_branch, state.clone());
        let else_outcome = if let Some(else_branch) = else_branch {
            self.analyze_stmt(else_branch, state)
        } else {
            FlowOutcome::normal(state)
        };
        self.merge_branch_outcomes(then_outcome, else_outcome)
    }

    fn analyze_while_stmt(
        &mut self,
        condition: ExprId,
        body: StmtId,
        state: FlowState,
    ) -> FlowOutcome {
        let state = self.analyze_expr(condition, state);
        let _ = self.analyze_stmt(body, state.clone());
        FlowOutcome::normal(state)
    }

    fn analyze_do_while_stmt(
        &mut self,
        body: StmtId,
        condition: ExprId,
        state: FlowState,
    ) -> FlowOutcome {
        let body_outcome = self.analyze_stmt(body, state.clone());
        let condition_state = if body_outcome.completes_normally {
            body_outcome.state.clone()
        } else {
            state.clone()
        };
        let _ = self.analyze_expr(condition, condition_state);
        FlowOutcome::normal(state)
    }

    fn analyze_for_stmt(
        &mut self,
        init: Option<ForInit>,
        condition: Option<ExprId>,
        update: Option<ExprId>,
        body: StmtId,
        mut state: FlowState,
    ) -> FlowOutcome {
        let has_local_init = matches!(init, Some(ForInit::LocalVar { .. }));
        if has_local_init {
            state.push_scope();
        }

        let mut state = match init {
            Some(ForInit::Expr(expr_id)) => self.analyze_expr(expr_id, state),
            Some(ForInit::LocalVar {
                name, initializer, ..
            }) => self.analyze_local_var(name.name, initializer, state),
            None => state,
        };

        if let Some(condition) = condition {
            state = self.analyze_expr(condition, state);
        }

        let body_outcome = self.analyze_stmt(body, state.clone());
        if body_outcome.completes_normally
            && let Some(update) = update
        {
            let _ = self.analyze_expr(update, body_outcome.state);
        }

        if has_local_init {
            state.pop_scope();
        }
        FlowOutcome::normal(state)
    }

    fn analyze_switch_stmt(
        &mut self,
        expr: ExprId,
        cases: Vec<SwitchCase>,
        state: FlowState,
    ) -> FlowOutcome {
        let selector_state = self.analyze_expr(expr, state);
        let has_default = cases.iter().any(|case| {
            case.labels
                .iter()
                .any(|label| matches!(label, SwitchLabel::Default))
        });

        let mut normal_exit_state = if has_default {
            None
        } else {
            Some(selector_state.clone())
        };

        for case in cases {
            let mut case_state = selector_state.clone();
            for label in case.labels {
                if let SwitchLabel::Case(expr_id) = label {
                    case_state = self.analyze_expr(expr_id, case_state);
                }
            }
            case_state.push_scope();
            let mut case_outcome = FlowOutcome::normal(case_state);
            for stmt_id in case.body {
                if !case_outcome.completes_normally {
                    break;
                }
                case_outcome = self.analyze_stmt(stmt_id, case_outcome.state);
            }
            case_outcome.state.pop_scope();

            if case_outcome.completes_normally {
                normal_exit_state = Some(match normal_exit_state {
                    Some(existing) => existing.intersect(&case_outcome.state),
                    None => case_outcome.state,
                });
            }
        }

        if let Some(state) = normal_exit_state {
            FlowOutcome::normal(state)
        } else {
            FlowOutcome::abrupt(selector_state)
        }
    }

    fn analyze_try_stmt(
        &mut self,
        try_block: StmtId,
        catches: Vec<rajac_ast::CatchClause>,
        finally_block: Option<StmtId>,
        state: FlowState,
    ) -> FlowOutcome {
        let try_outcome = self.analyze_stmt(try_block, state.clone());
        let mut all_path_state = try_outcome.state.clone();
        let mut normal_exit_state = if try_outcome.completes_normally {
            Some(try_outcome.state.clone())
        } else {
            None
        };

        for catch_clause in catches {
            let mut catch_state = state.clone();
            catch_state.push_scope();
            let Param { name, .. } = self.arena.param(catch_clause.param).clone();
            self.note_identifier_occurrence(&name.name);
            catch_state.declare_local(name.name, true);
            let mut catch_outcome = self.analyze_stmt(catch_clause.body, catch_state);
            catch_outcome.state.pop_scope();

            all_path_state = all_path_state.intersect(&catch_outcome.state);
            if catch_outcome.completes_normally {
                normal_exit_state = Some(match normal_exit_state {
                    Some(existing) => existing.intersect(&catch_outcome.state),
                    None => catch_outcome.state,
                });
            }
        }

        let Some(finally_block) = finally_block else {
            return if let Some(state) = normal_exit_state {
                FlowOutcome::normal(state)
            } else {
                FlowOutcome::abrupt(all_path_state)
            };
        };

        let finally_outcome = self.analyze_stmt(finally_block, all_path_state);
        if !finally_outcome.completes_normally {
            return FlowOutcome::abrupt(finally_outcome.state);
        }

        if normal_exit_state.is_some() {
            FlowOutcome::normal(finally_outcome.state)
        } else {
            FlowOutcome::abrupt(finally_outcome.state)
        }
    }

    fn analyze_local_var_stmt(
        &mut self,
        name: SharedString,
        initializer: Option<ExprId>,
        state: FlowState,
    ) -> FlowOutcome {
        FlowOutcome::normal(self.analyze_local_var(name, initializer, state))
    }

    fn analyze_local_var(
        &mut self,
        name: SharedString,
        initializer: Option<ExprId>,
        mut state: FlowState,
    ) -> FlowState {
        self.note_identifier_occurrence(&name);
        state = if let Some(initializer) = initializer {
            self.analyze_expr(initializer, state)
        } else {
            state
        };
        state.declare_local(name, initializer.is_some());
        state
    }

    fn analyze_expr(&mut self, expr_id: ExprId, state: FlowState) -> FlowState {
        match self.arena.expr(expr_id).clone() {
            Expr::Error | Expr::Literal(_) | Expr::Super | Expr::This(None) => state,
            Expr::Ident(name) => {
                self.note_identifier_occurrence(&name.name);
                if state
                    .lookup_local(&name.name)
                    .is_some_and(|local| !local.definitely_assigned)
                {
                    self.emit_uninitialized_local(&name.name);
                }
                state
            }
            Expr::Unary { op, expr } => {
                let mut state = self.analyze_expr(expr, state);
                if matches!(
                    op,
                    rajac_ast::UnaryOp::Increment | rajac_ast::UnaryOp::Decrement
                ) && let Some(name) = self.local_ident_name(expr)
                {
                    state.assign_local(&name);
                }
                state
            }
            Expr::Binary { lhs, rhs, .. } => {
                let state = self.analyze_expr(lhs, state);
                self.analyze_expr(rhs, state)
            }
            Expr::Assign { op, lhs, rhs } => self.analyze_assign_expr(op, lhs, rhs, state),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let state = self.analyze_expr(condition, state);
                let then_state = self.analyze_expr(then_expr, state.clone());
                let else_state = self.analyze_expr(else_expr, state);
                then_state.intersect(&else_state)
            }
            Expr::Cast { expr, .. } | Expr::InstanceOf { expr, .. } => {
                self.analyze_expr(expr, state)
            }
            Expr::FieldAccess { expr, .. } => self.analyze_expr(expr, state),
            Expr::MethodCall { expr, args, .. } => {
                let mut state = if let Some(expr_id) = expr {
                    self.analyze_expr(expr_id, state)
                } else {
                    state
                };
                for arg in args {
                    state = self.analyze_expr(arg, state);
                }
                state
            }
            Expr::New { args, .. } => {
                let mut state = state;
                for arg in args {
                    state = self.analyze_expr(arg, state);
                }
                state
            }
            Expr::NewArray {
                dimensions,
                initializer,
                ..
            } => {
                let mut state = state;
                for dimension in dimensions {
                    state = self.analyze_expr(dimension, state);
                }
                if let Some(initializer) = initializer {
                    state = self.analyze_expr(initializer, state);
                }
                state
            }
            Expr::ArrayInitializer { elements } => {
                let mut state = state;
                for element in elements {
                    state = self.analyze_expr(element, state);
                }
                state
            }
            Expr::ArrayAccess { array, index } => {
                let state = self.analyze_expr(array, state);
                self.analyze_expr(index, state)
            }
            Expr::ArrayLength { array } | Expr::This(Some(array)) => {
                self.analyze_expr(array, state)
            }
            Expr::SuperCall { args, .. } => {
                let mut state = state;
                for arg in args {
                    state = self.analyze_expr(arg, state);
                }
                state
            }
        }
    }

    fn analyze_assign_expr(
        &mut self,
        op: rajac_ast::AssignOp,
        lhs: ExprId,
        rhs: ExprId,
        state: FlowState,
    ) -> FlowState {
        if matches!(op, rajac_ast::AssignOp::Eq)
            && let Some(name) = self.local_ident_name(lhs)
        {
            self.note_identifier_occurrence(&name);
            let mut state = self.analyze_expr(rhs, state);
            state.assign_local(&name);
            return state;
        }

        let mut state = self.analyze_expr(lhs, state);
        state = self.analyze_expr(rhs, state);
        if let Some(name) = self.local_ident_name(lhs) {
            state.assign_local(&name);
        }
        state
    }

    fn merge_branch_outcomes(
        &self,
        then_outcome: FlowOutcome,
        else_outcome: FlowOutcome,
    ) -> FlowOutcome {
        match (
            then_outcome.completes_normally,
            else_outcome.completes_normally,
        ) {
            (true, true) => FlowOutcome::normal(then_outcome.state.intersect(&else_outcome.state)),
            (true, false) => FlowOutcome::normal(then_outcome.state),
            (false, true) => FlowOutcome::normal(else_outcome.state),
            (false, false) => {
                FlowOutcome::abrupt(then_outcome.state.intersect(&else_outcome.state))
            }
        }
    }

    fn local_ident_name(&self, expr_id: ExprId) -> Option<SharedString> {
        match self.arena.expr(expr_id) {
            Expr::Ident(name) => Some(name.name.clone()),
            _ => None,
        }
    }

    fn note_identifier_occurrence(&mut self, name: &SharedString) -> usize {
        let occurrence = self.ident_occurrences.entry(name.clone()).or_insert(0);
        *occurrence += 1;
        *occurrence
    }

    fn emit_uninitialized_local(&mut self, name: &SharedString) {
        let occurrence = self.ident_occurrences.get(name).copied().unwrap_or(1);
        let message = format!("variable {} might not have been initialized", name.as_str());
        let chunk = source_chunk_for_marker_occurrence(
            self.source_file,
            self.source,
            name.as_str(),
            occurrence,
        );
        self.diagnostics.add(Diagnostic {
            severity: Severity::Error,
            message: SharedString::new(&message),
            chunks: vec![chunk],
        });
    }
}

fn source_chunk_for_marker_occurrence(
    source_file: &FilePath,
    source: &str,
    marker: &str,
    occurrence: usize,
) -> SourceChunk {
    let offset = marker_offset(source, marker, occurrence).unwrap_or(0);
    let (line, line_start, line_end) = line_bounds_for_offset(source, offset);
    let fragment = &source[line_start..line_end];
    let annotation_start = fragment.find(marker).unwrap_or(0);
    let annotation_end = annotation_start + marker.len().max(1);

    SourceChunk {
        path: source_file.clone(),
        fragment: SharedString::new(fragment),
        offset: line_start,
        line,
        annotations: vec![Annotation {
            span: Span(annotation_start..annotation_end),
            message: SharedString::new(""),
        }],
    }
}

fn marker_offset(source: &str, marker: &str, occurrence: usize) -> Option<usize> {
    if marker.is_empty() || occurrence == 0 {
        return None;
    }

    let mut search_start = 0;
    for current_occurrence in 1..=occurrence {
        let relative = source[search_start..].find(marker)?;
        let found_at = search_start + relative;
        if current_occurrence == occurrence {
            return Some(found_at);
        }
        search_start = found_at + marker.len();
    }

    None
}

fn line_bounds_for_offset(source: &str, offset: usize) -> (usize, usize, usize) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    let line = source[..line_start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    (line, line_start, line_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::{attribute_analysis, collection, resolution};
    use rajac_base::file_path::FilePath;
    use rajac_diagnostics::Diagnostics;
    use rajac_lexer::Lexer;
    use rajac_parser::Parser;

    #[test]
    fn reports_uninitialized_local_read() {
        let source = r#"
class Example {
    int run() {
        int value;
        return value;
    }
}
"#;

        let diagnostics = analyze_source(source);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .as_str()
                .contains("variable value might not have been initialized")
        }));
    }

    #[test]
    fn allows_reads_after_assignment_in_both_if_branches() {
        let source = r#"
class Example {
    int run(boolean flag) {
        int value;
        if (flag) {
            value = 1;
        } else {
            value = 2;
        }
        return value;
    }
}
"#;

        let diagnostics = analyze_source(source);
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .as_str()
                .contains("might not have been initialized")
        }));
    }

    #[test]
    fn reports_uninitialized_local_after_partial_if_assignment() {
        let source = r#"
class Example {
    int run(boolean flag) {
        int value;
        if (flag) {
            value = 1;
        }
        return value;
    }
}
"#;

        let diagnostics = analyze_source(source);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .as_str()
                .contains("variable value might not have been initialized")
        }));
    }

    #[test]
    fn reports_uninitialized_local_after_loop_assignment() {
        let source = r#"
class Example {
    int run(boolean flag) {
        int value;
        while (flag) {
            value = 1;
        }
        return value;
    }
}
"#;

        let diagnostics = analyze_source(source);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .as_str()
                .contains("variable value might not have been initialized")
        }));
    }

    #[test]
    fn treats_parameters_as_definitely_assigned() {
        let source = r#"
class Example {
    int run(int value) {
        return value;
    }
}
"#;

        let diagnostics = analyze_source(source);
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .as_str()
                .contains("might not have been initialized")
        }));
    }

    fn analyze_source(source: &str) -> Diagnostics {
        let parse_result = Parser::new(Lexer::new(source, FilePath::new("Test.java")), source)
            .parse_compilation_unit();
        let mut units = vec![CompilationUnit {
            source_file: FilePath::new("Test.java"),
            ast: parse_result.ast,
            arena: parse_result.arena,
            diagnostics: Diagnostics::new(),
        }];
        let mut symbol_table = rajac_symbols::SymbolTable::new();
        collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
            .expect("collect symbols");
        resolution::resolve_identifiers(&mut units, &mut symbol_table);
        let _ = attribute_analysis::analyze_attributes(&mut units, &mut symbol_table);
        analyze_flows(&mut units)
    }
}
