use rustc_driver::DEFAULT_LOCALE_RESOURCES;
use rustc_errors::registry::Registry;
use rustc_hash::{FxHashMap, FxHashSet};
use rustc_hir::{
    def::DefKind, def::Res, def_id::DefId, Block, BodyId, Expr, ExprKind, Guard, Node, QPath,
    StmtKind,
};
use rustc_hir::{def_id::LOCAL_CRATE, hir_id::ItemLocalId};
use rustc_interface::Config;
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;
use rustc_session::config::*;
use rustc_target::spec::TargetTriple;
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
                        let target_config = config_from_args(&dep_args, &self.sysroot);
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
                            let blocks = &elem.1 .0;
                            let block = blocks[0];
                            let mut call_stack = elem.1 .1.clone();
                            get_panic_in_block(
                                &mut tcx.hir(),
                                block,
                                &mut self.function_to_check,
                                &mut tcx,
                                &mut call_stack,
                            );
                        }
                    })
            })
        });
    }
}
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
fn get_functions<'tcx>(
    hir_krate: &mut Map<'tcx>,
) -> Vec<(BodyId, (Vec<&'tcx Block<'tcx>>, Vec<String>))> {
    let mut ret = vec![];
    for item_id in hir_krate.items() {
        let item = hir_krate.item(item_id);
        if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
            let mut deny_panic_blocks = vec![];
            let expr = hir_krate.body(body_id).value;
            get_deny_panic_in_expr(expr, &mut deny_panic_blocks);
            if !deny_panic_blocks.is_empty() {
                let function = format!("{} in {:?}", item.ident.to_string(), item.span);
                ret.push((body_id, (deny_panic_blocks, vec![function])));
            }
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
/// TODO we should be able to add deny_panic everywhere
///
fn get_deny_panic_in_expr<'tcx>(expr: &Expr<'tcx>, blocks: &mut Vec<&Block<'tcx>>) {
    match expr.kind {
        ExprKind::Block(block, None) => {
            if let Some(expr) = block.expr {
                match expr.kind {
                    ExprKind::Block(block, Some(label)) => {
                        if label.ident.as_str().contains("deny_panic") {
                            blocks.push(block);
                        }
                    }
                    _ => (),
                }
            }
        }
        _ => (),
    };
}

/// If it is solved call handle_solved_path
/// If not solve and call handle_solved_path
fn handle_qpath<'tcx>(
    hir_krate: &mut Map<'tcx>,
    qpath: QPath,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
) {
    match qpath {
        QPath::Resolved(_, path) => {
            if let Some(last) = path.segments.last() {
                match last.res {
                    Res::Def(def_kind, def_id) => {
                        let fn_ident = last.ident.as_str().to_string();
                        handle_solved_path(
                            hir_krate, def_kind, def_id, fn_ident, acc, tcx, &qpath, call_stack,
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
            let mut s = segment.hir_id.clone();
            s.local_id = local;
            let result = tcx.typeck(segment.hir_id.owner.def_id);
            match result.qpath_res(&qpath, s) {
                Res::Def(def_kind, def_id) => {
                    let path = format!("{:?}", def_id);
                    let path: Vec<&str> = path.split("::").collect();
                    handle_solved_path(
                        hir_krate,
                        def_kind,
                        def_id,
                        path.last().unwrap().to_string(),
                        acc,
                        tcx,
                        &qpath,
                        call_stack,
                    );
                }
                _ => todo!(),
            }
        }
        // TODO
        QPath::LangItem(_, _, _) => todo!(),
    }
}

/// If is local check it now
/// If is not save for later
/// If is a panic emit an error
fn handle_solved_path<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_kind: DefKind,
    def_id: DefId,
    fn_ident: String,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
    tcx: &mut TyCtxt<'tcx>,
    qpath: &QPath,
    call_stack: &mut Vec<String>,
) {
    let function = format!("{} in {:?}", fn_ident, qpath.span());
    call_stack.push(function);
    match def_kind {
        DefKind::Fn => {
            if let Some(local_id) = def_id.as_local() {
                let item = hir_krate.expect_item(local_id);
                if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
                    let expr = hir_krate.body(body_id).value;
                    get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
                }
            } else {
                let krate_name = tcx.crate_name(def_id.krate);
                if krate_name.to_string() == "std" && fn_ident == "begin_panic" {
                    eprintln!("OMG A PANIC");
                    for funtion in call_stack.clone() {
                        eprintln!("    {}", funtion);
                        eprintln!("");
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
        DefKind::AssocFn => handle_assoc_fn(hir_krate, def_id, acc, tcx, call_stack),
        // TODO
        _ => (),
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
) {
    if let Some(local_id) = def_id.as_local() {
        let item = hir_krate.expect_impl_item(local_id);
        if let rustc_hir::ImplItemKind::Fn(_, body_id) = item.kind {
            let expr = hir_krate.body(body_id).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
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
) {
    for stmt in block.stmts {
        get_panic_in_stmt(hir_krate, &stmt.kind, acc, tcx, call_stack);
    }
    if let Some(expr) = block.expr {
        get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
    }
}

fn get_panic_in_expr<'tcx>(
    hir_krate: &mut Map<'tcx>,
    expr_: &Expr<'tcx>,
    acc: &mut HashMap<String, Vec<(DefId, Vec<String>)>>,
    tcx: &mut TyCtxt<'tcx>,
    call_stack: &mut Vec<String>,
) {
    match expr_.kind {
        ExprKind::ConstBlock(const_block) => {
            let expr = hir_krate.body(const_block.body).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::Array(array) => {
            for expr in array {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
            }
        }
        ExprKind::Call(call, args) => {
            get_panic_in_expr(hir_krate, call, acc, tcx, call_stack);
            for expr in args {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
            }
        }
        ExprKind::MethodCall(method, receiver, args, _span) => {
            let result = tcx.typeck(receiver.hir_id.owner.def_id);
            let ty = result.expr_ty(receiver);
            let def_id = result
                .type_dependent_def_id(expr_.hir_id)
                .expect("ERROR: Can not get def id");
            let function = format!("{:?} in {:?}", method.ident, ty);
            call_stack.push(function);
            handle_assoc_fn(hir_krate, def_id, acc, tcx, call_stack);

            for expr in args {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
            }
        }
        ExprKind::Tup(tup) => {
            for expr in tup {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
            }
        }
        // TODO check if BinOp can panic
        ExprKind::Binary(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack);
        }
        // TODO check if UnOp can panic
        ExprKind::Unary(_, arg) => {
            get_panic_in_expr(hir_krate, arg, acc, tcx, call_stack);
        }
        ExprKind::Lit(_) => (),
        ExprKind::Cast(expr, _) => get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack),
        ExprKind::Type(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::DropTemps(expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::Let(let_) => {
            get_panic_in_expr(hir_krate, let_.init, acc, tcx, call_stack);
        }
        ExprKind::If(cond, if_block, Some(else_block)) => {
            get_panic_in_expr(hir_krate, cond, acc, tcx, call_stack);
            get_panic_in_expr(hir_krate, if_block, acc, tcx, call_stack);
            get_panic_in_expr(hir_krate, else_block, acc, tcx, call_stack);
        }
        ExprKind::If(cond, if_block, None) => {
            get_panic_in_expr(hir_krate, cond, acc, tcx, call_stack);
            get_panic_in_expr(hir_krate, if_block, acc, tcx, call_stack);
        }
        // TODO check if label is allow_panic
        ExprKind::Loop(block, _, _, _) => {
            get_panic_in_block(hir_krate, block, acc, tcx, call_stack);
        }
        ExprKind::Match(expr, arms, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
            for arm in arms {
                match arm.guard {
                    Some(Guard::If(expr)) => {
                        get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack)
                    }
                    Some(Guard::IfLet(let_)) => {
                        get_panic_in_expr(hir_krate, let_.init, acc, tcx, call_stack)
                    }
                    None => (),
                };
                get_panic_in_expr(hir_krate, arm.body, acc, tcx, call_stack);
            }
        }
        ExprKind::Closure(closure) => {
            let expr = hir_krate.body(closure.body).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::Block(block, Some(label)) => {
            if !label.ident.as_str().contains("allow_panic") {
                get_panic_in_block(hir_krate, block, acc, tcx, call_stack);
            } else {
                // TODO this is always printed!
                eprintln!("ATTENTION ALLOW PANIC IN A DEPENDENCY");
            }
        }
        ExprKind::Block(block, None) => {
            get_panic_in_block(hir_krate, block, acc, tcx, call_stack);
        }
        ExprKind::Assign(arg1, arg2, _) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack);
        }
        // TODO check if BinOp can panic
        ExprKind::AssignOp(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack);
        }
        ExprKind::Field(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::Index(arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx, call_stack);
            get_panic_in_expr(hir_krate, arg2, acc, tcx, call_stack);
        }
        ExprKind::Path(path) => handle_qpath(hir_krate, path, acc, tcx, call_stack),
        ExprKind::AddrOf(_, _, expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::Break(_, Some(expr)) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::Break(_, None) => (),
        ExprKind::Continue(_) => (),
        ExprKind::Ret(Some(expr)) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
        }
        ExprKind::Ret(None) => (),
        ExprKind::InlineAsm(_) => (),
        ExprKind::OffsetOf(_, _) => (),
        ExprKind::Struct(_, fields, Some(base)) => {
            get_panic_in_expr(hir_krate, base, acc, tcx, call_stack);
            for field in fields {
                get_panic_in_expr(hir_krate, field.expr, acc, tcx, call_stack);
            }
        }
        ExprKind::Struct(_, fields, None) => {
            for field in fields {
                get_panic_in_expr(hir_krate, field.expr, acc, tcx, call_stack);
            }
        }
        ExprKind::Repeat(elem, _) => {
            get_panic_in_expr(hir_krate, elem, acc, tcx, call_stack);
        }
        ExprKind::Yield(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
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
) {
    match stmt {
        StmtKind::Local(local) => {
            if let Some(expr) = local.init {
                get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
            }
            if let Some(block) = local.els {
                for stmt in block.stmts {
                    get_panic_in_stmt(hir_krate, &stmt.kind, acc, tcx, call_stack);
                }
                if let Some(expr) = block.expr {
                    get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack);
                }
            }
        }
        StmtKind::Item(_) => (),
        StmtKind::Expr(expr) => get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack),
        StmtKind::Semi(expr) => get_panic_in_expr(hir_krate, expr, acc, tcx, call_stack),
    }
}

pub fn config_from_args(args: &Vec<String>, sysroot: &Path) -> Config {
    let src_path = &get_location(&args).expect("ERROR: No location in args");
    let src_path = Path::new(src_path);
    let (externs, search_paths) = get_externs(&args);
    let edition = get_edition(&args);
    Config {
        opts: Options {
            maybe_sysroot: Some(sysroot.to_path_buf()),
            incremental: None,
            externs,
            edition,
            search_paths,
            target_triple: TargetTriple::TargetTriple("x86_64-unknown-linux-gnu".to_string()),
            crate_name: Some(get_crate_name(&args).expect("ERROR: No crate name in args")),
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
        registry: Registry::new(&rustc_error_codes::DIAGNOSTICS),
    }
}
