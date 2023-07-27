use alloc::collections::{btree_map::BTreeMap, btree_set::BTreeSet};
use rustc_session::{
    config::{ExternEntry, ExternLocation, Externs},
    search_paths::{PathKind, SearchPath, SearchPathFile},
    utils::CanonicalizedPath,
};
use rustc_span::edition::Edition;
use std::path::{Path, PathBuf};

use crate::errors::Error;
use std::str::FromStr;

/// Get the args from cargo and return the path for the sources
pub fn get_location(args: &Vec<String>) -> Result<String, Error> {
    let mut args = args.iter();
    if let Some(_) = args.position(|s| s == "--crate-name") {
        args.next().ok_or(Error::SrcLocationMissing)?;
        args.next().ok_or(Error::SrcLocationMissing)?;
        args.next().ok_or(Error::SrcLocationMissing).cloned()
    } else {
        Err(Error::SrcLocationMissing)
    }
}

// TODO for now it support only one search_path
pub fn get_externs(args: &Vec<String>) -> (Externs, Vec<SearchPath>) {
    let path_dir = get_dep_path(args);
    let mut dep_map = BTreeMap::new();
    let externs_arg = _get_externs(args);
    let mut search_path = SearchPath {
        kind: PathKind::All,
        dir: PathBuf::from_str(&path_dir).expect("ERROR: Invalid path_dir"),
        files: vec![],
    };
    for arg in externs_arg {
        let splitted = arg
            .split('=')
            .next_chunk::<2>()
            .expect("ERROR: Invalid args format");
        let name = dbg!(splitted[0]);
        let path_str = dbg!(splitted[1]);

        let path = Path::new(&path_str);

        let file = SearchPathFile {
            path: path.to_path_buf(),
            file_name_str: path
                .file_name()
                .expect("ERROR: No file name in path")
                .to_str()
                .expect("ERROR: Invalid file name in path")
                .to_string(),
        };
        search_path.files.push(file);

        dep_map.insert(
            name.to_string(),
            ExternEntry {
                location: ExternLocation::ExactPaths(BTreeSet::from([CanonicalizedPath::new(
                    path,
                )])),
                is_private_dep: false,
                add_prelude: true,
                nounused_dep: false,
                force: false,
            },
        );
    }
    dbg!(&dep_map);
    let externs = Externs::new(dep_map);
    (externs, vec![search_path])
}

fn get_arg(args: &Vec<String>, arg_name: &str) -> Vec<String> {
    args.iter()
        .filter(|s| s.contains(arg_name) && *s != arg_name)
        .map(|s| {
            s.split("=")
                .next_chunk::<2>()
                .expect(format!("ERROR: Can not get {} in args", arg_name).as_str())[1]
                .to_string()
        })
        .collect()
}
pub fn have_arg(args: &Vec<String>, arg_name: &str) -> bool {
    for arg in args {
        if arg == arg_name {
            return true;
        }
    }
    false
}

pub fn get_edition(args: &Vec<String>) -> Edition {
    let edition = dbg!(get_arg(args, "--edition="));
    match edition.len() {
        0 => panic!("Must specify edition"),
        1 => match edition[0].as_str() {
            "2015" => Edition::Edition2015,
            "2018" => Edition::Edition2018,
            "2021" => Edition::Edition2021,
            _ => panic!("Unsupported edition {}", edition[0].as_str()),
        },
        _ => panic!("Can not use more then one edition"),
    }
}
// TODO for now it support only one search_path
fn get_dep_path(args: &Vec<String>) -> String {
    let paths = get_arg(args, "edition=");
    match paths.len() {
        1 => paths[0].clone(),
        _ => todo!(),
    }
}

fn _get_externs(args: &Vec<String>) -> Vec<String> {
    let mut externs = vec![];
    for (i, arg) in args.iter().enumerate() {
        if arg == "--extern" {
            externs.push(args[i + 1].clone());
        }
    }
    externs
}

pub fn get_crate_name(args: &Vec<String>) -> Result<String, Error> {
    let mut args = args.iter();
    if let Some(_) = args.position(|s| s == "--crate-name") {
        args.next().ok_or(Error::CrateNameMissing).cloned()
    } else {
        Err(Error::CrateNameMissing)
    }
}

pub fn is_dependency(args: &Vec<String>) -> bool {
    //dbg!(args);
    let target_crate = std::env::var("TARGET_CRATE").expect("ERROR: Env var TARGET_CRATE not set");
    let crate_name = get_crate_name(&args).expect("ERROR: Can not get crate name");
    target_crate != crate_name
}

pub fn get_target_path_index(args: &Vec<String>) -> Result<usize, Error> {
    if let Some(i) = args.iter().position(|s| s == "--out-dir") {
        if args.len() - 1 >= i + 1 {
            Ok(i + 1)
        } else {
            Err(Error::TargetPathMissing)
        }
    } else {
        Err(Error::TargetPathMissing)
    }
}
fn is_valid_out_dir(_out_dir: &String) -> bool {
    true
}

// Get the path where we save dependecy name and rustc arg for that crate
pub fn get_unpanic_path(args: &Vec<String>, index: usize) -> Result<String, Error> {
    let out_dir = &args[index];
    if is_valid_out_dir(out_dir) {
        let mut splitted = out_dir.split("target/");
        // already checked for path validity
        let mut head = splitted
            .next()
            .expect("ERROR: Can not split out dir")
            .to_string();
        let tail = splitted.next().expect("ERROR: Can not split out dir");
        head.push_str("target/no-panic/");
        head.push_str(tail);
        head.remove_matches("/debug");
        head.remove_matches("/release");
        head.remove_matches("/build");
        head.remove_matches("/x86_64-unknown-linux-gnu");
        Ok(head)
    } else {
        Err(Error::InvalidOutDir)
    }
}
