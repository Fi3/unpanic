use rustc_driver::DEFAULT_LOCALE_RESOURCES;
use rustc_errors::registry::Registry;
use rustc_hash::{FxHashMap, FxHashSet};
use rustc_hir::{
    def::DefKind, def::Res, def_id::DefId, Block, BodyId, Expr, ExprKind, Guard, Node, QPath,
    StmtKind, TraitFn,
};
use rustc_hir::{def_id::LOCAL_CRATE, hir_id::ItemLocalId, HirId};
use rustc_interface::Config;
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;
use rustc_session::config::*;
use rustc_target::spec::TargetTriple;
use rustc_type_ir::sty::TyKind;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use crate::rustc_arg_handlers::*;

pub struct HirTraverser {
    pub errors: Vec<String>,
    pub function_to_check: HashMap<
        /* krate name */ String,
        /* function path, call stack */ Vec<(DefId, Vec<String>)>,
    >,
    pub target_args: Vec<String>,
    pub dep_map: HashMap<
        /* krate name */ String,
        /*args:*/ (/* buildrs*/ Option<Vec<String>>, Vec<String>),
    >,
    pub sysroot: PathBuf,
    pub visited_functions: Vec<HirId>,
}

impl HirTraverser {
    pub fn start(&mut self) {
        let target_config = config_from_args(&self.target_args, &self.sysroot);
        self.check_crate(target_config, None);
        while !self.function_to_check.keys().is_empty() {
            let keys = &self.function_to_check.clone();
            let keys = keys.keys();
            for key in keys {
                let to_check = self
                    .function_to_check
                    .remove(key)
                    .expect("ERROR: No key in deps map");
                match key.as_str() {
                    "std" | "alloc" | "core" => eprintln!("skip std"),
                    _ => {
                        let (_, dep_args) = self
                            .dep_map
                            .get_mut(key)
                            .expect("ERROR: No key in deps map");
                        let target_config = config_from_args(dep_args, &self.sysroot);
                        self.check_crate(target_config, Some(to_check));
                    }
                };
            }
        }
    }

    fn check_crate(
        &mut self,
        target_config: Config,
        function_to_check: Option<Vec<(DefId, Vec<String>)>>,
    ) {
        rustc_interface::run_compiler(target_config, |compiler| {
            compiler.enter(|queries| {
                queries
                    .global_ctxt()
                    .expect("ERROR: Can not get global context")
                    .enter(|mut tcx| {
                        let ids = match function_to_check {
                            Some(ids) => get_function_for_dependency(&mut tcx.hir(), ids),
                            None => get_functions(&mut tcx.hir()),
                        };
                        for elem in &ids {
                            self.visited_functions = vec![];
                            let blocks = &elem.1 .0;
                            let block = blocks[0];
                            let mut call_stack = elem.1 .1.clone();
                            get_panic_in_block(
                                &mut tcx.hir(),
                                block,
                                &mut self.function_to_check,
                                &mut tcx,
                                &mut call_stack,
                                &mut self.visited_functions,
                            );
                        }
                    })
            })
        });
    }
}
#[allow(clippy::type_complexity)]
fn get_function_for_dependency<'tcx>(
    hir_krate: &mut Map<'tcx>,
    ids: Vec<(DefId, Vec<String>)>,
) -> Vec<(BodyId, (Vec<&'tcx Block<'tcx>>, Vec<String>))> {
    let mut ret = vec![];
    for id in ids {
        let stack = id.1;
        let mut id = id.0;
        id.krate = LOCAL_CRATE;
        let item = hir_krate.get_if_local(id).expect("ERROR MESSAGE");
        let fn_body_id = match item {
            Node::Item(item) => item.expect_fn().2,
            Node::ImplItem(item) => item.expect_fn().1,
            Node::TraitItem(item) => match item.expect_fn().1 {
                TraitFn::Provided(body_id) => *body_id,
                TraitFn::Required(_body_id) => return vec![],
            },
            _ => todo!(),
        };
        match hir_krate.body(fn_body_id).value.kind {
            ExprKind::Block(block, Some(label)) => {
                if label.ident.as_str().contains("allow_panic") {
                    eprintln!("ATTENTION ALLOW PANIC IN A DEPENDENCY");
                    continue;
                } else {
                    ret.push((fn_body_id, (vec![block], stack)));
                }
            }
            ExprKind::Block(block, None) => ret.push((fn_body_id, (vec![block], stack))),
            _ => todo!(),
        }
    }
    ret
}

