use crate::rustc_arg_handlers::*;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::Write;

// Write `carte-name args..` into ~/target/no-panic/deps
// carte name and each arg are separated by a space
// each line is a dependecy
pub fn write_args(args: Vec<String>, path_index: usize) {
    let mut crate_name = get_crate_name(&args).expect("ERROR: No crate name in rustc args");
    if crate_name == "build_script_build" {
        return;
    }
    crate_name.push_str(" ");
    let path = get_unpanic_path(&args, path_index).expect("ERROR: No unpanic path");
    let path = std::path::Path::new(&path);
    let serialized = serialize_args(args);
    crate_name.push_str(serialized.as_str());
    let row = crate_name;
    let parent_dir = path.parent().expect("ERROR: No parent dir");
    std::fs::create_dir_all(parent_dir).expect("ERROR: Impossible to create directory");
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap_or_else(|_| std::fs::File::create(path).unwrap());
    writeln!(file, "{}", row).expect("ERROR: Can not write to file");
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
    for line in std::io::BufReader::new(std::fs::File::open(path.clone()).expect(format!("ERRROR: Can not create PathBuf {:?}",path).as_str())).lines() {
        let line = line.expect("ERROR: No lines in deps file");
        let mut line = line.split_whitespace().map(|s| s.to_string());
        dep_map.insert(line.next().expect("ERROR: Invalid deps file format"), (None, line.collect::<Vec<String>>()));
    }
    std::fs::remove_file(path).expect("ERRROR: Impossible to remove deps path file");
    dep_map
}
