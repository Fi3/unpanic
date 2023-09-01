//! use rustc_interface::run_compiler to check the hir of the target crate and the dependency if
//! there is any forbidden panic. If there is it will report it.
use rustc_hir::def_id::DefId;
use rustc_hir::{HirId,ExprKind};
use rustc_interface::Config;
use rustc_middle::ty::AssocItem;
use rustc_middle::ty::Ty;
use rustc_middle::ty::TyCtxt;

use std::{collections::HashMap, path::PathBuf};

use crate::utils::config_from_args;
use crate::utils::{log_panic_in_deny_block,log_allow_panic};
use std::collections::VecDeque;

mod function_collectors;
mod function_handlers;
//mod path_handlers;
mod traversers;
use function_collectors::{get_function_for_dependency, get_functions};
use traversers::{FunctionCallPartialTree};

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
        }
    }

    pub fn start(&mut self) {
        let target_config = config_from_args(&self.target_args, &self.sysroot);
        self.check_crate(target_config, None);
        while !self.function_to_check.keys().is_empty() {
            for key in self.function_to_check.keys() {
                let to_check = self
                    .function_to_check
                    .remove(&key)
                    .expect("ERROR: No key in deps map");
                match key.as_str() {
                    "std" | "alloc" | "core" => (),
                    _ => {
                        let (_, dep_args) = self
                            .dep_map
                            .get_mut(&key)
                            .expect("ERROR: No key in deps map");
                        let target_config = config_from_args(dep_args, &self.sysroot);
                        self.check_crate(target_config, Some(to_check));
                    }
                };
            }
        }
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
        rustc_interface::run_compiler(target_config, |compiler| {
            compiler.enter(|queries| {
                queries
                    .global_ctxt()
                    .expect("ERROR: Can not get global context")
                    .enter(|mut tcx| {
                        let ids = match function_to_check {
                            Some(ids) => get_function_for_dependency(&mut tcx, ids),
                            None => get_functions(&mut tcx),
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
                            for block in &elem.1.0 {
                                traverser.traverse_block(block,&mut call_stack);
                            }
                            let mut to_log = vec![];
                            for (_,(def_id,fn_ident,call_stack)) in traverser.visited_functions.iter().filter(|x| x.0.is_extern()) {
                                function_handlers::check_fn_panics(
                                    *def_id,
                                    fn_ident.clone(),
                                    &mut tcx,
                                    &mut self.function_to_check,
                                    call_stack,
                                    &mut to_log,
                                );
                            }
                            for (_,(def_id,receiver,call_stack)) in traverser.visited_assoc_functions.iter().filter(|x| x.0.is_extern()) {
                                self.function_to_check.save_for_later_check(*def_id,&mut tcx,call_stack,*receiver);
                            }
                            for stack in to_log {
                                log_panic_in_deny_block(&stack);
                            }
                            for allow_panic in traverser.allow_panics {
                                log_allow_panic(&allow_panic);
                            }
                        }
                    })
            })
        });
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
