use colored::Colorize;
use std::process::Command;

fn main() {
    let porject_root = project_root::get_project_root().unwrap();
    let porject_root = porject_root.to_str().unwrap();

    Command::new("cargo")
        .args(["build", "-p", "unpanic"])
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
        .args(["build", "-p", "test1_bin"])
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
        .args(["build", "-p", "test2-lib"])
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

    println!("\n{}", "TESTS: \n".green().bold());
    for (description, test, should_contain) in TESTS {
        let test = check_test1_with_unpanic_stderr.contains(test);
        let test_pass = matches!((test, should_contain), (true, true) | (false, false));
        if test_pass {
            println!("    {}{}", "Ok: ".green(), description);
        } else {
            eprintln!("    {}{}", "Error: ".red(), description);
        }
    }
}

/// (Test description, String to test, The string should or should not be in the output)
const TESTS: [(&str, &str, bool); 18] = [
    (
        "check if can see panics in function from external crates",
        "test_if_see_panics_in_imported_functions in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if can see panics in function from same crate",
        "test_if_see_panics_in_local_functions in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if can see panics methods calls",
        "test_if_see_panics_in_methods in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if can see panics assoc fn",
        "test_if_see_panics_in_assoc_fn in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "check if ignore allow panic blocks",
        "ATTENTION ALLOW PANIC",
        true,
    ),
    ("check if handle traits 1", "ATTENTION ALLOW PANIC", true),
    (
        "check if carte name that contains `-` are checked",
        "it_panic_2 in tests/test2-lib/src/lib.rs",
        true,
    ),
    (
        "can check nested libs",
        "it_panic_nested in tests/test2-lib/src/lib.rs",
        true,
    ),
    (
        "can check nested libs with feature",
        "it_panic_nested_feature in tests/test2-lib/src/lib.rs",
        true,
    ),
    (
        "can check nested libs with macro",
        "it_panic_nested_macro in tests/test2-lib/src/lib.rs",
        true,
    ),
    (
        "can check panics in closures",
        "check_closures in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "can check panics in non local methods 0",
        "test_if_see_panics_in_trait_0 in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "can check panics in non local methods 1",
        "test_if_see_panics_in_trait_1 in tests/test1_bin/src/main.rs",
        true,
    ),
    (
        "can deny_panic block in dependency",
        "deny_panic_in_dependency in tests/test1_lib/src/lib.rs",
        true,
    ),
    ("higher order function", "test_higher_order_fn_1", true),
    (
        "higher order function in differents crates",
        "test_higher_order_fn_different_crate_",
        true,
    ),
    (
        "higher order function in differents crates 2",
        "test_higher_order_fn_different_crate_2",
        true,
    ),
    (
        "higher order function with trait",
        "test_higher_order_with_trait_1",
        true,
    ),
];
