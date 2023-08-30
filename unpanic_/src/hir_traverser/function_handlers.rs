//! When we have the def_id of an actual function we can check it, this is kind of the final phase
//! of the checker, of course if the function is not very simple it will call itself recursively
//! via the traverser.
use rustc_hir::HirId;
use rustc_hir::{def_id::DefId, Node};
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;

use super::traversers::get_panic_in_expr;
use super::ForeignCallsToCheck;
use crate::utils::log_panic_in_deny_block;

/// If is local check it now
/// If is not save for later
/// If is a panic emit an error
pub fn handle_fn<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_id: DefId,
    fn_ident: String,
    acc: &mut ForeignCallsToCheck,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
) {
    if let Some(local_id) = def_id.as_local() {
        let item = hir_krate.expect_item(local_id);
        if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
            let expr = hir_krate.body(body_id).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
    } else {
        let krate_name = tcx.crate_name(def_id.krate);
        if krate_name.to_string() == "std" && fn_ident == "begin_panic"
            || krate_name.to_string() == "core" && fn_ident == "panic"
        {
            log_panic_in_deny_block(call_stack);
            return;
        }
        acc.save_for_later_check(def_id, tcx, call_stack, None);
    }
}

/// If is local check it now
/// If is not save for later
pub fn handle_assoc_fn<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_id: DefId,
    acc: &mut ForeignCallsToCheck,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
    receiver: Option<DefId>,
) {
    if let Some(local_id) = def_id.as_local() {
        match hir_krate.get_by_def_id(local_id) {
            // TraitItem are handled in handle_qpath function after typechecking we check if the
            // element is a traititem if it is and is solvable we check the implementor for panics
            Node::TraitItem(_) => (),
            Node::ImplItem(item) => {
                if let rustc_hir::ImplItemKind::Fn(_, body_id) = item.kind {
                    let expr = hir_krate.body(body_id).value;
                    get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
                }
            }
            item => panic!("Unexpected Node {:?}", item),
        }
    } else {
        acc.save_for_later_check(def_id, tcx, call_stack, receiver);
    }
}
