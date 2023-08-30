//! helpers to get BodyId of specific fucntions in a crate.

use crate::utils::log_allow_panic;
use rustc_hir::def_id::LOCAL_CRATE;
use rustc_hir::{def_id::DefId, Block, BodyId, Expr, ExprKind, Node, TraitFn};
use rustc_middle::hir::map::Map;

/// Traverse an HIR and for each function that contains a block labelled 'deny_panic return a
/// a (BodyId, (deny_panic_blocks, call_stack)) where:
///     * BodyId is the BodyId of the function
///     * deny_panic_blocks are all the block in the function body labelled 'deny_panic
///     * call satck is a vector that cointains the path of all the function in the call stack for
///     logging purposes. In that case it will contains only the path of the function itself,
///     cause `get_functions` is called on the target crate.
#[allow(clippy::type_complexity)]
pub fn get_functions<'tcx>(
    hir_krate: &mut Map<'tcx>,
) -> Vec<(
    BodyId,
    (Vec<&'tcx Block<'tcx>>, /* call_stack */ Vec<String>),
)> {
    let mut ret = vec![];
    for item_id in hir_krate.items() {
        let item = hir_krate.item(item_id);
        match item.kind {
            rustc_hir::ItemKind::Fn(_, _, body_id) => {
                let mut deny_panic_blocks = vec![];
                let expr = hir_krate.body(body_id).value;
                get_deny_panic_in_expr(expr, &mut deny_panic_blocks);
                if !deny_panic_blocks.is_empty() {
                    let function = format!("{} in {:?}", item.ident.to_string(), item.span);
                    ret.push((body_id, (deny_panic_blocks, vec![function])));
                }
            }
            rustc_hir::ItemKind::Impl(impl_) => {
                for item in impl_.items {
                    let hir = item.id.hir_id();
                    match hir_krate.get(hir) {
                        Node::ImplItem(item) => match item.kind {
                            rustc_hir::ImplItemKind::Fn(_, body_id) => {
                                let mut deny_panic_blocks = vec![];
                                let expr = hir_krate.body(body_id).value;
                                get_deny_panic_in_expr(expr, &mut deny_panic_blocks);
                                if !deny_panic_blocks.is_empty() {
                                    let function =
                                        format!("{} in {:?}", item.ident.to_string(), item.span);
                                    ret.push((body_id, (deny_panic_blocks, vec![function])));
                                }
                            }
                            _ => panic!("Expected item kind Fn"),
                        },
                        _ => panic!("Expected impl item"),
                    }
                }
            }
            _ => (),
        }
    }
    if ret.is_empty() {
        panic!();
    }
    ret
}

use rustc_middle::ty::TyCtxt;

/// Get an HIR and vector of DefId of functions and call stacks.
/// For each function get the BodyId and the Block also add the path of the function in the
/// call_stack.
/// If the Block is not labelled 'allow_panic add the BodyId the Block and the call_stack to a
/// vector the we will return.
/// If the Block is labelled 'allow_panic log it and continue.
#[allow(clippy::type_complexity)]
pub fn get_function_for_dependency<'tcx>(
    tcx: &mut TyCtxt<'tcx>,
    ids: Vec<(DefId, /* call_stack */ Vec<String>, Option<DefId>)>,
) -> Vec<(
    BodyId,
    (Vec<&'tcx Block<'tcx>>, /* call_stack */ Vec<String>),
)> {
    let hir_krate = tcx.hir();
    let mut ret = vec![];
    for id_ in ids {
        let stack = id_.1;
        let mut id = id_.0;
        id.krate = LOCAL_CRATE;
        let item = hir_krate.get_if_local(id).expect("ERROR MESSAGE");
        let fn_body_id = match item {
            Node::Item(item) => item.expect_fn().2,
            Node::ImplItem(item) => item.expect_fn().1,
            Node::TraitItem(item) => match item.expect_fn().1 {
                TraitFn::Provided(body_id) => *body_id,
                TraitFn::Required(_) => {
                    let mut receiver = id_.2.expect("Expect receiver");
                    receiver.krate = LOCAL_CRATE;
                    let ty = tcx.type_of(receiver).skip_binder();
                    let impl_item = super::get_impl_item(tcx, id, Some(ty))
                        .expect("Trait is never implemented in crate");
                    if let Some(Node::ImplItem(item)) = hir_krate.get_if_local(impl_item.def_id) {
                        item.expect_fn().1
                    } else {
                        panic!("Impossible to find called BodyId");
                    }
                }
            },
            _ => panic!("Item is not a function {:?}", item),
        };
        match hir_krate.body(fn_body_id).value.kind {
            ExprKind::Block(block, Some(label)) => {
                if label.ident.as_str().contains("allow_panic") {
                    log_allow_panic(&stack);
                    continue;
                } else {
                    ret.push((fn_body_id, (vec![block], stack)));
                }
            }
            ExprKind::Block(block, None) => ret.push((fn_body_id, (vec![block], stack))),
            _ => panic!("Not a block (unreachable)"),
        }
    }
    ret
}

//fn solveit(hir_krate: &mut Map, id: DefId) {
//    let expr = hir_krate.body(id).value;
//    let result = tcx.typeck(segment.hir_id.owner.def_id);
//    get_deny_panic_in_expr(expr, &mut vec![]);
//}

/// Only check for first level block:
/// this work:
/// ```
/// fn foo() {
///   'deny_panic: {
///   } // do not put a ; here or will not work
/// }
/// ```
///
/// This do not work:
/// ```
/// fn foo() {
///   let x = 'deny_panic: {
///   }
/// }
/// ```
///
/// See issue: #1
///
fn get_deny_panic_in_expr<'tcx>(expr: &Expr<'tcx>, blocks: &mut Vec<&Block<'tcx>>) {
    if let ExprKind::Block(block, None) = expr.kind {
        if let Some(expr) = block.expr {
            if let ExprKind::Block(block, Some(label)) = expr.kind {
                if label.ident.as_str().contains("deny_panic") {
                    blocks.push(block);
                }
            }
        }
    }
}
