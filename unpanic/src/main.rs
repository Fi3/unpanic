#![feature(rustc_private, stmt_expr_attributes)]
#![feature(string_remove_matches)]
#![feature(exact_size_is_empty)]
#![feature(iter_next_chunk)]

use std::collections::HashMap;
use std::process::Command;

extern crate alloc;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_error_codes;
extern crate rustc_errors;
extern crate rustc_hash;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;

use std::path::PathBuf;

use rustc_driver::Compilation;
use rustc_interface::interface::Config;

struct Callbacks;

mod dep_handler;
mod errors;
mod hir_traverser;
mod rustc_arg_handlers;
use dep_handler::*;
use hir_traverser::*;
use rustc_arg_handlers::*;

impl rustc_driver::Callbacks for Callbacks {
    fn config(&mut self, _config: &mut Config) {}

    fn after_analysis<'tcx>(
        &mut self,
        _: &rustc_interface::interface::Compiler,
        _q: &'tcx rustc_interface::Queries<'tcx>,
    ) -> Compilation {
        Compilation::Continue
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    rustc_driver::RunCompiler::new(&args[1..], &mut Callbacks)
        .run()
        .expect("ERROR: Fail to compile");
    if have_arg(&args, "--print=cfg") {
        return;
    };
    if is_dependency(&args) {
        if let Ok(target_path_index) = get_target_path_index(&args) {
            write_args(args, target_path_index);
        }
    } else {
        let index = get_target_path_index(&args).ok();
        let dep_map = parse_deps_args(&args, index);
        let out = Command::new("rustc")
            .arg("--print=sysroot")
            .current_dir(".")
            .output()
            .expect("ERROR: Impossible to call rustc in current directory");
        let sysroot = std::str::from_utf8(&out.stdout)
            .expect("ERROR: Can not retreive sysroot")
            .trim();
        let sysroot = PathBuf::from(sysroot);
        let mut traverser = HirTraverser {
            errors: Vec::new(),
            function_to_check: HashMap::new(),
            target_args: args,
            dep_map,
            sysroot,
        };
        traverser.start();
    }
}
