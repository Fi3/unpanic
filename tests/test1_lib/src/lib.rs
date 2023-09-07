#![no_std]

pub mod allow_panic_test;
/// Library used by test1_bin
pub mod function_test;
pub mod method_test;


pub fn it_panic() {
    panic!()
}

pub fn deny_panic_in_dependency() {
    #[allow(unused_labels)]
    'deny_panic: {
        it_panic()
    }
}

pub fn test_higher_order_fn_different_crate<N: Fn(), X: FnMut()>(f: N, mut x: X) {
    #[allow(unused_labels)]
    'deny_panic: {
        f();
    }
    x();
}
