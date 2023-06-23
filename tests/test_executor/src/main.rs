use std::process::Command;
use colored::Colorize;

fn main() {
    let porject_root = project_root::get_project_root().unwrap();
    let porject_root = porject_root.to_str().unwrap();

    Command::new("cargo")
        .args(["+nightly", "build", "-p", "unpanic"])
        .current_dir(porject_root)
        .spawn()
        .unwrap();
    let unpanic_path = format!("{}/target/debug/unpanic", porject_root);

    let check_test1_with_unpanic_out = Command::new("cargo").args(["+nightly", "build", "-p", "test1_bin"])
        .current_dir(porject_root)
        .env("RUSTC_WRAPPER", unpanic_path)
        .env("TARGET_CRATE", "test1_bin")
        .output()
        .unwrap();

    let check_test1_with_unpanic_stderr = String::from_utf8(check_test1_with_unpanic_out.stderr).unwrap();

    println!("");
    println!("{}", "TESTS: \n".green().bold());
    for (description, test, should_contain) in TESTS {
        let test = check_test1_with_unpanic_stderr.contains(test);
        let test_pass = match (test, should_contain) {
            (true, true) => true,
            (false, false) => true,
            _ => false,
        };
        if test_pass {
            println!("    {}{}", "Ok: ".green(), description);
        } else {
            eprintln!("    {}{}", "Error: ".red(), description);
        }
    }
}

/// (Test description, String to test, The string should or should not be in the output)
const TESTS: [(&str, &str, bool); 6] = [
    (
        "check if can see panics in function from external crates",
        "function_test in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if can see panics in function from same crate",
        "same_crate in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if can see panics methods calls",
        "method_test#0 in test1_lib::method_test::MethodTest",
        true,
    ),
    (
        "check if can see panics assoc fn",
        "assoc_fn) in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if ignore allow panic blocks",
        "allow_panic in tests/test1_bin/src/main.rs",
        false,
    ),
    (
        "check if ignore allow panic blocks 2",
        "ATTENTION ALLOW PANIC IN A DEPENDENCY",
        true,
    ),
];
