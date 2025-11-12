// Copyright 2017 The xi-editor Authors.
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

//! Benchmarks of PEG parsing libraries

use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Run as:
/// ```
/// run nightly cargo bench --features "nom regex pom"
/// ```
use std::env;

extern crate xi_lang;

// We no longer use the nightly `test` crate; use `criterion` for stable benches.

#[cfg(feature = "pom")]
extern crate pom;

#[cfg(feature = "regex")]
fn main() {
    eprintln!("Benchmarks removed in xi-editor-ph7; use the upstream xi-editor project if you need perf data.");
}
#[macro_use]
