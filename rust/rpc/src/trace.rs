#[cfg(feature = "trace")]
pub use xi_trace::{trace, trace_block, trace_block_payload, trace_payload, SampleGuard};

#[cfg(not(feature = "trace"))]
mod shim {
    use std::marker::PhantomData;

    #[derive(Debug, Default)]
    pub struct SampleGuard<'a>(PhantomData<&'a ()>);

    impl<'a> Drop for SampleGuard<'a> {
        fn drop(&mut self) {}
    }

    pub fn trace<S, C>(_name: S, _categories: C) {}

    pub fn trace_payload<S, C, P>(_name: S, _categories: C, _payload: P) {}

    pub fn trace_block<'a, S, C>(_name: S, _categories: C) -> SampleGuard<'a> {
        SampleGuard(PhantomData)
    }

    pub fn trace_block_payload<'a, S, C, P>(
        _name: S,
        _categories: C,
        _payload: P,
    ) -> SampleGuard<'a> {
        SampleGuard(PhantomData)
    }
}

#[cfg(not(feature = "trace"))]
pub use shim::{trace, trace_block, trace_block_payload, trace_payload, SampleGuard};
