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

use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

#[cfg(any(test, feature = "test"))]
use risingwave_common::monitor::object_metrics::ObjectStoreMetrics;
use risingwave_common::object::ObjectStoreRef;
#[cfg(any(test, feature = "test"))]
use risingwave_common::object::{InMemObjectStore, ObjectStoreImpl};
use risingwave_rpc_client::{StreamClientPool, StreamClientPoolRef};

use super::{HashMappingManager, HashMappingManagerRef};
use crate::manager::{
    IdGeneratorManager, IdGeneratorManagerRef, NotificationManager, NotificationManagerRef,
};
#[cfg(any(test, feature = "test"))]
use crate::storage::MemStore;
use crate::storage::MetaStore;

/// [`MetaSrvEnv`] is the global environment in Meta service. The instance will be shared by all
/// kind of managers inside Meta.
#[derive(Clone)]
pub struct MetaSrvEnv<S>
where
    S: MetaStore,
{
    /// id generator manager.
    id_gen_manager: IdGeneratorManagerRef<S>,

    /// meta store.
    meta_store: Arc<S>,

    /// object store.
    object_store: ObjectStoreRef,

    /// notification manager.
    notification_manager: NotificationManagerRef,

    /// hash mapping manager.
    hash_mapping_manager: HashMappingManagerRef,

    /// stream client pool memorization.
    stream_client_pool: StreamClientPoolRef,

    /// options read by all services
    pub opts: Arc<MetaOpts>,
}

/// Options shared by all meta service instances
pub struct MetaOpts {
    pub enable_recovery: bool,
    pub checkpoint_interval: Duration,
}

impl Default for MetaOpts {
    fn default() -> Self {
        Self {
            enable_recovery: false,
            checkpoint_interval: Duration::from_millis(100),
        }
    }
}

impl<S> MetaSrvEnv<S>
where
    S: MetaStore,
{
    pub async fn new(opts: MetaOpts, meta_store: Arc<S>, object_store: ObjectStoreRef) -> Self {
        // change to sync after refactor `IdGeneratorManager::new` sync.
        let id_gen_manager = Arc::new(IdGeneratorManager::new(meta_store.clone()).await);
        let stream_client_pool = Arc::new(StreamClientPool::default());
        let notification_manager = Arc::new(NotificationManager::new());
        let hash_mapping_manager = Arc::new(HashMappingManager::new());

        Self {
            id_gen_manager,
            meta_store,
            object_store,
            notification_manager,
            hash_mapping_manager,
            stream_client_pool,
            opts: opts.into(),
        }
    }

    pub fn meta_store_ref(&self) -> Arc<S> {
        self.meta_store.clone()
    }

    pub fn meta_store(&self) -> &S {
        self.meta_store.deref()
    }

    pub fn object_store(&self) -> &ObjectStoreRef {
        &self.object_store
    }

    pub fn id_gen_manager_ref(&self) -> IdGeneratorManagerRef<S> {
        self.id_gen_manager.clone()
    }

    pub fn id_gen_manager(&self) -> &IdGeneratorManager<S> {
        self.id_gen_manager.deref()
    }

    pub fn notification_manager_ref(&self) -> NotificationManagerRef {
        self.notification_manager.clone()
    }

    pub fn notification_manager(&self) -> &NotificationManager {
        self.notification_manager.deref()
    }

    pub fn hash_mapping_manager_ref(&self) -> HashMappingManagerRef {
        self.hash_mapping_manager.clone()
    }

    pub fn hash_mapping_manager(&self) -> &HashMappingManager {
        self.hash_mapping_manager.deref()
    }

    pub fn stream_client_pool_ref(&self) -> StreamClientPoolRef {
        self.stream_client_pool.clone()
    }

    pub fn stream_client_pool(&self) -> &StreamClientPool {
        self.stream_client_pool.deref()
    }
}

#[cfg(any(test, feature = "test"))]
impl MetaSrvEnv<MemStore> {
    // Instance for test.
    pub async fn for_test() -> Self {
        // change to sync after refactor `IdGeneratorManager::new` sync.
        let meta_store = Arc::new(MemStore::default());
        let object_store = Arc::new(ObjectStoreImpl::new(
            Box::new(InMemObjectStore::new(false)),
            Arc::new(ObjectStoreMetrics::unused()),
        ));
        let id_gen_manager = Arc::new(IdGeneratorManager::new(meta_store.clone()).await);
        let notification_manager = Arc::new(NotificationManager::new());
        let stream_client_pool = Arc::new(StreamClientPool::default());
        let hash_mapping_manager = Arc::new(HashMappingManager::new());

        Self {
            id_gen_manager,
            meta_store,
            object_store,
            notification_manager,
            hash_mapping_manager,
            stream_client_pool,
            opts: MetaOpts::default().into(),
        }
    }
}
