pub struct MethodTest;

pub trait Trait {
    fn trait_0(&self);
    fn trait_1();
}

impl MethodTest {
    pub fn method_test(&self) {
        crate::it_panic()
    }
    pub fn assoc_fn() {
        crate::it_panic()
    }
}

impl Trait for MethodTest {
    fn trait_0(&self) {
        crate::it_panic()
    }
    fn trait_1() {
        crate::it_panic()
    }
}
