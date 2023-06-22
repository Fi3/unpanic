use rustc_hir::{
    def::DefKind, def::Res, def_id::DefId, Block, BodyId, Expr, ExprKind, Guard, Node, QPath,
    StmtKind,
};
use rustc_hir::{def_id::LOCAL_CRATE, hir_id::ItemLocalId};
use rustc_interface::Config;
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;
use std::{collections::HashMap, path::PathBuf};
use rustc_driver::DEFAULT_LOCALE_RESOURCES;
use rustc_errors::registry::Registry;
use rustc_hash::{FxHashMap, FxHashSet};
use rustc_session::config::*;
use rustc_target::spec::TargetTriple;
use std::path::Path;

use crate::rustc_arg_handlers::*;

pub struct HirTraverser {
    pub errors: Vec<String>,
    pub function_to_check:
        HashMap</* krate name */ String, /* function path */ Vec<DefId>>,
    pub target_args: Vec<String>,
    pub dep_map: HashMap<
        /* krate name */ String,
        /*args:*/ (/* buildrs*/ Option<Vec<String>>, Vec<String>),
    >,
    pub sysroot: PathBuf,
}

impl HirTraverser {
    pub fn start(&mut self) {
        let target_config = config_from_args(dbg!(&self.target_args), &self.sysroot);
        self.check_crate(target_config, None);
        while !self.function_to_check.keys().is_empty() {
            let keys = self.function_to_check.clone();
            let keys = keys.keys();
            for key in keys {
                let to_check = self.function_to_check.remove(key).unwrap();
                match key.as_str() {
                    "std" | "alloc" | "core" => continue,
                    _ => {
                        let (_, dep_args) = self.dep_map.get_mut(key).expect("ERROR MESSAGE");
                        let target_config = config_from_args(&dep_args, &self.sysroot);
                        self.check_crate(target_config, Some(to_check));
                    }
                };
            }
        }
    }

    fn check_crate(&mut self, target_config: Config, function_to_check: Option<Vec<DefId>>) {
        rustc_interface::run_compiler(target_config, |compiler| {
            compiler.enter(|queries| {
                queries.global_ctxt().unwrap().enter(|mut tcx| {
                    if !tcx.sess.rust_2018() {
                        panic!("Rust 2018 is required");
                    }
                    let ids = function_to_check
                        .map(|ids| get_function_for_dependency(&mut tcx.hir(), ids))
                        .unwrap_or(get_functions(&mut tcx.hir()));
                    for block in &ids {
                        let block = block.1[0];
                        get_panic_in_block(
                            &mut tcx.hir(),
                            block,
                            &mut self.function_to_check,
                            &mut tcx,
                        );
                    }
                })
            })
        });
    }
}
fn get_function_for_dependency<'tcx>(
    hir_krate: &mut Map<'tcx>,
    ids: Vec<DefId>,
) -> Vec<(BodyId, Vec<&'tcx Block<'tcx>>)> {
    let mut ret = vec![];
    for mut id in ids {
        id.krate = LOCAL_CRATE;
        let fn_body_id = match hir_krate.get_if_local(id).expect("ERROR MESSAGE") {
            Node::Item(item) => item.expect_fn().2,
            Node::ImplItem(item) => item.expect_fn().1,
            _ => todo!(),
        };
        match hir_krate.body(fn_body_id).value.kind {
            ExprKind::Block(block, Some(label)) => {
                if label.ident.as_str().contains("allow_panic") {
                    println!("ATTENTION ALLOW PANIC IN A DEPENDENCY");
                    continue;
                } else {
                    ret.push((fn_body_id, vec![block]));
                }
            }
            ExprKind::Block(block, None) => ret.push((fn_body_id, vec![block])),
            _ => todo!(),
        }
    }
    ret
}

