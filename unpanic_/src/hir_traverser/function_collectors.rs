//! helpers to get BodyId of specific fucntions in a crate.

use super::traversers::FunctionCallPartialTree;
use crate::utils::log_allow_panic;
use rustc_hir::def_id::{LOCAL_CRATE,DefIndex};
use rustc_hir::{
    def_id::DefId, Block, BodyId, Expr, ExprKind, GenericBound, GenericParamKind, HirId, Item,
    ItemKind, Node, QPath, StmtKind, TraitFn,def::Res,ImplItemKind,
};
use rustc_middle::hir::map::Map;
use rustc_span::Span;
use std::collections::HashMap;
use std::collections::HashSet;
use rustc_middle::ty::fast_reject::{TreatParams,simplify_type};
use rustc_type_ir::sty::TyKind;
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
                                if let ExprKind::Block(block, _) =
                                    hir_krate.body(body_id).value.kind
                                {
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

/// For each expr in callers check if it contaion call to function in called;
pub fn get_callers<'tcx>(
    tcx: &mut TyCtxt<'tcx>,
    callers: Vec<Block<'tcx>>,
    called: HashMap<DefId, HashMap<usize,DefId>>,
) -> HashMap<HirId, Vec<(Expr<'tcx>, DefId, HashMap<usize,DefId>,String)>> {
    let mut ret: HashMap<HirId, Vec<(Expr<'tcx>, DefId, HashMap<usize,DefId>,String)>> = HashMap::new();
    // TODO is DefIndex correct here?
    let called: HashMap<DefIndex, HashMap<usize,DefId>> = called.into_iter().map(|(def_id, v)| {
        (def_id.index, v.clone())
    }).collect();
    for block in callers {
        let to_log = format!("{:?} in {:?}", block.hir_id.owner, block.span);
        let mut traverser = FunctionCallPartialTree::new(*tcx, false);
        traverser.traverse_block(&block, &mut vec![]);
        for call in traverser.first_level_calls {
            if let Some(def_id) = from_callers_to_called_def_id(tcx,call) {
                if called.contains_key(&def_id.index) {
                    if let Some(v) = ret.get_mut(&block.hir_id) {
                        let map = called.get(&def_id.index).unwrap().clone();
                        v.push((call, def_id, map, to_log.clone()));
                    } else {
                        let map = called.get(&def_id.index).unwrap().clone();
                        ret.insert(block.hir_id, vec![(call, def_id, map,to_log.clone())]);
                    }
                }
            }
        }
    }
    ret
}

/// Given an HashMap of callers DefId -> (calling_expr, DefId, deny args)
/// return the Expr rapresenting the denied arg in the calling expression
pub fn callers_into_args<'tcx>(callers: HashMap<HirId, Vec<(Expr<'tcx>, DefId, HashMap<usize,DefId>,String)>>) -> Vec<(Expr<'tcx>,String,DefId)> {
    let mut ret = vec![];
    let mut control = vec![];
    callers.values().for_each(|v| {
        v.iter().for_each(|(expr,_,arg_indexes,to_log)| {
            let args = match expr.kind {
                ExprKind::Call(_, args) => args,
                ExprKind::MethodCall(_, _, args, _) => args,
                _ => panic!(),
            };
            for (i,def_id) in arg_indexes {
                // TODO add comment (why arg can be either i or i - 1 ??)
                if i < &args.len() {
                    let arg = args[*i];
                    if ! control.contains(&arg.hir_id) {
                        control.push(arg.hir_id);
                        ret.push((arg,to_log.clone(),def_id.clone()));
                    }
                } else if i > &0_usize {
                    let arg = args[*i - 1];
                    if ! control.contains(&arg.hir_id) {
                        control.push(arg.hir_id);
                        ret.push((arg,to_log.clone(),def_id.clone()));
                    }
                }
            }
        });
    });
    ret
}

use rustc_middle::ty::TyCtxt;

