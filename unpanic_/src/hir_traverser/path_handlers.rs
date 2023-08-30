//! When we want to check if a call to a function contains panics, what we have id the QPath that
//! point to that function. The QPath can be either local or not. If it is local we can get the
//! function body and check for panics, if it is not we have to save the QPath for later cheks.
use rustc_hir::HirId;
use rustc_hir::{def::DefKind, def::Res, def_id::DefId, Node, QPath};
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;
use rustc_type_ir::sty::TyKind;

use super::traversers::get_panic_in_expr;
use super::ForeignCallsToCheck;

use super::function_handlers::{handle_assoc_fn, handle_fn};

/// If the QPath is not type dependent (we already know which function we want to call) we call
/// handle_solved_path.
/// It the QPath is type dependent we need to solve it in order to know the actual function that we
/// are calling.
pub fn handle_qpath<'tcx>(
    hir_krate: &mut Map<'tcx>,
    qpath: QPath,
    acc: &mut ForeignCallsToCheck,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
) {
    match qpath {
        QPath::Resolved(_, path) => {
            if let Some(last) = path.segments.last() {
                match last.res {
                    Res::Def(def_kind, def_id) => {
                        let fn_ident = last.ident.as_str().to_string();
                        handle_solved_path(
                            hir_krate,
                            def_kind,
                            def_id,
                            fn_ident,
                            acc,
                            tcx,
                            &qpath,
                            call_stack,
                            visited_functions,
                        );
                    }
                    Res::Local(_) => (),
                    _ => panic!("Unexpected Res"),
                }
            }
        }
        QPath::TypeRelative(_, segment) => {
            let result = tcx.typeck(segment.hir_id.owner.def_id);
            let items = result.node_types().items_in_stable_order();
            for item in items {
                if let TyKind::FnDef(def_id, generic_args) = item.1.kind() {
                    // If is a trait fn and is implemented locally check it for panics
                    if let Some(impl_item) = super::get_impl_item(
                        tcx,
                        *def_id,
                        generic_args.get(0).map(|x| x.expect_ty()),
                    ) {
                        handle_assoc_fn(
                            hir_krate,
                            impl_item.def_id,
                            acc,
                            tcx,
                            call_stack,
                            visited_functions,
                            // If we are here the trait item implementation is local so no need to
                            // pass the implementor type for future checks
                            None,
                        )
                    // If is local check if the function contains calls to panic
                    } else if let Some(local_id) = def_id.as_local() {
                        if let Some(Node::Item(item)) = hir_krate.find_by_def_id(local_id) {
                            if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
                                let expr = hir_krate.body(body_id).value;
                                get_panic_in_expr(
                                    hir_krate,
                                    expr,
                                    acc,
                                    tcx,
                                    call_stack,
                                    visited_functions,
                                );
                            }
                        };
                        if let Some(Node::ImplItem(item)) = hir_krate.find_by_def_id(local_id) {
                            if let rustc_hir::ImplItemKind::Fn(_, body_id) = item.kind {
                                let expr = hir_krate.body(body_id).value;
                                get_panic_in_expr(
                                    hir_krate,
                                    expr,
                                    acc,
                                    tcx,
                                    call_stack,
                                    visited_functions,
                                );
                            }
                        };
                    // Otherwise save it for later check
                    } else {
                        let receiver = generic_args.get(0).map(|x| match x.expect_ty().kind() {
                            rustc_middle::ty::Adt(adt_def, _) => adt_def.did(),
                            _ => panic!(),
                        });
                        acc.save_for_later_check(*def_id, tcx, call_stack, receiver);
                    }
                }
            }
        }
        QPath::LangItem(_, _, _) => panic!("Unexpected QPath {:?}", qpath),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn handle_solved_path<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_kind: DefKind,
    def_id: DefId,
    fn_ident: String,
    acc: &mut ForeignCallsToCheck,
    tcx: &mut TyCtxt<'tcx>,
    qpath: &QPath,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
) {
    let function = format!("{} in {:?}", fn_ident, qpath.span());
    call_stack.push(function);
    match def_kind {
        DefKind::Fn => handle_fn(
            hir_krate,
            def_id,
            fn_ident,
            acc,
            tcx,
            call_stack,
            visited_functions,
        ),
        // Ignore contructor they can no panic
        DefKind::Ctor(_, _) => (),
        kind => eprintln!(
            "Unhandled kind {:?}, please open an issue on https://github.com/Fi3/unpanic/issues",
            kind
        ),
    }
}
