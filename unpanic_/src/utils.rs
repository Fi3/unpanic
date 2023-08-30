use rustc_driver::DEFAULT_LOCALE_RESOURCES;
use rustc_errors::registry::Registry;
use rustc_hash::{FxHashMap, FxHashSet};
use rustc_interface::Config;
use rustc_session::config::*;
use rustc_target::spec::TargetTriple;
use std::path::Path;

use crate::rustc_arg_handlers::*;

/// Given cargo args create a Config for run_compiler
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

pub fn log_panic_in_deny_block(call_stack: &[String]) {
    eprintln!("OMG A PANIC");
    for funtion in call_stack {
        eprintln!("    {}\n", funtion);
    }
}

pub fn log_allow_panic(call_stack: &[String]) {
    eprintln!("ATTENTION ALLOW PANIC");
    for funtion in call_stack {
        eprintln!("    {}\n", funtion);
    }
}