/// Traverse the crate and return all the functions that contains `deny_panic blocks and the blocks
fn get_functions<'tcx>(hir_krate: &mut Map<'tcx>) -> Vec<(BodyId, Vec<&'tcx Block<'tcx>>)> {
    let mut ret = vec![];
    for item_id in hir_krate.items() {
        let item = hir_krate.item(item_id);
        if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
            let mut deny_panic_blocks = vec![];
            let expr = hir_krate.body(body_id).value;
            get_deny_panic_in_expr(expr, &mut deny_panic_blocks);
            if !deny_panic_blocks.is_empty() {
                ret.push((body_id, deny_panic_blocks));
            }
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

/// If is local check it now
/// If is not save for later
fn handle_qpath<'tcx>(
    hir_krate: &mut Map<'tcx>,
    qpath: QPath,
    acc: &mut HashMap<String, Vec<DefId>>,
    tcx: &mut TyCtxt<'tcx>,
) {
    match qpath {
        QPath::Resolved(_, path) => {
            if let Some(last) = path.segments.last() {
                match last.res {
                    Res::Def(def_kind, def_id) => {
                        let fn_ident = last.ident.as_str().to_string();
                        handle_solved_path(hir_krate, def_kind, def_id, fn_ident, acc, tcx, &qpath);
                    }
                    _ => todo!(),
                }
            }
        }
        // TODO
        QPath::TypeRelative(_, segment) => {
            // TODO this `-3` work and I donno why!
            let local = ItemLocalId::from_usize(segment.hir_id.local_id.as_usize() - 3);
            let mut s = segment.hir_id.clone();
            s.local_id = local;
            let result = tcx.typeck(segment.hir_id.owner.def_id);
            match result.qpath_res(&qpath, s) {
                Res::Def(def_kind, def_id) => {
                    handle_solved_path(
                        hir_krate,
                        def_kind,
                        def_id,
                        "".to_string(),
                        acc,
                        tcx,
                        &qpath,
                    );
                }
                _ => todo!(),
            }
        }
        // TODO
        QPath::LangItem(_, _, _) => todo!(),
    }
}

fn handle_solved_path<'tcx>(
    hir_krate: &mut Map<'tcx>,
    def_kind: DefKind,
    def_id: DefId,
    fn_ident: String,
    acc: &mut HashMap<String, Vec<DefId>>,
    tcx: &mut TyCtxt<'tcx>,
    _qpath: &QPath,
) {
    // TODO this should happen only if we are on std
    //if fn_ident.contains("panic") {
    //    println!("OMG A PANIC {:?}", fn_ident);
    //};
    match def_kind {
        DefKind::Fn => {
            if let Some(local_id) = def_id.as_local() {
                let item = hir_krate.expect_item(local_id);
                if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
                    let expr = hir_krate.body(body_id).value;
                    get_panic_in_expr(hir_krate, expr, acc, tcx);
                }
            } else {
                let krate_name = tcx.crate_name(def_id.krate);
                if let Some(functions) = acc.get_mut(&krate_name.to_string()) {
                    functions.push(def_id);
                } else {
                    if krate_name.to_string() == "std" && fn_ident == "begin_panic" {
                        println!("OMG A PANIC {:?}", fn_ident);
                    }
                    acc.insert(krate_name.to_string(), vec![def_id]);
                }
            }
        }
        DefKind::AssocFn => {
            if let Some(local_id) = def_id.as_local() {
                let item = hir_krate.expect_impl_item(local_id);
                if let rustc_hir::ImplItemKind::Fn(_, body_id) = item.kind {
                    let expr = hir_krate.body(body_id).value;
                    get_panic_in_expr(hir_krate, expr, acc, tcx);
                }
            } else {
                let krate_name = tcx.crate_name(def_id.krate);
                if let Some(functions) = acc.get_mut(&krate_name.to_string()) {
                    functions.push(def_id);
                } else {
                    acc.insert(krate_name.to_string(), vec![def_id]);
                }
            }
        }
        // TODO
        _ => (), //{dbg!("TODO",def_kind);},
    }
}

fn get_panic_in_block<'tcx>(
    hir_krate: &mut Map<'tcx>,
    block: &Block<'tcx>,
    acc: &mut HashMap<String, Vec<DefId>>,
    tcx: &mut TyCtxt<'tcx>,
) {
    for stmt in block.stmts {
        get_panic_in_stmt(hir_krate, &stmt.kind, acc, tcx);
    }
    if let Some(expr) = block.expr {
        get_panic_in_expr(hir_krate, expr, acc, tcx);
    }
}

