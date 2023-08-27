use colored::Colorize;
use std::process::Command;

fn main() {
    let porject_root = project_root::get_project_root().unwrap();
    let porject_root = porject_root.to_str().unwrap();

    Command::new("cargo")
        .args(["+nightly", "build", "-p", "unpanic"])
        .current_dir(porject_root)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    let unpanic_path = format!("{}/target/debug/unpanic", porject_root);
    Command::new("mv")
        .args([unpanic_path, porject_root.to_string()])
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    let unpanic_path = format!("{}/unpanic", porject_root);

    let check_test1_with_unpanic_out = Command::new("cargo")
        .args(["+nightly", "build", "-p", "test1_bin"])
        .current_dir(porject_root)
        .env("RUSTC_WRAPPER", unpanic_path.clone())
        .env("TARGET_CRATE", "test1_bin")
        .output()
        .unwrap();

    let mut check_test1_with_unpanic_stderr =
        String::from_utf8(check_test1_with_unpanic_out.stderr).unwrap();

    Command::new("rm")
        .args(["-r", format!("{}/target", porject_root).as_str()])
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    let check_test2_with_unpanic_out = Command::new("cargo")
        .args(["+nightly", "build", "-p", "test2-lib"])
        .current_dir(porject_root)
        .env("RUSTC_WRAPPER", &unpanic_path)
        .env("TARGET_CRATE", "test2-lib")
        .output()
        .unwrap();

    check_test1_with_unpanic_stderr.push_str(
        String::from_utf8(check_test2_with_unpanic_out.stderr)
            .unwrap()
            .as_str(),
    );

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
const TESTS: [(&str, &str, bool); 11] = [
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
        "test_if_see_panics_in_assoc_fn in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if ignore allow panic blocks",
        "allow_panic in tests/test1_bin/src/main.rs",
        false,
    ),
    (
        "check if ignore allow panic blocks 2",
        "function_test in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if handle traits 1",
        "ATTENTION ALLOW PANIC IN A DEPENDENCY",
        true,
    ),
    (
        "check if carte name that contains `-` are checked",
        "it_panic in tests/test2-lib/src/lib.rs",
        true,
    ),
    (
        "can check nested libs",
        "it_panic_nested in tests/nested-libs/nested-nested-lib/src/lib.rs",
        true,
    ),
    (
        "can check nested libs with feature",
        "it_panic_nested_feature in tests/nested-libs-feature/nested-nested-lib-feature/src/lib.rs",
        true,
    ),
    (
        "can check nested libs with macro",
        "it_panic_nested_macro in tests/test2-lib/src/lib.rs",
        true,
    ),
];
