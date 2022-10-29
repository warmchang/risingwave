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

/// NOTICE: This file describes not the official Canal format, but the `TiCDC` implementation, as described in [the website](https://docs.pingcap.com/tidb/v6.0/ticdc-canal-json).
use std::collections::BTreeMap;

use risingwave_common::error::ErrorCode::ProtocolError;
use risingwave_common::error::{Result, RwError};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

use crate::parser::canal::operators::*;
use crate::parser::common::json_parse_value;
use crate::SourceParser;

#[derive(Debug)]
pub struct CanalJsonParser;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanalJsonEvent {
    #[serde(rename = "old")]
    pub before: Option<Vec<BTreeMap<String, Value>>>,
    #[serde(rename = "data")]
    pub after: Option<Vec<BTreeMap<String, Value>>>,
    #[serde(rename = "type")]
    pub op: String,
    pub ts: u64,
}

impl SourceParser for CanalJsonParser {
    fn parse(
        &self,
        payload: &[u8],
        writer: crate::SourceStreamChunkRowWriter<'_>,
    ) -> Result<crate::WriteGuard> {
        let event: CanalJsonEvent = serde_json::from_slice(payload)
            .map_err(|e| RwError::from(ProtocolError(e.to_string())))?;
        match event.op.as_str() {
            TICDC_DELETE_EVENT => {
                let before = event.before.as_ref().ok_or_else(|| {
                    RwError::from(ProtocolError("old is missing for delete event".to_string()))
                })?;
                if before.is_empty() {
                    Err(RwError::from(ProtocolError(
                        "old field is empty".to_string(),
                    )))
                } else {
                    writer.delete(|columns| {
                        json_parse_value(&columns.data_type, before[0].get(&columns.name))
                            .map_err(Into::into)
                    })
                }
            }
            TICDC_INSERT_EVENT => {
                let after = event.after.as_ref().ok_or_else(|| {
                    RwError::from(ProtocolError(
                        "data is missing for delete event".to_string(),
                    ))
                })?;
                if after.is_empty() {
                    Err(RwError::from(ProtocolError(
                        "data field is empty".to_string(),
                    )))
                } else {
                    writer.insert(|columns| {
                        json_parse_value(&columns.data_type, after[0].get(&columns.name))
                            .map_err(Into::into)
                    })
                }
            }
            TICDC_UPDATE_EVENT => {
                let before = event.before.as_ref().ok_or_else(|| {
                    RwError::from(ProtocolError("old is missing for delete event".to_string()))
                })?;
                if before.is_empty() {
                    return Err(RwError::from(ProtocolError(
                        "old field is empty".to_string(),
                    )));
                }

                let after = event.after.as_ref().ok_or_else(|| {
                    RwError::from(ProtocolError(
                        "data is missing for delete event".to_string(),
                    ))
                })?;
                if after.is_empty() {
                    return Err(RwError::from(ProtocolError(
                        "data field is empty".to_string(),
                    )));
                }

                writer.update(|column| {
                    let old = json_parse_value(&column.data_type, before[0].get(&column.name))?;
                    let new = json_parse_value(&column.data_type, after[0].get(&column.name))?;

                    Ok((old, new))
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

mod test {
    use super::*;

    fn get_update_payload() -> &'static str {
        r#"
{
    "id": 0,
    "database": "test",
    "table": "tp_int",
    "pkNames": [
        "id"
    ],
    "isDdl": false,
    "type": "INSERT",
    "es": 1639633141221,
    "ts": 1639633142960,
    "sql": "",
    "sqlType": {
        "c_bigint": -5,
        "c_int": 4,
        "c_mediumint": 4,
        "c_smallint": 5,
        "c_tinyint": -6,
        "id": 4
    },
    "mysqlType": {
        "c_bigint": "bigint",
        "c_int": "int",
        "c_mediumint": "mediumint",
        "c_smallint": "smallint",
        "c_tinyint": "tinyint",
        "id": "int"
    },
    "data": [
        {
            "c_bigint": "9223372036854775807",
            "c_int": "2147483647",
            "c_mediumint": "8388607",
            "c_smallint": "32767",
            "c_tinyint": "127",
            "id": "2"
        }
    ],
    "old": null,
    "_tidb": {
        "commitTs": 163963314122145239
    }
}
    "#
    }
    #[test]
    fn test_parse_event() {
        let event: CanalJsonEvent =
            serde_json::from_slice(get_update_payload().as_bytes()).unwrap();
        println!("{:?}", event);
    }
}