fn get_panic_in_expr<'tcx>(
    hir_krate: &mut Map<'tcx>,
    expr_: &Expr<'tcx>,
    acc: &mut HashMap<String, Vec<DefId>>,
    tcx: &mut TyCtxt<'tcx>,
) {
    match expr_.kind {
        ExprKind::ConstBlock(const_block) => {
            let expr = hir_krate.body(const_block.body).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Array(array) => {
            for expr in array {
                get_panic_in_expr(hir_krate, expr, acc, tcx);
            }
        }
        ExprKind::Call(call, args) => {
            get_panic_in_expr(hir_krate, call, acc, tcx);
            for expr in args {
                get_panic_in_expr(hir_krate, expr, acc, tcx);
            }
        }
        ExprKind::MethodCall(_, call, args, _) => {
            // TODO check if works
            get_panic_in_expr(hir_krate, call, acc, tcx);
            for expr in args {
                get_panic_in_expr(hir_krate, expr, acc, tcx);
            }
        }
        ExprKind::Tup(tup) => {
            for expr in tup {
                get_panic_in_expr(hir_krate, expr, acc, tcx);
            }
        }
        // TODO check if BinOp can panic
        ExprKind::Binary(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx);
            get_panic_in_expr(hir_krate, arg2, acc, tcx);
        }
        // TODO check if UnOp can panic
        ExprKind::Unary(_, arg) => {
            get_panic_in_expr(hir_krate, arg, acc, tcx);
        }
        ExprKind::Lit(_) => (),
        ExprKind::Cast(expr, _) => get_panic_in_expr(hir_krate, expr, acc, tcx),
        ExprKind::Type(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::DropTemps(expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Let(let_) => {
            get_panic_in_expr(hir_krate, let_.init, acc, tcx);
        }
        ExprKind::If(cond, if_block, Some(else_block)) => {
            get_panic_in_expr(hir_krate, cond, acc, tcx);
            get_panic_in_expr(hir_krate, if_block, acc, tcx);
            get_panic_in_expr(hir_krate, else_block, acc, tcx);
        }
        ExprKind::If(cond, if_block, None) => {
            get_panic_in_expr(hir_krate, cond, acc, tcx);
            get_panic_in_expr(hir_krate, if_block, acc, tcx);
        }
        // TODO check if label is allow_panic
        ExprKind::Loop(block, _, _, _) => {
            get_panic_in_block(hir_krate, block, acc, tcx);
        }
        ExprKind::Match(expr, arms, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
            for arm in arms {
                match arm.guard {
                    Some(Guard::If(expr)) => get_panic_in_expr(hir_krate, expr, acc, tcx),
                    Some(Guard::IfLet(let_)) => get_panic_in_expr(hir_krate, let_.init, acc, tcx),
                    None => (),
                };
                get_panic_in_expr(hir_krate, arm.body, acc, tcx);
            }
        }
        ExprKind::Closure(closure) => {
            let expr = hir_krate.body(closure.body).value;
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Block(block, Some(label)) => {
            if !label.ident.as_str().contains("allow_panic") {
                get_panic_in_block(hir_krate, block, acc, tcx);
            }
        }
        ExprKind::Block(block, None) => {
            get_panic_in_block(hir_krate, block, acc, tcx);
        }
        ExprKind::Assign(arg1, arg2, _) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx);
            get_panic_in_expr(hir_krate, arg2, acc, tcx);
        }
        // TODO check if BinOp can panic
        ExprKind::AssignOp(_, arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx);
            get_panic_in_expr(hir_krate, arg2, acc, tcx);
        }
        ExprKind::Field(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Index(arg1, arg2) => {
            get_panic_in_expr(hir_krate, arg1, acc, tcx);
            get_panic_in_expr(hir_krate, arg2, acc, tcx);
        }
        ExprKind::Path(path) => handle_qpath(hir_krate, path, acc, tcx),
        ExprKind::AddrOf(_, _, expr) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Break(_, Some(expr)) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Break(_, None) => (),
        ExprKind::Continue(_) => (),
        ExprKind::Ret(Some(expr)) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Ret(None) => (),
        ExprKind::InlineAsm(_) => (),
        ExprKind::OffsetOf(_, _) => (),
        ExprKind::Struct(_, fields, Some(base)) => {
            get_panic_in_expr(hir_krate, base, acc, tcx);
            for field in fields {
                get_panic_in_expr(hir_krate, field.expr, acc, tcx);
            }
        }
        ExprKind::Struct(_, fields, None) => {
            for field in fields {
                get_panic_in_expr(hir_krate, field.expr, acc, tcx);
            }
        }
        ExprKind::Repeat(elem, _) => {
            get_panic_in_expr(hir_krate, elem, acc, tcx);
        }
        ExprKind::Yield(expr, _) => {
            get_panic_in_expr(hir_krate, expr, acc, tcx);
        }
        ExprKind::Err(_) => panic!(),
    }
}

fn get_panic_in_stmt<'tcx>(
    hir_krate: &mut Map<'tcx>,
    stmt: &StmtKind<'tcx>,
    acc: &mut HashMap<String, Vec<DefId>>,
    tcx: &mut TyCtxt<'tcx>,
) {
    match stmt {
        StmtKind::Local(local) => {
            if let Some(expr) = local.init {
                get_panic_in_expr(hir_krate, expr, acc, tcx);
            }
            if let Some(block) = local.els {
                for stmt in block.stmts {
                    get_panic_in_stmt(hir_krate, &stmt.kind, acc, tcx);
                }
                if let Some(expr) = block.expr {
                    get_panic_in_expr(hir_krate, expr, acc, tcx);
                }
            }
        }
        StmtKind::Item(_) => (),
        StmtKind::Expr(expr) => get_panic_in_expr(hir_krate, expr, acc, tcx),
        StmtKind::Semi(expr) => get_panic_in_expr(hir_krate, expr, acc, tcx),
    }
}

pub fn config_from_args(args: &Vec<String>, sysroot: &Path) -> Config {
    let src_path = &get_location(&args).expect("ERROR MESSAGE");
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
            crate_name: Some(get_crate_name(&args).unwrap()),
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

