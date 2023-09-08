//! use rustc_interface::run_compiler to check the hir of the target crate and the dependency if
//! there is any forbidden panic. If there is it will report it.
use rustc_hir::def_id::DefId;
use rustc_hir::{Block, BodyId, ExprKind, HirId};
use rustc_interface::Config;
use rustc_middle::ty::AssocItem;
use rustc_middle::ty::Ty;
use rustc_middle::ty::TyCtxt;

use std::collections::HashSet;
use std::{collections::HashMap, path::PathBuf};

use crate::utils::config_from_args;
use crate::utils::{log_allow_panic, log_panic_in_deny_block};
use std::collections::VecDeque;

mod function_collectors;
mod function_handlers;
//mod path_handlers;
mod traversers;
use function_collectors::{
    get_all_fn_in_crate, get_callers, get_function_for_dependency, get_functions,
    get_procedural_parameters,
};
use traversers::FunctionCallPartialTree;

pub struct HirTraverser {
    pub errors: Vec<String>,
    pub function_to_check: ForeignCallsToCheck,
    //pub indirect_function_to_check: ForeignCallsToCheck,
    pub target_args: Vec<String>,
    pub dep_map: HashMap<
        /* krate name */ String,
        /*args:*/ (/* buildrs*/ Option<Vec<String>>, Vec<String>),
    >,
    pub sysroot: PathBuf,
    pub visited_functions: Vec<HirId>,
    pub deny_panic_procedural_parameters: HashMap<DefId, HashMap<usize, DefId>>,
    pub vistited_crates: HashSet<String>,
    pub to_log: Vec<Vec<String>>,
}

