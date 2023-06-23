/// Call a function that can panic imported from test1_lib
/// When checked by unpanic it should report an error
extern crate test1_lib;

use test1_lib::function_test::function_test;
use test1_lib::method_test::MethodTest;
use test1_lib::allow_panic_test::allow_panic;

fn main() {
}

#[allow(dead_code)]
fn test_if_see_panics_in_imported_functions() {
    #[allow(unused_labels)]
    'deny_panic: {
        function_test();
    }
}

#[allow(dead_code)]
fn test_if_see_panics_in_local_functions() {
    #[allow(unused_labels)]
    'deny_panic: {
        same_crate();
    }
}

#[allow(dead_code)]
fn test_if_see_panics_in_methods() {
    #[allow(unused_labels)]
    'deny_panic: {
        let a = MethodTest;
        a.method_test();
    }
}

#[allow(dead_code)]
fn test_if_see_panics_in_assoc_fn() {
    #[allow(unused_labels)]
    'deny_panic: {
        MethodTest::assoc_fn();
    }
}

#[allow(dead_code)]
fn test_if_ingnore_panic_in_allow_block() {
    #[allow(unused_labels)]
    'deny_panic: {
        allow_panic()
    }
}

#[allow(dead_code)]
fn same_crate() {
    panic!()
}

// TODO
//#[allow(dead_code)]
//#[allow(unconditional_recursion)]
//fn do_not_stuck_on_loops() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        do_not_stuck_on_loops()
//    }
//}