/// Get an HIR and vector of DefId of functions and call stacks.
/// For each function get the BodyId and the Block also add the path of the function in the
/// call_stack.
/// If the Block is not labelled 'allow_panic add the BodyId the Block and the call_stack to a
/// vector then we will return.
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
// If it is a trait it does the same TODO verify it and if is the right thing todo
pub fn get_procedural_parameters<'tcx>(
    tcx: &mut TyCtxt<'tcx>,
    deny_panic_fn_bodies: &Vec<(BodyId, (Vec<&Block<'tcx>>, Vec<String>))>,
) -> 
HashMap<
    /* id of the function with procedural parameters or trait */ DefId,
    /* parameter number with trait method def id */ HashMap<usize, DefId>,
>
{
    let mut hir_krate = tcx.hir();
    let mut to_check_later: HashMap<DefId, HashMap<usize,DefId>> = HashMap::new();
    for deny_panic_fn in deny_panic_fn_bodies {
        let function_id = deny_panic_fn.0.hir_id.owner;
        let typeck_results = tcx.typeck(function_id);
        let body = tcx.hir().body(deny_panic_fn.0);
        for block in &deny_panic_fn.1 .0 {
            let calls_in_block = get_call_in_block(block, tcx);
            for call in calls_in_block {
                if let Some(i) = get_arg_number(call, body) {
                    if let Some(res) = typeck_results.type_dependent_def(call.hir_id) {
                        if let Some(map) = to_check_later.get_mut(&function_id.to_def_id()) {
                            map.insert(i,res.1);
                        } else {
                            let mut map = HashMap::new();
                            map.insert(i,res.1);
                            to_check_later.insert(function_id.to_def_id(), map);
                        }
                    }
                }
            }
        }
    }
    to_check_later
}

pub fn solve_arg<'tcx>(tcx: &mut TyCtxt<'tcx>, arg: Expr<'tcx>, method: DefId) -> Expr<'tcx> {
    match arg.kind {
        ExprKind::Path(path) => match path {
            QPath::Resolved(_,path) => {
                let last = path.segments.last().unwrap();
                match last.res {
                    Res::Local(id) => {
                        let result = tcx.typeck(id.owner.def_id);
                        let ty = result.expr_ty(&arg);
                        let trait_id = tcx.trait_of_item(method).unwrap();
                        let mut trait_impls = tcx.trait_impls_of(trait_id);
                        if ! trait_impls.blanket_impls().is_empty() {
                            todo!()
                        }
                        let simplified_ty = simplify_type(tcx.clone(),ty,TreatParams::ForLookup).unwrap();
                        if let Some(def_ids) = trait_impls.non_blanket_impls().get(&simplified_ty) {
                            let trait_items = match def_ids.len() {
                                1 => tcx.associated_items(def_ids[0]),
                                // TODO here I should have only one element cause I the impl of
                                // Trait and then I get the impl for Type so I should always have
                                // at maximum one element ??
                                _ => todo!(),
                            };
                            for item in trait_items.in_definition_order() {
                                if item.trait_item_def_id.unwrap() == method {
                                    let def_id = item.def_id.as_local().unwrap();
                                    match tcx.hir().get_by_def_id(def_id).expect_impl_item().kind {
                                        ImplItemKind::Fn(_,body_id) => {
                                            return tcx.hir().body(body_id).value.clone()
                                        }
                                        _ => panic!(),
                                    }
                                }
                            }
                            panic!()
                        } else {
                            // TODO we should return it save for later and recursivly check
                            todo!()
                        }
                    }
                    _ => panic!(),
                }
            }
            _ => panic!(),
        },
        ExprKind::Closure(_) => arg,
        _ => panic!(),
    }
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
    traverser.traverse_block(block, &mut Vec::new());
    traverser.first_level_calls
}

fn get_arg_number(expr: Expr<'_>, fn_body: &rustc_hir::Body<'_>) -> Option<usize> {
    let generic_hir = get_hir(expr)?;
    for (i, param) in fn_body.params.iter().enumerate() {
        if param.pat.hir_id == generic_hir {
            return Some(i);
        }
    }
    // Not every call is associated to an arg number
    None
}

fn get_function_hir<'hir>(expr: Expr<'hir>) -> rustc_hir::HirId {
    match expr.kind {
        ExprKind::Call(ref function, ref _args) => function.hir_id,
        _ => panic!(),
    }
}
fn get_function_symbol<'hir>(expr: Expr<'hir>) -> rustc_span::Symbol {
    match expr.kind {
        ExprKind::Call(ref function, ref _args) => match function.kind {
            ExprKind::Path(ref path) => match path {
                rustc_hir::QPath::Resolved(_, path) => path.segments.last().unwrap().ident.name,
                _ => panic!(),
            },
            _ => panic!(),
        },
        _ => panic!(),
    }
}
fn get_res<'hir>(expr: Expr<'hir>) -> Option<rustc_hir::def::Res> {
    match expr.kind {
        ExprKind::Call(function, _) => {
            match function.kind {
                ExprKind::Path(ref path) => match path {
                    rustc_hir::QPath::Resolved(_, path) => Some(path.res),
                    // If it refer to a closure it will be solved
                    _ => None,
                },
                _ => panic!(),
            }
        }
        ExprKind::MethodCall(_, function, _,_) => {
            match function.kind {
                ExprKind::Path(ref path) => match path {
                    rustc_hir::QPath::Resolved(_, path) => Some(path.res),
                    // If it refer to a closure it will be solved
                    _ => None,
                },
                _ => panic!(),
            }
        }
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
        _ => None,
    }
}
fn from_callers_to_called_def_id<'tcx>(tcx: &mut TyCtxt<'tcx>, expr: Expr<'tcx>) -> Option<DefId> {
    match expr.kind {
        ExprKind::Call(function, _) => match function.kind {
            ExprKind::Path(path) => match path {
                QPath::Resolved(_, path) => path.res.opt_def_id(),
                QPath::TypeRelative(_, segment) => segment.res.opt_def_id(),
                // TODO this should be unreachable
                _ => None,
            },
            // TODO this should be unreachable
            _ => None,
        },
        ExprKind::MethodCall(_,function,_,_) => match function.kind {
            ExprKind::Path(path) => match path {
                QPath::Resolved(_, path) => {
                    match path.res.opt_def_id() {
                        Some(def_id) => Some(def_id),
                        None => {
                            match path.res {
                                Res::Local(id) => {
                                    let result = tcx.typeck(function.hir_id.owner.def_id);
                                    let ty = result.expr_ty(function);
                                    let def_id = result
                                        // TODO add comment why we return None here
                                        .type_dependent_def_id(expr.hir_id)?;
                                    Some(def_id)
                                },
                                _ => panic!(),
                            }
                        }
                    }
                }
                QPath::TypeRelative(_, segment) => segment.res.opt_def_id(),
                _ => panic!(),
            },
            // TODO this should be unreachable
            _ => None,
        },
        _ => panic!(),
    }
}

//fn resolve_path<'tcx>(&mut tcx: TyCtxt<'tcx>, res: Res
