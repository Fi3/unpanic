use crate::errors::Error;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::Write;
use crate::rustc_arg_handlers::*;

// Write `carte-name args..` into ~/target/no-panic/deps
// carte name and each arg are separated by a space
// each line is a dependecy
pub fn write_args(args: Vec<String>, path_index: usize) {
    let mut crate_name = get_crate_name(&args).expect("ERROR MESSAGE");
    if crate_name == "build_script_build" {
        return;
    }
    crate_name.push_str(" ");
    let path = get_unpanic_path(&args, path_index).expect("ERROR MESSAGE");
    let path = std::path::Path::new(&path);
    let serialized = serialize_args(args);
    crate_name.push_str(serialized.as_str());
    let row = crate_name;
    let parent_dir = path.parent().unwrap();
    std::fs::create_dir_all(parent_dir).expect("ERROR MESSAGE");
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap_or_else(|_| std::fs::File::create(path).unwrap());
    writeln!(file, "{}", row).expect("ERROR MESSAGE");
}

pub fn serialize_args(args: Vec<String>) -> String {
    let mut x = format!("{:?}", args);
    x.remove_matches("[");
    x.remove_matches("]");
    x.remove_matches('"');
    x.remove_matches(",");
    x
}

// Return an hashmap with the dep name as key and the rustc args for that dep as args
pub fn parse_deps_args(
    args: &Vec<String>,
    index: Option<usize>,
) -> std::collections::HashMap<String, (/* build.rs */ Option<Vec<String>>, Vec<String>)> {
    let path = match index {
        Some(i) => get_unpanic_path(&args, i).expect("ERROR MESSAGE"),
        None => "./target/no-panic/deps".to_string(),
    };
    let mut dep_map = std::collections::HashMap::new();
    // At this point this file must exist
    for line in std::io::BufReader::new(std::fs::File::open(path.clone()).unwrap()).lines() {
        let line = line.unwrap();
        let mut line = line.split_whitespace().map(|s| s.to_string());
        dep_map.insert(line.next().unwrap(), (None, line.collect::<Vec<String>>()));
    }
    std::fs::remove_file(path).unwrap();
    dep_map
}
