/// Call a function that can panic imported from test1_lib
/// When checked by unpanic it should report an error
use test1_lib;
// use unicode_ident::*;
// use proc_macro2::*;
// use quote::*;
// use syn::*;
// use serde::*;

use test1_lib::allow_panic_test::allow_panic;
use test1_lib::function_test::function_test;
use test1_lib::method_test::MethodTest;

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

trait Cane {
    fn bau(self);
}

impl Cane for u8 {
    fn bau(self) {}
}

trait ItPanic {
    fn panic_now() -> u32;
    //fn panic_now_self(self);
    //fn panic_now_implemented() {
    //    panic!()
    //}
}

struct PanicStruct {}
struct Gatto {}

impl ItPanic for PanicStruct {
    fn panic_now() -> u32 {
        panic!();
        0
    }

    //fn panic_now_self(self) {
    //    panic!()
    //}
}
impl ItPanic for Gatto {
    fn panic_now() -> u32 {
        panic!();
        0
    }
}

//#[allow(dead_code)]
//fn panic_() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        PanicStruct::panic_now()
//    }
//}

impl PanicStruct {
    // TODO 'deny_panic in methods are not supported
    #[allow(dead_code)]
    fn panic_<C: Clone>(c: C) {
        #[allow(unused_labels)]
        'deny_panic: {
            let _x = Self::panic_now() + PanicStruct::panic_now() + Gatto::panic_now();
            //Gatto::panic_now();
            //let _g = c.clone();
            // TODO vec![..] fails add a test for it!!
            //let x = vec![1,2,3];
            //function_test();
            //function_test();
            //x.iter().map(|x| Gatto::panic_now).collect::<Vec<_>>();
        }
    }
}
