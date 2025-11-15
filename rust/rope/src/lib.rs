// Copyright 2016 The xi-editor Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Trees for text.

#![allow(
    clippy::collapsible_if,
    clippy::len_without_is_empty,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::new_without_default,
    clippy::should_implement_trait,
    clippy::wrong_self_convention
)]

extern crate bytecount;
extern crate memchr;
extern crate regex;
extern crate unicode_segmentation;

#[cfg(feature = "serde")]
extern crate serde;

#[cfg(test)]
extern crate serde_json;
#[cfg(test)]
extern crate serde_test;

pub mod breaks;
pub mod compare;
pub mod delta;
pub mod diff;
pub mod engine;
pub mod find;
pub(crate) mod helpers;
pub mod interval;
pub(crate) mod metrics;
pub mod multiset;
pub mod rope;
#[cfg(feature = "serde")]
pub mod serde_fixtures;
#[cfg(feature = "serde")]
mod serde_impls;
pub mod spans;
#[cfg(test)]
mod test_helpers;
pub mod tree;

pub use crate::delta::{Builder as DeltaBuilder, Delta, DeltaElement, Transformer};
pub use crate::interval::Interval;
pub use crate::rope::{LinesMetric, Rope, RopeDelta, RopeInfo};
#[cfg(feature = "cursor_state")]
pub use crate::tree::CursorState;
pub use crate::tree::{Cursor, CursorDescriptor, Metric};
#[cfg(feature = "tree_builder_slice_trace")]
pub use crate::tree::{
    NullTreeBuilderTracer, TreeBuilderEvent, TreeBuilderEventKind, TreeBuilderTracer,
};
