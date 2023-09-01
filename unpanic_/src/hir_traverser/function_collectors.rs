//! helpers to get BodyId of specific fucntions in a crate.

use crate::utils::log_allow_panic;
use super::traversers::FunctionCallPartialTree;
use rustc_hir::def_id::LOCAL_CRATE;
use rustc_hir::{def_id::DefId, Block, BodyId, Expr, ExprKind, Node, TraitFn, ItemKind,StmtKind,HirId,GenericBound,GenericParamKind,Item,QPath};
use rustc_middle::hir::map::Map;
use std::collections::HashSet;
use std::collections::HashMap;
//use super::traversers::get_call_in_block;

pub fn get_all_fn_in_crate<'tcx>(tcx: &mut TyCtxt<'tcx>) -> Vec<Block<'tcx>> {
    let mut ret = vec![];
    let hir_krate = tcx.hir();
    for item_id in hir_krate.items() {
        let item = hir_krate.item(item_id);
        match item.kind {
            rustc_hir::ItemKind::Fn(_, _, body_id) => {
                if let ExprKind::Block(block, _) = hir_krate.body(body_id).value.kind {
                    ret.push(*block);
                }
            }
            rustc_hir::ItemKind::Impl(impl_) => {
                for item in impl_.items {
                    let hir = item.id.hir_id();
                    match hir_krate.get(hir) {
                        Node::ImplItem(item) => match item.kind {
                            rustc_hir::ImplItemKind::Fn(_, body_id) => {
                                if let ExprKind::Block(block, _) = hir_krate.body(body_id).value.kind {
                                    ret.push(*block);
                                }
                            }
                            _ => (),
                        },
                        _ => panic!("Expected impl item {:#?}", item),
                    }
                }
            }
            _ => (),
        }
    }
    ret
}


/// Traverse an HIR and for each function that contains a block labelled 'deny_panic return a
/// a (BodyId, (deny_panic_blocks, call_stack)) where:
///     * BodyId is the BodyId of the function
///     * deny_panic_blocks are all the block in the function body labelled 'deny_panic
///     * call satck is a vector that cointains the path of all the function in the call stack for
///     logging purposes. In that case it will contains only the path of the function itself,
///     cause `get_functions` is called on the target crate.
#[allow(clippy::type_complexity)]
pub fn get_functions<'tcx>(
    tcx: &mut TyCtxt<'tcx>,
) -> Vec<(
    BodyId,
    (Vec<&'tcx Block<'tcx>>, /* call_stack */ Vec<String>),
)> {
    let mut ret = vec![];
    let mut hir_krate = tcx.hir();
    let mut to_check_later = HashMap::new();
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
                let indirect = indirect_function_call(tcx, &mut ret);
                to_check_later.extend(indirect);
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
                                let indirect = indirect_function_call(tcx, &mut ret);
                                to_check_later.extend(indirect);
                            }
                            _ => (),
                        },
                        _ => panic!("Expected impl item {:#?}", item),
                    }
                }
            }
            _ => (),
        }
    }
    //if ret.is_empty() {
    //    panic!();
    //}
    dbg!(&to_check_later);
    let all_fn = get_all_fn_in_crate(tcx);
    get_callers(tcx, all_fn, to_check_later);
    ret
}

/// For each expr in callers check if it contaion call to function in called;
fn get_callers<'tcx>(tcx: &mut TyCtxt<'tcx>, callers: Vec<Block<'tcx>>, called: HashMap<DefId, HashSet<usize>>) -> Vec<Block<'tcx>> {
    for block in callers {
        let mut traverser = FunctionCallPartialTree::new(*tcx, false);
        traverser.traverse_block(&block, &mut vec![]);
        for call in traverser.first_level_calls {
            let def_id = dbg!(call_to_def_id(call));
            dbg!(called.contains_key(&def_id));
        }
    };
    todo!()
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
pub fn get_deny_panic_in_expr<'tcx>(expr: &Expr<'tcx>, blocks: &mut Vec<&Block<'tcx>>) {
    if let ExprKind::Block(block, None) = expr.kind {
        for stmt in block.stmts.iter() {
            if let StmtKind::Expr(expr) = stmt.kind {
                if let ExprKind::Block(block, Some(label)) = expr.kind {
                    if label.ident.as_str().contains("deny_panic") {
                        blocks.push(block);
                    }
                }
            }
        }
        if let Some(expr) = block.expr {
            if let ExprKind::Block(block, Some(label)) = expr.kind {
                if label.ident.as_str().contains("deny_panic") {
                    blocks.push(block);
                }
            }
        }
    }
}