/// Given the target crate do:
/// 1. traverse the hir to get all the functions that contains a block labelled 'deny_panic inside
///    the body, for each of those function
/// 2. apply check function to each of those function
/// 3. recursivly apply check function to all the functions in the non local function map  
///
///
/// * check function do that:
///    1. check if the function contain a call to panic if so log it.
///    2. for all the local function call inside the function body apply check function to the
///       called function.
///    3. save all the non local function call inside the function body in a map
impl HirTraverser {
    pub fn new(
        target_args: Vec<String>,
        dep_map: HashMap<String, (Option<Vec<String>>, Vec<String>)>,
        sysroot: PathBuf,
    ) -> Self {
        Self {
            errors: Vec::new(),
            function_to_check: ForeignCallsToCheck::new(),
            target_args,
            dep_map,
            sysroot,
            visited_functions: Vec::new(),
            deny_panic_procedural_parameters: HashMap::new(),
            vistited_crates: HashSet::new(),
            to_log: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.dep_map.remove("std");
        self.dep_map.remove("alloc");
        self.dep_map.remove("core");
        self.dep_map.remove("getrandom");
        self.dep_map.remove("secp256k1");
        self.dep_map.remove("bitcoin_hashes");
        self.dep_map.remove("bitcoin");
        self.dep_map.remove("async-recursion");
        self.dep_map.remove("async_trait");
        //self.dep_map.remove("proc_macro");
        //self.dep_map.remove("proc_macro2");
        //self.dep_map.remove("tracing");
        //self.dep_map.remove("tracing_core");
        self.dep_map.remove("tracing_attributes");
        //self.dep_map.remove("once_cell");
        //self.dep_map.remove("cfg_if");
        //self.dep_map.remove("aes_gcm");
        let target_config = config_from_args(&self.target_args, &self.sysroot);
        self.check_crate(target_config, None);
        self.dep_map
            .insert("SELF".to_string(), (None, self.target_args.clone()));
        while !self.function_to_check.keys().is_empty() {
            dbg!("PRIMO CICLO");
            for crate_ in self.function_to_check.keys() {
                let to_check = self
                    .function_to_check
                    .remove(&crate_)
                    .expect("ERROR: No crate in deps map");
                match crate_.as_str() {
                    "std" | "alloc" | "core" => (),
                    _ => {
                        let (_, dep_args) = self
                            .dep_map
                            .get_mut(&crate_)
                            .expect("ERROR: No crate in deps map");
                        let target_config = config_from_args(dep_args, &self.sysroot);
                        self.vistited_crates.insert(crate_.to_string());
                        self.check_crate(target_config, Some(to_check));
                    }
                };
            }
        }
        // Make sure to check all the crates
        let mut not_checked_crates = vec![];
        for crate_name in self.dep_map.keys() {
            if !self.vistited_crates.contains(crate_name) {
                not_checked_crates.push(crate_name.clone());
            }
        }
        for crate_name in not_checked_crates {
            let (_, dep_args) = self
                .dep_map
                .get_mut(&crate_name)
                .expect("ERROR: No crate in deps map");
            match crate_name.as_str() {
                "std" | "alloc" | "core" => (),
                _ => {
                    let (_, dep_args) = self
                        .dep_map
                        .get_mut(&crate_name)
                        .expect("ERROR: No crate in deps map");
                    let target_config = config_from_args(dep_args, &self.sysroot);
                    self.vistited_crates.insert(crate_name.to_string());
                    self.check_crate(target_config, None);
                }
            };
        }
        self.second_pass();
    }

    /// For each function function to check call get_panic_in_block for the function block.
    /// This will call get_panic_in_stmt and get_panic_in_expr for each statement and expression in
    /// th block.
    #[allow(clippy::type_complexity)]
    fn check_crate(
        &mut self,
        target_config: Config,
        function_to_check: Option<Vec<(DefId, Vec<String>, Option<DefId>)>>,
    ) {
        dbg!("SECONDO CICLO");
        rustc_interface::run_compiler(target_config, |compiler| {
            compiler.enter(|queries| {
                queries
                    .global_ctxt()
                    .expect("ERROR: Can not get global context")
                    .enter(|mut tcx| {
                        let ids = match function_to_check {
                            Some(ids) => {
                                let called_functions_to_check =
                                    get_function_for_dependency(&mut tcx, ids);
                                let deny_panic_functions = get_functions(&mut tcx);
                                let mut ret = called_functions_to_check;
                                for (id, blocks) in deny_panic_functions {
                                    if !ret.iter().any(|el| el.0 == id) {
                                        ret.push((id, blocks.clone()));
                                    }
                                }
                                let procedural_parameters =
                                    get_procedural_parameters(&mut tcx, &ret);
                                self.deny_panic_procedural_parameters
                                    .extend(procedural_parameters);
                                ret
                            }
                            None => {
                                let ret = get_functions(&mut tcx);
                                let procedural_parameters =
                                    get_procedural_parameters(&mut tcx, &ret);
                                self.deny_panic_procedural_parameters
                                    .extend(procedural_parameters);
                                ret
                            }
                        };
                        for elem in &ids {
                            self.visited_functions = vec![];
                            let mut call_stack = elem.1 .1.clone();
                            let mut traverser = FunctionCallPartialTree {
                                tcx,
                                visited_functions: HashMap::new(),
                                visited_assoc_functions: HashMap::new(),
                                /// For each allow_panic that we encounter we save the call_stack
                                allow_panics: Vec::new(),
                                save_stack: true,
                                first_level_calls: Vec::new(),
                            };
                            for block in &elem.1 .0 {
                                traverser.traverse_block(block, &mut call_stack);
                            }
                            dbg!("BLOCCHI ATTAVERSATI");
                            for (_, (def_id, fn_ident, call_stack)) in traverser
                                .visited_functions
                                .iter()
                                .filter(|x| x.0.is_extern())
                            {
                                function_handlers::check_fn_panics(
                                    *def_id,
                                    fn_ident.clone(),
                                    &mut tcx,
                                    &mut self.function_to_check,
                                    call_stack,
                                    &mut self.to_log,
                                );
                            }
                            dbg!("PANIC ATTAVERSATI");
                            for (_, (def_id, receiver, call_stack)) in traverser
                                .visited_assoc_functions
                                .iter()
                                .filter(|x| x.0.is_extern())
                            {
                                self.function_to_check
                                    .save_for_later_check(*def_id, &mut tcx, call_stack, *receiver);
                            }
                            for allow_panic in traverser.allow_panics {
                                log_allow_panic(&allow_panic);
                            }
                        }
                    })
            })
        });
    }

    fn second_pass(&mut self) {
        for (k, (_, dep_args)) in self.dep_map.clone().iter() {
            dbg!(k);
            let target_config = config_from_args(dep_args, &self.sysroot);
            rustc_interface::run_compiler(target_config, |compiler| {
                compiler.enter(|queries| {
                    queries
                        .global_ctxt()
                        .expect("ERROR: Can not get global context")
                        .enter(|mut tcx| {
                            let all_fn = get_all_fn_in_crate(&mut tcx);
                            let callers = get_callers(
                                &mut tcx,
                                all_fn,
                                self.deny_panic_procedural_parameters.clone(),
                            );
                            let args_to_check = function_collectors::callers_into_args(callers);
                            for (arg, to_log, def_id) in args_to_check.iter() {
                                let arg = function_collectors::solve_arg(
                                    &mut tcx,
                                    arg.clone(),
                                    def_id.clone(),
                                );
                                self.visited_functions = vec![];
                                let mut traverser = FunctionCallPartialTree {
                                    tcx,
                                    visited_functions: HashMap::new(),
                                    visited_assoc_functions: HashMap::new(),
                                    /// For each allow_panic that we encounter we save the call_stack
                                    allow_panics: Vec::new(),
                                    save_stack: true,
                                    first_level_calls: Vec::new(),
                                };
                                let mut call_stack = vec![to_log.clone()];
                                traverser.traverse_expr(&arg, &mut call_stack);
                                for (_, (def_id, fn_ident, call_stack)) in traverser
                                    .visited_functions
                                    .iter()
                                    .filter(|x| x.0.is_extern())
                                {
                                    function_handlers::check_fn_panics(
                                        *def_id,
                                        fn_ident.clone(),
                                        &mut tcx,
                                        &mut self.function_to_check,
                                        call_stack,
                                        &mut self.to_log,
                                    );
                                }
                                for (_, (def_id, receiver, call_stack)) in traverser
                                    .visited_assoc_functions
                                    .iter()
                                    .filter(|x| x.0.is_extern())
                                {
                                    self.function_to_check
                                        .save_for_later_check(
                                            *def_id, &mut tcx, call_stack, *receiver,
                                        );
                                }
                                for allow_panic in traverser.allow_panics {
                                    log_allow_panic(&allow_panic);
                                }
                            }
                        });
                });
            });
        }
        // And finally check all the non local calls
        while !self.function_to_check.keys().is_empty() {
            dbg!("ULTIMO CICLO");
            for crate_ in self.function_to_check.keys() {
                let to_check = self
                    .function_to_check
                    .remove(&crate_)
                    .expect("ERROR: No crate in deps map");
                match crate_.as_str() {
                    "std" | "alloc" | "core" => (),
                    _ => {
                        let (_, dep_args) = self
                            .dep_map
                            .get_mut(&crate_)
                            .expect("ERROR: No crate in deps map");
                        let target_config = config_from_args(dep_args, &self.sysroot);
                        self.vistited_crates.insert(crate_.to_string());
                        self.check_crate(target_config, Some(to_check));
                    }
                };
            }
        }
        for stack in &self.to_log {
            log_panic_in_deny_block(&stack);
        }
        eprintln!("FINISH!!!");
    }
}

fn get_impl_item<'tcx>(
    tcx: &mut TyCtxt<'tcx>,
    trait_fn_def_id: DefId,
    receiver: Option<Ty<'tcx>>,
) -> Option<AssocItem> {
    let receiver = receiver?;
    let trait_def_id = tcx.parent(trait_fn_def_id);
    let trait_fn_name = tcx.item_name(trait_fn_def_id);
    for impl_def_id in tcx.all_local_trait_impls(()).get(&trait_def_id)? {
        let implementor_type = tcx.type_of(impl_def_id.to_def_id()).skip_binder();
        if implementor_type == receiver {
            for impl_item in tcx
                .associated_items(impl_def_id.to_def_id())
                .in_definition_order()
            {
                if impl_item.name == trait_fn_name {
                    return Some(*impl_item);
                }
            }
        }
    }
    None
    //panic!("Impossible to find trait implementation")
}


