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
extern crate xi_rope;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use xi_rope::rope::Rope;

fn build_triangle(n: usize) -> String {
    let mut s = String::new();
    let mut line = String::new();
    for _ in 0..n {
        s += &line;
        s += "\n";
        line += "a";
    }
    s
}

fn build_short_lines(n: usize) -> String {
    let line = "match s.as_bytes()[minsplit - 1..splitpoint].iter().rposition(|&c| c == b'\n') {";
    let mut s = String::new();
    for _ in 0..n {
        s += line;
    }
    s
}

fn build_few_big_lines(size: usize) -> String {
    let mut s = String::with_capacity(size * 10 + 20);
    for _ in 0..10 {
        for _ in 0..size {
            s += "a";
        }
        s += "\n";
    }
    s
}

fn benchmark_file_load_short_lines(c: &mut Criterion) {
    let text = build_short_lines(50_000);
    c.bench_function("benchmark_file_load_short_lines", |b| b.iter(|| {
        Rope::from(black_box(&text));
    }));
}

fn benchmark_file_load_few_big_lines(c: &mut Criterion) {
    let text = build_few_big_lines(1_000_000);
    c.bench_function("benchmark_file_load_few_big_lines", |b| b.iter(|| {
        Rope::from(black_box(&text));
    }));
}

fn benchmark_char_insertion_one_line_edit(c: &mut Criterion) {
    let mut text = Rope::from("b".repeat(100));
    let mut offset = 100;
    c.bench_function("benchmark_char_insertion_one_line_edit", |b| b.iter(|| {
        text.edit(offset..=offset, black_box("a"));
        offset += 1;
    }));
}

fn benchmark_paste_into_line(c: &mut Criterion) {
    fn main() {
        eprintln!("Benchmarks removed in xi-editor-ph7; use the upstream xi-editor project if you need perf data.");
    }
    c.bench_function("benchmark_paste_into_line", |b| b.iter(|| {
