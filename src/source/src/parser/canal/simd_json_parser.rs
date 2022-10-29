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

use risingwave_common::error::ErrorCode::ProtocolError;
use risingwave_common::error::RwError;
use simd_json::{BorrowedValue, ValueAccess};

use crate::parser::canal::operators::*;
use crate::parser::common::simd_json_parse_value;
use crate::{SourceParser, SourceStreamChunkRowWriter, WriteGuard};

#[derive(Debug)]
pub struct CanalJsonParser;

const AFTER: &str = "data";
const BEFORE: &str = "old";
const OP: &str = "type";

impl SourceParser for CanalJsonParser {
    fn parse(
        &self,
        payload: &[u8],
        writer: SourceStreamChunkRowWriter<'_>,
    ) -> risingwave_common::error::Result<WriteGuard> {
        let mut payload_mut = payload.to_vec();
        let event: BorrowedValue<'_> = simd_json::to_borrowed_value(&mut payload_mut)
            .map_err(|e| RwError::from(ProtocolError(e.to_string())))?;

        let op = event.get(OP).and_then(|v| v.as_str()).ok_or_else(|| {
            RwError::from(ProtocolError(
                "op field not found in maxwell json".to_owned(),
            ))
        })?;

        match op {
            TICDC_INSERT_EVENT => {
                let after = event.get(AFTER).ok_or_else(|| {
                    RwError::from(ProtocolError(
                        "data is missing for creating event".to_string(),
                    ))
                })?;
                writer.insert(|column| {
                    simd_json_parse_value(&column.data_type, after.get(column.name.as_str()))
                        .map_err(Into::into)
                })
            }
            TICDC_UPDATE_EVENT => {
                let after = event.get(AFTER).ok_or_else(|| {
                    RwError::from(ProtocolError(
                        "data is missing for updating event".to_string(),
                    ))
                })?;
                let before = event.get(BEFORE).ok_or_else(|| {
                    RwError::from(ProtocolError(
                        "old is missing for updating event".to_string(),
                    ))
                })?;

                writer.update(|column| {
                    // old only contains the changed columns but data contains all columns.
                    let before =
                        simd_json_parse_value(&column.data_type, before.get(column.name.as_str()))?;
                    let after =
                        simd_json_parse_value(&column.data_type, after.get(column.name.as_str()))?;
                    Ok((before, after))
                })
            }
            TICDC_DELETE_EVENT => {
                let before = event.get(AFTER).ok_or_else(|| {
                    RwError::from(ProtocolError("old is missing for delete event".to_string()))
                })?;
                writer.delete(|column| {
                    simd_json_parse_value(&column.data_type, before.get(column.name.as_str()))
                        .map_err(Into::into)
                })
            }
            TICDC_QUERY_EVENT => Err(RwError::from(ProtocolError(
                "received a query message, please set `canal.instance.filter.query.dml` to true."
                    .to_string(),
            ))),
            other => Err(RwError::from(ProtocolError(format!(
                "unknown canal json op: {}",
                other
            )))),
        }
    }
}
