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
    crate_name.push(' ');
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
    args: &[String],
    index: Option<usize>,
) -> std::collections::HashMap<String, (/* build.rs */ Option<Vec<String>>, Vec<String>)> {
    let path = match index {
        Some(i) => get_unpanic_path(args, i).expect("ERROR MESSAGE"),
        None => "./target/no-panic/deps".to_string(),
    };
    let mut dep_map = std::collections::HashMap::new();
    // At this point this file must exist
    for line_ in
        std::io::BufReader::new(std::fs::File::open(path.clone()).unwrap_or(
            std::fs::File::open("/dev/null").expect("ERROR: Impossible to open dev null"),
        ))
        .lines()
    {
        let line_ = line_.expect("ERROR: No lines in deps file");
        if line_ == "" {
            continue;
        }
        let mut line = line_.split_whitespace().map(|s| s.to_string());
        dep_map.insert(
            line.next().expect(format!("ERROR: Invalid deps file format \n {:#?} \n {:?}", args, line_).as_str()),
            (None, line.collect::<Vec<String>>()),
        );
    }
    let _ = std::fs::remove_file(path);
    dep_map
}
