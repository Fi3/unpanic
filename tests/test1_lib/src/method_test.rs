pub struct MethodTest;

impl MethodTest {
    pub fn method_test(&self) {
        crate::it_panic()
    }
    pub fn assoc_fn() {
        crate::it_panic()
    }
}
