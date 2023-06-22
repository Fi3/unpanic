/// Call a function that can panic imported from test1_lib
/// When checked by unpanic it should report an error
extern crate test1_lib;
use test1_lib::add;
fn main() {
    should_panic();
}

fn should_panic() {
    #[allow(unused_labels)]
    'deny_panic: {
        add();
    }
}
