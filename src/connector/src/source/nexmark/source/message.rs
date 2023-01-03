// Copyright 2023 Singularity Data
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

use bytes::Bytes;
use nexmark::event::Event;

use crate::source::nexmark::source::combined_event::CombinedEvent;
use crate::source::{SourceMessage, SplitId};

#[derive(Clone, Debug)]
pub struct NexmarkMessage {
    pub split_id: SplitId,
    pub sequence_number: String,
    pub payload: Bytes,
}

impl From<NexmarkMessage> for SourceMessage {
    fn from(msg: NexmarkMessage) -> Self {
        SourceMessage {
            payload: Some(msg.payload),
            offset: msg.sequence_number.clone(),
            split_id: msg.split_id,
        }
    }
}

impl NexmarkMessage {
    pub fn new_single_event(split_id: SplitId, offset: u64, event: Event) -> Self {
        NexmarkMessage {
            split_id,
            sequence_number: offset.to_string(),
            payload: match &event {
                Event::Person(p) => serde_json::to_string(p),
                Event::Auction(a) => serde_json::to_string(a),
                Event::Bid(b) => serde_json::to_string(b),
            }
            .unwrap()
            .into(),
        }
    }

    pub fn new_combined_event(split_id: SplitId, offset: u64, event: Event) -> Self {
        let combined_event = match event {
            Event::Person(p) => CombinedEvent::person(p),
            Event::Auction(a) => CombinedEvent::auction(a),
            Event::Bid(b) => CombinedEvent::bid(b),
        };
        let combined_event = serde_json::to_string(&combined_event).unwrap();
        NexmarkMessage {
            split_id,
            sequence_number: offset.to_string(),
            payload: combined_event.into(),
        }
    }
}
