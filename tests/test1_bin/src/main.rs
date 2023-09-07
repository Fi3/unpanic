/// Call a function that can panic imported from test1_lib
/// When checked by unpanic it should report an error
//use test1_lib::allow_panic_test::allow_panic;
//use test1_lib::function_test::function_test;
//use test1_lib::method_test::{MethodTest, Trait};

fn main() {}
//
//#[allow(dead_code)]
//fn test_if_see_panics_in_imported_functions() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        function_test();
//    }
//}
//
//#[allow(dead_code)]
//fn test_if_see_panics_in_local_functions() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        same_crate();
//    }
//}
//
//#[allow(dead_code)]
//fn test_if_see_panics_in_methods() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        let a = MethodTest;
//        a.method_test();
//    }
//}
//
//#[allow(dead_code)]
//fn test_if_see_panics_in_assoc_fn() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        MethodTest::assoc_fn();
//    }
//}
//
//#[allow(dead_code)]
//fn test_if_see_panics_in_trait_0() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        let a = MethodTest;
//        a.trait_0();
//    }
//}
//
//#[allow(dead_code)]
//fn test_if_see_panics_in_trait_1() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        MethodTest::trait_1();
//    }
//}
//
//#[allow(dead_code)]
//fn test_if_ingnore_panic_in_allow_block() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        allow_panic()
//    }
//}
//
//#[allow(dead_code)]
//fn same_crate() {
//    panic!()
//}
//
//#[allow(dead_code)]
//#[allow(unconditional_recursion)]
//fn do_not_stuck_on_loops() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        do_not_stuck_on_loops()
//    }
//}
//
//#[allow(dead_code)]
//fn check_closures() {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        let x = || panic!();
//        x();
//    }
//}
//
//fn test_higher_order_fn_<N: Fn(), X: FnMut()>(f: N, mut x: X) {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        f();
//    }
//    x();
//}
//
//#[allow(dead_code)]
//fn test_higher_order_fn_1() {
//    test_higher_order_fn_(|| panic!(), || {});
//}

// TODO below test do not works
//#[allow(dead_code)]
//fn test_higher_order_fn_2_<N: Fn()>(n: N) {
//    test_higher_order_fn_(n, || {});
//}
//
//#[allow(dead_code)]
//fn test_higher_order_fn_2<N: Fn()>(n: N) {
//    test_higher_order_fn_2_(|| panic!());
//}

//#[allow(dead_code)]
//fn test_higher_order_fn_different_crate_() {
//    test1_lib::test_higher_order_fn_different_crate(|| panic!(), || {});
//}
//
//#[allow(dead_code)]
//fn test_higher_order_fn_different_crate_2() {
//    test1_lib::test_higher_order_fn_different_crate(|| test1_lib::it_panic(), || {});
//}
//
//struct TestStruct {}
//
//impl Clone for TestStruct {
//    fn clone(&self) -> Self {
//        panic!()
//    }
//}
//
//#[allow(dead_code)]
//fn test_higher_order_with_trait_<C: Clone>(c: C) {
//    #[allow(unused_labels)]
//    'deny_panic: {
//        let _ = c.clone();
//    }
//}
//
//#[allow(dead_code)]
//fn test_higher_order_with_trait_1() {
//    let test_struct = TestStruct {};
//    test_higher_order_with_trait_(test_struct);
//}

// TODO the below tests do not works
//
//fn test_higher_order_with_trait_2_<C: Clone>(c: C) {
//    test_higher_order_with_trait_(c);
//}
//
//fn test_higher_order_with_trait_2<C: Clone>(c: C) {
//    let test_struct = TestStruct {};
//    test_higher_order_with_trait_2(c);
//}


struct TestStruct2 {}
impl TestStruct2 {
    fn deny_p<T: Fn()>(&self, th: T) {
        'deny_panic: {
            th();
        }
    }
}

fn test() {
    let stru = TestStruct2 {};
    stru.deny_p(|| panic!());
}


//use std::sync::{Mutex as Mutex_, MutexGuard, PoisonError};
//pub struct Mutex<T: ?Sized>(Mutex_<T>);
//
//impl<T> Mutex<T> {
//    pub fn safe_lock<F, Ret>(&self, thunk: F) -> Result<Ret, PoisonError<MutexGuard<'_, T>>>
//    where
//        F: FnOnce(&mut T) -> Ret,
//    {
//        let mut lock = self.0.lock()?;
//        let return_value: Ret;
//        'deny_panic: {
//             return_value = thunk(&mut *lock);
//        }
//        drop(lock);
//        Ok(return_value)
//    }
//    pub fn new(v: T) -> Self {
//        Mutex(Mutex_::new(v))
//    }
//
//}
//
//
//fn test_() {
//    let x = Mutex::new(90);
//    x.safe_lock(|x| {
//        let y = *x + 10;
//        panic!();  
//        y
//    });
//}
