// Copyright 2022 Singularity Data
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(not(any(
target_feature = "sse4.2",
target_feature = "avx2",
target_feature = "neon",
target_feature = "simd128"
)))]
mod json_parser;

#[cfg(any(
target_feature = "sse4.2",
target_feature = "avx2",
target_feature = "neon",
target_feature = "simd128"
))]
mod simd_json_parser;

mod operators;

#[cfg(not(any(
target_feature = "sse4.2",
target_feature = "avx2",
target_feature = "neon",
target_feature = "simd128"
)))]
pub use json_parser::*;

#[cfg(any(
target_feature = "sse4.2",
target_feature = "avx2",
target_feature = "neon",
target_feature = "simd128"
))]
pub use simd_json_parser::*;
