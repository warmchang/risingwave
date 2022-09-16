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

use std::collections::BTreeMap;

use risingwave_common::array::{Op, Row};
use risingwave_common::util::ordered::OrderedRow;
use risingwave_storage::StateStore;

use crate::executor::error::StreamExecutorResult;
use crate::executor::managed_state::top_n::ManagedTopNState;

const TOPN_CACHE_HIGH_CAPACITY_FACTOR: usize = 2;

pub struct TopNCache {
    /// Rows in the range `[0, offset)`
    pub low: BTreeMap<OrderedRow, Row>,
    /// Rows in the range `[offset, offset+limit)`
    pub middle: BTreeMap<OrderedRow, Row>,
    /// Rows in the range `[offset+limit, offset+limit+high_capacity)`
    pub high: BTreeMap<OrderedRow, Row>,
    pub high_capacity: usize,
    pub offset: usize,
    /// Assumption: `limit != 0`
    pub limit: usize,
}

impl TopNCache {
    pub fn new(offset: usize, limit: usize) -> Self {
        assert!(limit != 0);
        Self {
            low: BTreeMap::new(),
            middle: BTreeMap::new(),
            high: BTreeMap::new(),
            high_capacity: (offset + limit) * TOPN_CACHE_HIGH_CAPACITY_FACTOR,
            offset,
            limit,
        }
    }

    pub fn is_low_cache_full(&self) -> bool {
        debug_assert!(self.low.len() <= self.offset);
        let full = self.low.len() == self.offset;
        if !full {
            debug_assert!(self.middle.is_empty());
            debug_assert!(self.high.is_empty());
        }
        full
    }

    pub fn is_middle_cache_full(&self) -> bool {
        debug_assert!(self.middle.len() <= self.limit);
        let full = self.middle.len() == self.limit;
        if full {
            debug_assert!(self.is_low_cache_full());
        } else {
            debug_assert!(self.high.is_empty());
        }
        full
    }

    pub fn is_high_cache_full(&self) -> bool {
        debug_assert!(self.high.len() <= self.high_capacity);
        let full = self.high.len() == self.high_capacity;
        if full {
            debug_assert!(self.is_middle_cache_full());
        }
        full
    }

    /// Insert input row to corresponding cache range according to its order key.
    ///
    /// Changes in `self.middle` is recorded to `res_ops` and `res_rows`, which will be
    /// used to generate messages to be sent to downstream operators.
    pub fn insert(
        &mut self,
        ordered_pk_row: OrderedRow,
        row: Row,
        res_ops: &mut Vec<Op>,
        res_rows: &mut Vec<Row>,
    ) {
        if !self.is_low_cache_full() {
            self.low.insert(ordered_pk_row, row);
            return;
        }

        let elem_to_compare_with_middle =
            if let Some(low_last) = self.low.last_entry()
                && ordered_pk_row <= *low_last.key() {
                // Take the last element of `cache.low` and insert input row to it.
                let low_last = low_last.remove_entry();
                self.low.insert(ordered_pk_row, row);
                low_last
            } else {
                (ordered_pk_row, row)
            };

        if !self.is_middle_cache_full() {
            self.middle.insert(
                elem_to_compare_with_middle.0,
                elem_to_compare_with_middle.1.clone(),
            );
            res_ops.push(Op::Insert);
            res_rows.push(elem_to_compare_with_middle.1);
            return;
        }

        let elem_to_compare_with_high = {
            let middle_last = self.middle.last_entry().unwrap();
            if elem_to_compare_with_middle.0 <= *middle_last.key() {
                // If the row in the range of [offset, offset+limit), the largest row in
                // `cache.middle` needs to be moved to `cache.high`
                let res = middle_last.remove_entry();
                res_ops.push(Op::Delete);
                res_rows.push(res.1.clone());
                res_ops.push(Op::Insert);
                res_rows.push(elem_to_compare_with_middle.1.clone());
                self.middle
                    .insert(elem_to_compare_with_middle.0, elem_to_compare_with_middle.1);
                res
            } else {
                elem_to_compare_with_middle
            }
        };

        if !self.is_high_cache_full() {
            self.high
                .insert(elem_to_compare_with_high.0, elem_to_compare_with_high.1);
        } else {
            let high_last = self.high.last_entry().unwrap();
            if elem_to_compare_with_high.0 <= *high_last.key() {
                high_last.remove_entry();
                self.high
                    .insert(elem_to_compare_with_high.0, elem_to_compare_with_high.1);
            }
        }
    }

    /// Delete input row from the cache.
    ///
    /// Changes in `self.middle` is recorded to `res_ops` and `res_rows`, which will be
    /// used to generate messages to be sent to downstream operators.
    ///
    /// Because we may need to add data from the state table to `self.high` during the delete
    /// operation, we need to pass in `pk_prefix`, `epoch` and `managed_state` to do a prefix
    /// scan of the state table.
    #[allow(clippy::too_many_arguments)]
    pub async fn delete<S: StateStore>(
        &mut self,
        pk_prefix: Option<&Row>,
        managed_state: &mut ManagedTopNState<S>,
        ordered_pk_row: OrderedRow,
        row: Row,
        epoch: u64,
        res_ops: &mut Vec<Op>,
        res_rows: &mut Vec<Row>,
    ) -> StreamExecutorResult<()> {
        if self.is_middle_cache_full() && ordered_pk_row > *self.middle.last_key_value().unwrap().0
        {
            // The row is in high
            self.high.remove(&ordered_pk_row);
        } else if self.is_low_cache_full()
            && (self.offset == 0 || ordered_pk_row > *self.low.last_key_value().unwrap().0)
        {
            // The row is in mid
            // Try to fill the high cache if it is empty
            if self.high.is_empty() {
                managed_state
                    .fill_cache(
                        pk_prefix,
                        &mut self.high,
                        self.middle.last_key_value().unwrap().0,
                        self.high_capacity,
                        epoch,
                    )
                    .await?;
            }

            self.middle.remove(&ordered_pk_row);
            res_ops.push(Op::Delete);
            res_rows.push(row.clone());

            // Bring one element, if any, from high cache to middle cache
            if !self.high.is_empty() {
                let high_first = self.high.pop_first().unwrap();
                res_ops.push(Op::Insert);
                res_rows.push(high_first.1.clone());
                self.middle.insert(high_first.0, high_first.1);
            }
        } else {
            // The row is in low
            self.low.remove(&ordered_pk_row);

            // Bring one element, if any, from middle cache to low cache
            if !self.middle.is_empty() {
                let middle_first = self.middle.pop_first().unwrap();
                res_ops.push(Op::Delete);
                res_rows.push(middle_first.1.clone());
                self.low.insert(middle_first.0, middle_first.1);

                // Try to fill the high cache if it is empty
                if self.high.is_empty() {
                    managed_state
                        .fill_cache(
                            pk_prefix,
                            &mut self.high,
                            self.middle.last_key_value().unwrap().0,
                            self.high_capacity,
                            epoch,
                        )
                        .await?;
                }

                // Bring one element, if any, from high cache to middle cache
                if !self.high.is_empty() {
                    let high_first = self.high.pop_first().unwrap();
                    res_ops.push(Op::Insert);
                    res_rows.push(high_first.1.clone());
                    self.middle.insert(high_first.0, high_first.1);
                }
            }
        }

        Ok(())
    }
}
