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

use std::fmt::Debug;

use risingwave_common::error::ErrorCode::ProtocolError;
use risingwave_common::error::{Result, RwError};
use simd_json::{BorrowedValue, StaticNode, ValueAccess};

use super::operators::*;
use crate::parser::common::simd_json_parse_value;
use crate::{SourceParser, SourceStreamChunkRowWriter, WriteGuard};

const BEFORE: &str = "before";
const AFTER: &str = "after";
const OP: &str = "op";

#[inline]
fn ensure_not_null<'a, 'b: 'a>(value: &'a BorrowedValue<'b>) -> Option<&'a BorrowedValue<'b>> {
    if let BorrowedValue::Static(StaticNode::Null) = value {
        None
    } else {
        Some(value)
    }
}

#[derive(Debug)]
pub struct DebeziumJsonParser {
    including_schema: bool,
}

impl DebeziumJsonParser {
    pub fn new(including_schema: bool) -> Self {
        Self { including_schema }
    }
}

impl SourceParser for DebeziumJsonParser {
    fn parse(&self, payload: &[u8], writer: SourceStreamChunkRowWriter<'_>) -> Result<WriteGuard> {
        let mut payload_mut = payload.to_vec();
        let message: BorrowedValue<'_> = simd_json::to_borrowed_value(&mut payload_mut)
            .map_err(|e| RwError::from(ProtocolError(e.to_string())))?;

        // if the debezium json message includes schema,
        // we need the payload field of the message,
        // else the message it self
        let payload = if self.including_schema {
            message
                .get("payload")
                .and_then(ensure_not_null)
                .ok_or_else(|| {
                    RwError::from(ProtocolError("no payload in debezium event".to_owned()))
                })?
        } else {
            &message
        };

        let op = payload.get(OP).and_then(|v| v.as_str()).ok_or_else(|| {
            RwError::from(ProtocolError(
                "op field not found in maxwell json".to_owned(),
            ))
        })?;

        match op {
            DEBEZIUM_UPDATE_OP => {
                let before = payload.get(BEFORE).and_then(ensure_not_null).ok_or_else(|| {
                    RwError::from(ProtocolError(
                        "before is missing for updating event. If you are using postgres, you may want to try ALTER TABLE $TABLE_NAME REPLICA IDENTITY FULL;".to_string(),
                    ))
                })?;

                let after = payload
                    .get(AFTER)
                    .and_then(ensure_not_null)
                    .ok_or_else(|| {
                        RwError::from(ProtocolError(
                            "after is missing for updating event".to_string(),
                        ))
                    })?;

                writer.update(|column| {
                    let before =
                        simd_json_parse_value(&column.data_type, before.get(column.name.as_str()))?;
                    let after =
                        simd_json_parse_value(&column.data_type, after.get(column.name.as_str()))?;

                    Ok((before, after))
                })
            }
            DEBEZIUM_CREATE_OP | DEBEZIUM_READ_OP => {
                let after = payload
                    .get(AFTER)
                    .and_then(ensure_not_null)
                    .ok_or_else(|| {
                        RwError::from(ProtocolError(
                            "after is missing for creating event".to_string(),
                        ))
                    })?;

                writer.insert(|column| {
                    simd_json_parse_value(&column.data_type, after.get(column.name.as_str()))
                        .map_err(Into::into)
                })
            }
            DEBEZIUM_DELETE_OP => {
                let before = payload
                    .get(BEFORE)
                    .and_then(ensure_not_null)
                    .ok_or_else(|| {
                        RwError::from(ProtocolError(
                            "before is missing for delete event".to_string(),
                        ))
                    })?;

                writer.delete(|column| {
                    simd_json_parse_value(&column.data_type, before.get(column.name.as_str()))
                        .map_err(Into::into)
                })
            }
            _ => Err(RwError::from(ProtocolError(format!(
                "unknown debezium op: {}",
                op
            )))),
        }
    }
}
