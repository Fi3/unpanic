//! This is the core of the hir traverser. It recursivley check an Node (either a Block an Expr or
//! a Stmt) for call to function that contains panics.

use crate::utils::log_allow_panic;
use rustc_hir::HirId;
use rustc_hir::{Block, Expr, ExprKind, Guard, StmtKind};
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;

use super::function_handlers::handle_assoc_fn;
use super::path_handlers::handle_qpath;
use super::ForeignCallsToCheck;

/// Check each statement and expression in the block for call to functions that can panics.
pub fn get_panic_in_block<'tcx>(
    hir_krate: &mut Map<'tcx>,
    block: &Block<'tcx>,
    acc: &mut ForeignCallsToCheck,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
) {
    for stmt in block.stmts {
        get_panic_in_stmt(
            hir_krate,
            &stmt.kind,
            acc,
            tcx,
            call_stack,
            visited_functions,
        );
    }
    if let Some(expr) = block.expr {
        get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
    }
}

/// Check each statement and expression in the statement for call to functions that can panics.
pub fn get_panic_in_stmt<'tcx>(
    hir_krate: &mut Map<'tcx>,
    stmt: &StmtKind<'tcx>,
    acc: &mut ForeignCallsToCheck,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
) {
    match stmt {
        StmtKind::Local(local) => {
            if let Some(expr) = local.init {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
            }
            if let Some(block) = local.els {
                for stmt in block.stmts {
                    get_panic_in_stmt(
                        hir_krate,
                        &stmt.kind,
                        acc,
                        tcx,
                        call_stack,
                        visited_functions,
                    );
                }
                if let Some(expr) = block.expr {
                    get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
                }
            }
        }
        StmtKind::Item(_) => (),
        StmtKind::Expr(expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions)
        }
        StmtKind::Semi(expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions)
        }
    }
}

// Recursivley check an expression for call to functions that can panics.
// 1. if the expression cointain other expressions call get_panic_in_expr
// 2. if the expression is a associated function call, call handle_assoc_fn
// 3. if the expression is a function is a path call handle_qpath
// 4. if the expression is a block
//     * if the block is not labelled 'allow_panic call get_panic_in_block
//     * if it log it an continue
// 5. if the expression is a loop call get_panic_in_block and ignore labels
pub fn get_panic_in_expr<'tcx>(
    hir_krate: &mut Map<'tcx>,
    expr_: &Expr<'tcx>,
    acc: &mut ForeignCallsToCheck,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
) {
    match expr_.kind {
        ExprKind::ConstBlock(const_block) => {
            let expr = hir_krate.body(const_block.body).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Array(array) => {
            for expr in array {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
            }
        }
        ExprKind::Call(call, args) => {
            let hir_id = call.hir_id;
            if !visited_functions.contains(&hir_id) {
                visited_functions.push(hir_id);
                get_panic_in_expr(hir_krate, call, acc, tcx, call_stack, visited_functions);
                for expr in args {
                    get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
                }
            }
        }
        ExprKind::MethodCall(method, receiver, args, _span) => {
            let hir_id = method.hir_id;
            if !visited_functions.contains(&hir_id) {
                visited_functions.push(hir_id);
                let result = tcx.typeck(receiver.hir_id.owner.def_id);
                let ty = result.expr_ty(receiver);
                let method_def_id = match ty.kind() {
                    rustc_middle::ty::Adt(adt_def, _) => adt_def.did(),
                    _ => panic!(),
                };
                let def_id = result
                    .type_dependent_def_id(expr_.hir_id)
                    .expect("ERROR: Can not get def id");
                handle_assoc_fn(
                    hir_krate,
                    def_id,
                    acc,
                    tcx,
                    call_stack,
                    visited_functions,
                    Some(method_def_id),
                );

                for expr in args {
                    get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
                }
            }
        }
        ExprKind::Tup(tup) => {
            for expr in tup {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
            }
        }
        ExprKind::Binary(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Unary(_, arg) => {
            get_panic_in_expr(hir_krate, arg, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Lit(_) => (),
        ExprKind::Cast(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions)
        }
        ExprKind::Type(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::DropTemps(expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Let(let_) => {
            get_panic_in_expr(
                hir_krate,
                let_.init,
                acc,
                tcx,
                call_stack,
                visited_functions,
            );
        }
        ExprKind::If(cond, if_block, Some(else_block)) => {
            get_panic_in_expr(hir_krate, cond, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, if_block, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(
                hir_krate,
                else_block,
                acc,
                tcx,
                call_stack,
                visited_functions,
            );
        }
        ExprKind::If(cond, if_block, None) => {
            get_panic_in_expr(hir_krate, cond, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, if_block, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Loop(block, _, _, _) => {
            get_panic_in_block(hir_krate, block, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Match(expr, arms, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
            for arm in arms {
                match arm.guard {
                    Some(Guard::If(expr)) => {
                        get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions)
                    }
                    Some(Guard::IfLet(let_)) => get_panic_in_expr(
                        hir_krate,
                        let_.init,
                        acc,
                        tcx,
                        call_stack,
                        visited_functions,
                    ),
                    None => (),
                };
                get_panic_in_expr(hir_krate, arm.body, acc, tcx, call_stack, visited_functions);
            }
        }
        ExprKind::Closure(closure) => {
            let expr = hir_krate.body(closure.body).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Block(block, Some(label)) => {
            if !label.ident.as_str().contains("allow_panic") {
                get_panic_in_block(hir_krate, block, acc, tcx, call_stack, visited_functions);
            } else {
                log_allow_panic(call_stack);
            }
        }
        ExprKind::Block(block, None) => {
            get_panic_in_block(hir_krate, block, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Assign(arg1, arg2, _) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::AssignOp(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Field(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Index(arg1, arg2,_) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Path(path) => {
            handle_qpath(hir_krate, path, acc, tcx, call_stack, visited_functions)
        }
        ExprKind::AddrOf(_, _, expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Break(_, Some(expr)) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Break(_, None) => (),
        ExprKind::Continue(_) => (),
        ExprKind::Ret(Some(expr)) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Ret(None) => (),
        ExprKind::InlineAsm(_) => (),
        ExprKind::OffsetOf(_, _) => (),
        ExprKind::Struct(_, fields, Some(base)) => {
            get_panic_in_expr(hir_krate, base, acc, tcx, call_stack, visited_functions);
            for field in fields {
                get_panic_in_expr(
                    hir_krate,
                    field.expr,
                    acc,
                    tcx,
                    call_stack,
                    visited_functions,
                );
            }
        }
        ExprKind::Struct(_, fields, None) => {
            for field in fields {
                get_panic_in_expr(
                    hir_krate,
                    field.expr,
                    acc,
                    tcx,
                    call_stack,
                    visited_functions,
                );
            }
        }
        ExprKind::Repeat(elem, _) => {
            get_panic_in_expr(hir_krate, elem, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Yield(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Become(_) => todo!(),
        ExprKind::Err(_) => panic!(),
    }
}
