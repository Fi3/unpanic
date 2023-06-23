pub fn allow_panic() {
    #[allow(unused_labels)]
    'allow_panic: {
        crate::it_panic();
    }
}
