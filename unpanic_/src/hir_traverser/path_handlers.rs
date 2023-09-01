// //! When we want to check if a call to a function contains panics, what we have id the QPath that
// //! point to that function. The QPath can be either local or not. If it is local we can get the
// //! function body and check for panics, if it is not we have to save the QPath for later cheks.
// use rustc_hir::HirId;
// use rustc_hir::{def::DefKind, def::Res, def_id::DefId, ItemKind, Node, QPath};
// use rustc_middle::hir::map::Map;
// use rustc_middle::ty::TyCtxt;
// use rustc_type_ir::sty::TyKind;
// 
// //use super::traversers::get_panic_in_expr;
// use super::ForeignCallsToCheck;
// 
// use super::function_handlers::{handle_assoc_fn, handle_fn};
// 
// /// If the QPath is not type dependent (we already know which function we want to call) we call
// /// handle_solved_path.
// /// It the QPath is type dependent we need to solve it in order to know the actual function that we
// /// are calling.
// /// If we have a LangItem we do not check it as they are considered safe TODO add a field in the
// /// config file to check for unwanted LangItem
// pub fn handle_qpath<'tcx>(qpath: QPath, tcx: &mut TyCtxt<'tcx>, state: &mut super::TraverserState<'tcx>, call_stack: &[String]) -> bool {
//     let hir_krate = tcx.hir();
//     match qpath {
//         QPath::Resolved(_, path) => {
//             if let Some(last) = path.segments.last() {
//                 match last.res {
//                     Res::Def(def_kind, def_id) => {
//                         let fn_ident = last.ident.as_str().to_string();
//                         return false;
//                         //return handle_solved_path(def_kind, def_id, fn_ident, tcx, &qpath, state,call_stack)
//                     }
//                     Res::Local(id) => {
//                         return false
//                         //match tcx.hir().find(id.owner.into()) {
//                         //    Some(Node::Item(item)) => match item.kind {
//                         //        ItemKind::Fn(sign, generics, body_id) => {
//                         //            let mut closures_args = Vec::new();
//                         //            for input in sign.decl.inputs {
//                         //                dbg!(input);
//                         //                closures_args.push(input);
//                         //            }
//                         //            let mut blocks = Vec::new();
//                         //            let expr = tcx.hir().body(body_id).value;
//                         //            super::function_collectors::get_deny_panic_in_expr(
//                         //                expr,
//                         //                &mut blocks,
//                         //            );
//                         //            dbg!(blocks);
//                         //        }
//                         //        _ => panic!(),
//                         //    }
//                         //    _ => panic!(),
//                         //}
// 
//                         //let result = tcx.typeck(id.owner.def_id);
//                         //let items = result.node_types().items_in_stable_order();
//                         //for item in items {
//                         //    match item.1.kind() {
//                         //    }
//                         //}
// 
//                         //match dbg!(tcx.hir().find(id)) {
//                         //    Some(Node::Pat(pat)) => {
//                         //    }
//                         //}
//                         //handle_fn(
//                         //    hir_krate,
//                         //    tcx.local_def_id(id.owner),
//                         //    "GIGI".to_string(),
//                         //    acc,
//                         //    tcx,
//                         //    call_stack,
//                         //    visited_functions,
//                         //);
//                     }
//                     _ => panic!("Unexpected Res"),
//                 }
//             } else {
//                 false
//             }
//         }
//         QPath::TypeRelative(_, segment) => {
//             false
//             //let result = tcx.typeck(segment.hir_id.owner.def_id);
//             //let items = result.node_types().items_in_stable_order();
//             //for item in items {
//             //    if let TyKind::FnDef(def_id, generic_args) = item.1.kind() {
//             //        // If is a trait fn and is implemented locally check it for panics
//             //        if let Some(impl_item) = super::get_impl_item(
//             //            tcx,
//             //            *def_id,
//             //            generic_args.get(0).map(|x| x.expect_ty()),
//             //        ) {
//             //            handle_assoc_fn(
//             //                impl_item.def_id,
//             //                tcx,
//             //                state,
//             //                // If we are here the trait item implementation is local so no need to
//             //                // pass the implementor type for future checks
//             //                None,
//             //            )
//             //        // If is local check if the function contains calls to panic
//             //        } else if let Some(local_id) = def_id.as_local() {
//             //            if let Some(Node::Item(item)) = hir_krate.find_by_def_id(local_id) {
//             //                if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
//             //                    let expr = hir_krate.body(body_id).value;
//             //                    get_panic_in_expr(expr, tcx, state);
//             //                }
//             //            };
//             //            if let Some(Node::ImplItem(item)) = hir_krate.find_by_def_id(local_id) {
//             //                if let rustc_hir::ImplItemKind::Fn(_, body_id) = item.kind {
//             //                    let expr = hir_krate.body(body_id).value;
//             //                    get_panic_in_expr(expr, tcx, state);
//             //                }
//             //            };
//             //        // Otherwise save it for later check
//             //        } else {
//             //            let receiver = generic_args.get(0).map(|x| match x.expect_ty().kind() {
//             //                rustc_middle::ty::Adt(adt_def, _) => adt_def.did(),
//             //                _ => panic!(),
//             //            });
//             //            state.acc.save_for_later_check(
//             //                *def_id,
//             //                tcx,
//             //                &mut state.call_stack,
//             //                receiver,
//             //            );
//             //        }
//             //    }
//             //}
//         }
//         // TODO Maybe add a field in the config file to check also for not wanted LangItem
//         QPath::LangItem(_, _, _) => false,
//     }
// }
// 
// #[allow(clippy::too_many_arguments)]
// pub fn handle_solved_path<'tcx>(
//     def_kind: DefKind,
//     def_id: DefId,
//     fn_ident: String,
//     tcx: &mut TyCtxt<'tcx>,
//     qpath: &QPath,
//     state: &mut super::TraverserState<'tcx>,
//     call_stack: &[String],
// ) -> bool {
//     match def_kind {
//         DefKind::Fn => handle_fn(def_id, fn_ident, tcx, state,call_stack),
//         // Ignore contructor they can no panic
//         DefKind::Ctor(_, _) => false,
//         kind => panic!(
//             "Unhandled kind {:?}, please open an issue on https://github.com/Fi3/unpanic/issues",
//             kind
//         ),
//     }
// }
// 
// fn handle_fn_with_closures_as_arg() {
//     // Check if and which closures are called inside a deny_panic block
// 
//     // Save the closres that can not panic
// 
//     // Return the function alongside the closure that can not pani
// 
//     // Later redo a complete parse of all the crates beginnning from the target to see if any
//     // function call the function and to see  if it pass closures that can panic!
// }
