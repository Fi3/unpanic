/// Call a function that can panic imported from test1_lib
/// When checked by unpanic it should report an error
use test1_lib::allow_panic_test::allow_panic;
use test1_lib::function_test::function_test;
use test1_lib::method_test::{MethodTest, Trait};

fn main() {}

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
fn test_if_see_panics_in_trait_0() {
    #[allow(unused_labels)]
    'deny_panic: {
        let a = MethodTest;
        a.trait_0();
    }
}

#[allow(dead_code)]
fn test_if_see_panics_in_trait_1() {
    #[allow(unused_labels)]
    'deny_panic: {
        MethodTest::trait_1();
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

#[allow(dead_code)]
#[allow(unconditional_recursion)]
fn do_not_stuck_on_loops() {
    #[allow(unused_labels)]
    'deny_panic: {
        do_not_stuck_on_loops()
    }
}

#[allow(dead_code)]
fn check_closures() {
    #[allow(unused_labels)]
    'deny_panic: {
        let x = || panic!();
        x();
    }
}

//fn test_closure_2_<F, T, Ret>(thunk: F, x: &mut T) -> Ret
//where
//    F: FnOnce(&mut T) -> Ret,
//{
//    #[allow(unused_labels)]
//    'deny_panic: {
//        thunk(x)
//    }
//}
//
//#[allow(dead_code)]
//fn test_closure_2() {
//    test_closure_2_(|x| panic!(), &mut vec![2]);
//}

//#[macro_export]
//macro_rules! deny_panic_closure {
//    ($closure:expr) => {'deny_panic: {|x| $closure(x)}}
//}

fn test<N: Fn(), X: FnMut()>(f: N, mut x: X) {
    #[allow(unused_labels)]
    'deny_panic: {
        f();
    }
    x();
}

fn il_cane() {
    test(|| panic!(), || {});
}
// TSTARE CHE LE CLOSURE CHE SONO NON INDENY POSSANO PANICARE

//fn giig() {}
//fn test_cani_2<F: Clone>(f: F) {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        f.clone();
//    }
//}


// TODO
//struct<C: Clone> Abc {
//    c: C,
//}
//
//impl<C: Clone> Abc {
//    pub fn gigi() {
//        'deny_panic: {
//            self.c.clone()
//        }
//    }
//}
//
//struct TestStruct {}
//impl Clone for TestStruct {
//    fn clone(&self) -> Self {
//        panic!()
//    }
//}
//
//fn test1() {
//    let c = Abc { c: TestStruct {} };
//    c.gigi();
//}