/// Traverse the crate and return all the functions that contains `deny_panic blocks and the blocks
#[allow(clippy::type_complexity)]
fn get_functions<'tcx>(
    hir_krate: &mut Map<'tcx>,
) -> Vec<(BodyId, (Vec<&'tcx Block<'tcx>>, Vec<String>))> {
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
                            _ => todo!(),
                        },
                        _ => todo!(),
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
/// TODO we should be able to add deny_panic everywhere TODO add issue for it and reference it here
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

/// If it is solved call handle_solved_path
/// If not solve and call handle_solved_path
fn handle_qpath<'tcx>(
    hir_krate: &mut Map<'tcx>,
    qpath: QPath,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
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
                    _ => todo!(),
                }
            }
        }
        QPath::TypeRelative(_, segment) => {
            // TODO this `-3` work and I donno why!
            let local = ItemLocalId::from_usize(segment.hir_id.local_id.as_usize() - 3);
            let mut s = segment.hir_id;
            s.local_id = local;
            let result = tcx.typeck(segment.hir_id.owner.def_id);
            let items = result.node_types().items_in_stable_order();
            for item in items {
                match item.1.kind() {
                    TyKind::FnDef(def_id, generic_args) => {
                        // Take the parent if fn def is a trait this is the trait definition
                        let parent = tcx.parent(*def_id);
                        // if is a trait find the implementor and handle as associate function
                        if let Some(impls) = tcx.all_local_trait_impls(()).get(&parent) {
                            let trait_item_name = tcx.item_name(*def_id);
                            for impl_def_id in impls {
                                let impl_self_ty =
                                    tcx.type_of(impl_def_id.to_def_id()).subst_identity();
                                if generic_args[0] == impl_self_ty.into() {
                                    let impl_items = tcx.associated_items(impl_def_id.to_def_id());
                                    for impl_item in impl_items.in_definition_order() {
                                        if impl_item.name == trait_item_name {
                                            // name of the trait item you started with
                                            handle_assoc_fn(
                                                hir_krate,
                                                impl_item.def_id,
                                                acc,
                                                tcx,
                                                call_stack,
                                                visited_functions,
                                            )
                                        }
                                    }
                                }
                            }
                        // If is local check if the function contains call to panic
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
                            }
                        // Otherwise save it for later check
                        } else {
                            // TODO remove these strings that get passed around
                            let path = format!("{:?}", def_id);
                            let path: Vec<&str> = path.split("::").collect();
                            save_non_local_def_id(
                                *def_id,
                                path.last().unwrap().to_string(),
                                acc,
                                tcx,
                                call_stack,
                            );
                        }
                    }
                    TyKind::FnPtr(_) => todo!(),
                    TyKind::Dynamic(_, _, _) => todo!(),
                    TyKind::Closure(_, _) => todo!(),
                    TyKind::Generator(_, _, _) => todo!(),
                    _ => (),
                }
            }

            //match result.qpath_res(&qpath, s) {
            //    Res::Def(def_kind, def_id) => {
            //        let path = format!("{:?}", def_id);
            //        let path: Vec<&str> = path.split("::").collect();
            //        handle_solved_path(
            //            hir_krate,
            //            def_kind,
            //            def_id,
            //            path.last().unwrap().to_string(),
            //            acc,
            //            tcx,
            //            &qpath,
            //            call_stack,
            //            visited_functions,
            //        );
            //    }
            //    _ => todo!(),
            //}
        }
        // TODO
        QPath::LangItem(_, _, _) => todo!(),
    }
}
/// If is local check it now
/// If is not save for later
/// If is a panic emit an error
fn handle_fn<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_id: DefId,
    fn_ident: String,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
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
            eprintln!("OMG A PANIC");
            for funtion in call_stack.clone() {
                eprintln!("    {}\n", funtion);
            }
            return;
        }
        if let Some(functions) = acc.get_mut(&krate_name.to_string()) {
            functions.push((def_id, call_stack.clone()));
        } else {
            acc.insert(krate_name.to_string(), vec![(def_id, call_stack.clone())]);
        }
    }
}

