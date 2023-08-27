use interface_lib::Panicable;
use interface_nested_lib::call_it_panic_nested;
use interface_nested_lib_feature::call_it_panic_nested_feature;

use test1_lib::*;

#[derive(Panicable)]
struct PanicStruct {}

pub fn it_panic_() {
    panic!()
}

pub fn it_panic_2() {
    #[allow(unused_labels)]
    'deny_panic: {
        it_panic()
    }
}

pub fn it_panic_nested() {
    #[allow(unused_labels)]
    'deny_panic: {
        call_it_panic_nested()
    }
}
pub fn it_panic_nested_feature() {
    #[allow(unused_labels)]
    'deny_panic: {
        call_it_panic_nested_feature()
    }
}

pub fn it_panic_nested_macro() {
    #[allow(unused_labels)]
    'deny_panic: {
        PanicStruct::panic_now()
    }
}