// For each 'deny_panic block check if contains call to not yet defined function.
// If it is and is a call to a closure return the function that contains the block and the arg
// number of the closure that must not panic, in a second passage we check all caller to check if
// the passed closure can panic.
//
// If it is a trait TODO
pub fn indirect_function_call<'tcx>(
    tcx: &mut TyCtxt<'tcx>,
    deny_panic_fn_bodies: &mut Vec<(BodyId, (Vec<&Block<'tcx>>,Vec<String>))>,
) -> HashMap<DefId, HashSet<usize>> {
    let mut hir_krate = tcx.hir();
    let mut to_check_later: HashMap<DefId, HashSet<usize>> = HashMap::new();
    for deny_panic_fn in deny_panic_fn_bodies {
        let function_id = deny_panic_fn.0.hir_id.owner;
        let typeck_results = tcx.typeck(function_id);
        let body = tcx.hir().body(deny_panic_fn.0);
        for block in &deny_panic_fn.1.0 {
            let calls_in_block = get_call_in_block(block,tcx);
            for call in calls_in_block {
                if let Some(i) = get_arg_number(call,body) {
                    if typeck_results.type_dependent_def(call.hir_id).is_some() {
                        if let Some(set) = to_check_later.get_mut(&function_id.to_def_id()) {
                            set.insert(i);
                        } else {
                            let mut set = HashSet::new();
                            set.insert(i);
                            to_check_later.insert(function_id.to_def_id(), set);
                        }
                    }
                }
            }
        }
    }
    to_check_later
}

fn get_call_in_block<'tcx>(block: &Block<'tcx>, tcx: &mut TyCtxt<'tcx>) -> Vec<Expr<'tcx>> {
    let mut traverser = FunctionCallPartialTree {
        tcx: *tcx,
        visited_functions: HashMap::new(),
        visited_assoc_functions: HashMap::new(),
        /// For each allow_panic that we encounter we save the call_stack
        allow_panics: Vec::new(),
        save_stack: false,
        first_level_calls: Vec::new(),
    };
    traverser.traverse_block(block,&mut Vec::new());
    traverser.first_level_calls
}

fn get_arg_number(expr: Expr<'_>, fn_body: & rustc_hir::Body<'_>) -> Option<usize> {
    let generic_hir = get_hir(expr)?;
    for (i,param) in fn_body.params.iter().enumerate() {
        if param.pat.hir_id == generic_hir {
            return Some(i)
        }
    }
    // Not every call is associated to an arg number
    None
}

fn get_function_hir<'hir>(expr: Expr<'hir>) -> rustc_hir::HirId {
    match expr.kind {
        ExprKind::Call(ref function, ref _args) => {
            function.hir_id
        },
        _ => panic!(),
    }
}
fn get_function_symbol<'hir>(expr: Expr<'hir>) -> rustc_span::Symbol {
    match expr.kind {
        ExprKind::Call(ref function, ref _args) => {
            match function.kind {
                ExprKind::Path(ref path) => match path {
                    rustc_hir::QPath::Resolved(_,path) => path.segments.last().unwrap().ident.name,
                    _ => panic!(),
                }
                _ => panic!(),
            }
        },
        _ => panic!(),
    }
}
fn get_res<'hir>(expr: Expr<'hir>) -> Option<rustc_hir::def::Res> {
    match expr.kind {
        ExprKind::Call(ref function, ref _args) => {
            match function.kind {
                ExprKind::Path(ref path) => match path {
                    rustc_hir::QPath::Resolved(_,path) => Some(path.res),
                    // If it refer to a closure it will be solved
                    _ => None,
                }
                _ => panic!(),
            }
        },
        _ => {
            dbg!(expr);
            panic!();
        }
    }
}

fn get_hir<'hir>(expr: Expr<'hir>) -> Option<HirId> {
    match get_res(expr)? {
        rustc_hir::def::Res::Local(id) => Some(id),
        // If it refer to a closure it will be local
        _ => None
    }
}
fn call_to_def_id<'hir>(expr: Expr<'hir>) -> DefId {
    match expr.kind {
        ExprKind::Call(ref function, ref _args) => match function.kind {
                ExprKind::Path(ref path) => match path {
                    QPath::Resolved(_,path) => path.segments.last().unwrap().hir_id.owner.to_def_id(),
                    QPath::TypeRelative(_,segment) => segment.hir_id.owner.to_def_id(),
                    _ => panic!()
                }
                _ => panic!()
            }
        _ => panic!()
    }
}
