//! When we have the def_id of an actual function we can check it, this is kind of the final phase
//! of the checker, of course if the function is not very simple it will call itself recursively
//! via the traverser.
use rustc_hir::HirId;
use rustc_hir::{def_id::DefId, Node};
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;

use super::ForeignCallsToCheck;
use crate::utils::log_panic_in_deny_block;

pub fn check_fn_panics<'tcx>(
    def_id: DefId,
    fn_ident: String,
    tcx: &mut TyCtxt<'tcx>,
    acc: &mut super::ForeignCallsToCheck,
    call_stack: &[String],
    to_log: &mut Vec<Vec<String>>,
) {
    let hir_krate = tcx.hir();
    if !def_id.is_local() {
        let krate_name = tcx.crate_name(def_id.krate);
        if is_panic(krate_name.as_str(), fn_ident.as_str()) {
            to_log.push(call_stack.to_vec());
        } else {
            acc.save_for_later_check(def_id, tcx, call_stack, None);
        }
    } else {
        panic!()
    }
}

pub fn is_panic(krate_name: &str, fn_name: &str) -> bool {
    let look_for = vec![("std", vec!["begin_panic"]), ("core", vec!["panic"])];
    for krate in look_for {
        let krate_name = krate.0;
        let function_names = krate.1;
        if krate_name == krate_name && function_names.contains(&fn_name) {
            return true;
        }
    }
    false
}

/// If is local check it now
/// If is not save for later
pub fn check_assoc_fn<'tcx>(
    def_id: DefId,
    tcx: &mut TyCtxt<'tcx>,
    acc: &mut super::ForeignCallsToCheck,
    receiver: Option<DefId>,
    call_stack: &[String],
) {
    let hir_krate = tcx.hir();
    if let Some(local_id) = def_id.as_local() {
        panic!()
    } else {
        acc.save_for_later_check(def_id, tcx, call_stack, receiver);
    }
}