fn save_non_local_def_id(
    def_id: DefId,
    fn_ident: String,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
    tcx: &mut TyCtxt<'_>,
    call_stack: &mut [String],
) {
    let krate_name = tcx.crate_name(def_id.krate);
    if krate_name.to_string() == "std" && fn_ident == "begin_panic"
        || krate_name.to_string() == "core" && fn_ident == "panic"
    {
        eprintln!("OMG A PANIC");
        for funtion in call_stack {
            eprintln!("    {}\n", funtion);
        }
        return;
    }
    if let Some(functions) = acc.get_mut(&krate_name.to_string()) {
        functions.push((def_id, call_stack.to_owned()));
    } else {
        acc.insert(krate_name.to_string(), vec![(def_id, call_stack.to_owned())]);
    }
}

/// If is local check it now
/// If is not save for later
/// If is a panic emit an error
#[allow(clippy::too_many_arguments)]
fn handle_solved_path<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_kind: DefKind,
    def_id: DefId,
    fn_ident: String,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
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
        DefKind::AssocFn => {
            handle_assoc_fn(hir_krate, def_id, acc, tcx, call_stack, visited_functions)
        }
        // TODO
        kind => println!("Unhandled kind {:?}", kind),
    }
}

/// If is local check it now
/// If is not save for later
fn handle_assoc_fn<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_id: DefId,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
    visited_functions: &mut Vec<HirId>,
) {
    if let Some(local_id) = def_id.as_local() {
        match hir_krate.get_by_def_id(local_id) {
            // TraitItem are handled in ... TODO complete comment
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
        let krate_name = tcx.crate_name(def_id.krate);
        if let Some(functions) = acc.get_mut(&krate_name.to_string()) {
            functions.push((def_id, call_stack.clone()));
        } else {
            acc.insert(krate_name.to_string(), vec![(def_id, call_stack.clone())]);
        }
    }
}

fn get_panic_in_block<'tcx>(
    hir_krate: &mut Map<'tcx>,
    block: &Block<'tcx>,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
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

fn get_panic_in_expr<'tcx>(
    hir_krate: &mut Map<'tcx>,
    expr_: &Expr<'tcx>,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
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
                let def_id = result
                    .type_dependent_def_id(expr_.hir_id)
                    .expect("ERROR: Can not get def id");
                let function = format!("{:?} in {:?}", method.ident, ty);
                call_stack.push(function);
                handle_assoc_fn(hir_krate, def_id, acc, tcx, call_stack, visited_functions);

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
        // TODO check if BinOp can panic
        ExprKind::Binary(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack, visited_functions);
        }
        // TODO check if UnOp can panic
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
        // TODO check if label is allow_panic
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
                // TODO this is always printed!
                eprintln!("ATTENTION ALLOW PANIC IN A DEPENDENCY");
            }
        }
        ExprKind::Block(block, None) => {
            get_panic_in_block(hir_krate, block, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Assign(arg1, arg2, _) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack, visited_functions);
        }
        // TODO check if BinOp can panic
        ExprKind::AssignOp(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack, visited_functions);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Field(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack, visited_functions);
        }
        ExprKind::Index(arg1, arg2) => {
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
        ExprKind::Err(_) => panic!(),
    }
}

fn get_panic_in_stmt<'tcx>(
    hir_krate: &mut Map<'tcx>,
    stmt: &StmtKind<'tcx>,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
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

pub fn config_from_args(args: &Vec<String>, sysroot: &Path) -> Config {
    let src_path = &get_location(args).expect("ERROR: No location in args");
    let src_path = Path::new(src_path);
    let (externs, search_paths) = get_externs(args);
    let edition = get_edition(args);
    Config {
        opts: Options {
            maybe_sysroot: Some(sysroot.to_path_buf()),
            incremental: None,
            externs,
            edition,
            search_paths,
            target_triple: TargetTriple::TargetTriple("x86_64-unknown-linux-gnu".to_string()),
            crate_name: Some(get_crate_name(args).expect("ERROR: No crate name in args")),
            ..Options::default()
        },
        input: Input::File(src_path.to_path_buf()),
        crate_cfg: FxHashSet::default(),
        crate_check_cfg: CheckCfg::default(),
        output_dir: None,
        output_file: None,
        file_loader: None,
        locale_resources: DEFAULT_LOCALE_RESOURCES,
        lint_caps: FxHashMap::default(),
        parse_sess_created: None,
        register_lints: None,
        override_queries: None,
        make_codegen_backend: None,
        registry: Registry::new(rustc_error_codes::DIAGNOSTICS),
    }
}