#[derive(Debug)]
pub struct ForeignCallsToCheck {
    #[allow(clippy::type_complexity)]
    inner: HashMap<
        /* crate_name */ String,
        Vec<(
            /* call to check */ DefId,
            /* call stack that leads to call to check */ Vec<String>,
            /* optional receiving type */ Option<DefId>,
        )>,
    >,
}

impl Clone for ForeignCallsToCheck {
    fn clone(&self) -> Self {
        ForeignCallsToCheck {
            inner: self.inner.clone(),
        }
    }
}

impl ForeignCallsToCheck {
    pub fn new() -> Self {
        ForeignCallsToCheck {
            inner: HashMap::new(),
        }
    }

    pub fn save_for_later_check(
        &mut self,
        def_id: DefId,
        tcx: &mut TyCtxt<'_>,
        call_stack: &[String],
        receiver: Option<DefId>,
    ) {
        let krate_name = tcx.crate_name(def_id.krate);
        if let Some(functions) = self.inner.get_mut(&krate_name.to_string()) {
            functions.push((def_id, call_stack.to_owned(), receiver));
        } else {
            self.inner.insert(
                krate_name.to_string(),
                vec![(def_id, call_stack.to_owned(), receiver)],
            );
        }
    }

    pub fn keys(&self) -> Vec<String> {
        self.inner.keys().map(|s| s.to_string()).collect()
    }

    #[allow(clippy::type_complexity)]
    pub fn remove(
        &mut self,
        key: &String,
    ) -> Option<Vec<(DefId, Vec<std::string::String>, Option<DefId>)>> {
        self.inner.remove(key)
    }
}
