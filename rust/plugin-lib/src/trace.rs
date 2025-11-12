#[cfg(feature = "trace")]
pub use xi_trace::{
    chrome_trace_dump,
    disable_tracing,
    enable_tracing,
    is_enabled,
    samples_cloned_unsorted,
    trace,
    trace_block,
    trace_block_payload,
    trace_payload,
    Sample,
    SampleGuard,
};

#[cfg(not(feature = "trace"))]
mod shim {
    use serde_json::Value;
    use std::io::Write;
    use std::marker::PhantomData;

    #[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Sample;

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

    pub fn trace_block_payload<'a, S, C, P>(_name: S, _categories: C, _payload: P) -> SampleGuard<'a> {
        SampleGuard(PhantomData)
    }

    pub fn enable_tracing() {}

    pub fn disable_tracing() {}

    pub fn is_enabled() -> bool {
        false
    }

    pub fn samples_cloned_unsorted() -> Vec<Sample> {
        Vec::new()
    }

    pub mod chrome_trace_dump {
        use super::Sample;
        use serde_json::Value;
        use std::io::Write;

        #[derive(Clone, Debug)]
        pub struct Error;

        pub fn decode(_samples: Value) -> Result<Vec<Sample>, Error> {
            Ok(Vec::new())
        }

        pub fn serialize<W>(_samples: &Vec<Sample>, _output: W) -> Result<(), Error>
        where
            W: Write,
        {
            Ok(())
        }

        pub fn to_value(_samples: &Vec<Sample>) -> Result<Value, Error> {
            Ok(Value::Null)
        }
    }

    pub use chrome_trace_dump;
    pub use SampleGuard;
}

#[cfg(not(feature = "trace"))]
pub use shim::{
    chrome_trace_dump,
    disable_tracing,
    enable_tracing,
    is_enabled,
    samples_cloned_unsorted,
    trace,
    trace_block,
    trace_block_payload,
    trace_payload,
    Sample,
    SampleGuard,
};
