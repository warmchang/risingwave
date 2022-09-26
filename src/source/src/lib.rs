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

#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(rustdoc::private_intra_doc_links)]
#![feature(trait_alias)]
#![feature(generic_associated_types)]
#![feature(binary_heap_drain_sorted)]
#![feature(lint_reasons)]
#![feature(result_option_inspect)]
#![feature(once_cell)]

use std::collections::HashMap;
use std::fmt::Debug;

use enum_as_inner::EnumAsInner;
pub use manager::*;
pub use parser::*;
use risingwave_common::array::StreamChunk;
use risingwave_common::error::Result;
use risingwave_common::types::DataType;
use risingwave_connector::source::SplitId;
pub use table::*;

use crate::connector_source::{ConnectorSource, ConnectorSourceReader};

pub mod parser;

mod manager;

mod common;
pub mod connector_source;
pub mod monitor;
pub mod row_id;
mod table;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    Invalid,
    Json,
    Protobuf,
    DebeziumJson,
    Avro,
}

impl SourceFormat {
    pub fn supported_type(&self, dt: &DataType) -> bool {
        match self {
            Self::Json | Self::Protobuf | Self::Avro => {
                use DataType as T;
                match dt {
                    T::Boolean
                    | T::Int16
                    | T::Int32
                    | T::Int64
                    | T::Float32
                    | T::Float64
                    | T::Decimal
                    | T::Date
                    | T::Varchar
                    | T::Time
                    | T::Timestamp => true,
                    T::Struct(st) => st.fields.iter().all(|dt| self.supported_type(dt)),
                    T::Timestampz | T::Interval | T::List { .. } => false,
                }
            }
            Self::DebeziumJson => true,
            _ => false,
        }
    }
}

#[derive(Debug, EnumAsInner)]
pub enum SourceImpl {
    Table(TableSource),
    Connector(ConnectorSource),
}

#[expect(clippy::large_enum_variant)]
pub enum SourceStreamReaderImpl {
    Table(TableStreamReader),
    Connector(ConnectorSourceReader),
}

impl SourceStreamReaderImpl {
    pub async fn next(&mut self) -> Result<StreamChunkWithState> {
        match self {
            SourceStreamReaderImpl::Table(t) => t.next().await.map(Into::into),
            SourceStreamReaderImpl::Connector(c) => c.next().await,
        }
    }
}

/// [`StreamChunkWithState`] returns stream chunk together with offset for each split. In the
/// current design, one connector source can have multiple split reader. The keys are unique
/// `split_id` and values are the latest offset for each split.
#[derive(Clone, Debug)]
pub struct StreamChunkWithState {
    pub chunk: StreamChunk,
    pub split_offset_mapping: Option<HashMap<SplitId, String>>,
}

/// The `split_offset_mapping` field is unused for the table source, so we implement `From` for it.
impl From<StreamChunk> for StreamChunkWithState {
    fn from(chunk: StreamChunk) -> Self {
        Self {
            chunk,
            split_offset_mapping: None,
        }
    }
}
