use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;

use super::{AddOptions, AddOutcome, AddSource, EngineError, TorrentEngine};
use crate::domain::{Snapshot, TorrentDetails, TorrentId};

#[derive(Default)]
pub struct FakeEngine {
    state: Mutex<FakeState>,
}

#[derive(Default)]
pub struct FakeState {
    pub snapshot: Snapshot,
    pub details: Option<TorrentDetails>,
    pub calls: Vec<String>,
}

impl FakeEngine {
    pub fn set_snapshot(&self, snapshot: Snapshot) {
        self.lock().snapshot = snapshot;
    }

    pub fn set_details(&self, details: TorrentDetails) {
        self.lock().details = Some(details);
    }

    pub fn calls(&self) -> Vec<String> {
        self.lock().calls.clone()
    }

    fn lock(&self) -> MutexGuard<'_, FakeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl TorrentEngine for FakeEngine {
    async fn snapshot(&self) -> Snapshot {
        self.lock().snapshot.clone()
    }

    async fn details(&self, id: TorrentId) -> Result<TorrentDetails, EngineError> {
        let mut state = self.lock();
        state.calls.push(format!("details {id}"));
        state
            .details
            .clone()
            .ok_or_else(|| EngineError::Failed("no details configured".to_owned()))
    }

    async fn add(&self, source: AddSource, options: AddOptions) -> Result<AddOutcome, EngineError> {
        let mut state = self.lock();
        state.calls.push(format!(
            "add {} paused={}",
            source.display(),
            options.paused
        ));
        Err(EngineError::Failed(
            "not supported in fake engine".to_owned(),
        ))
    }

    async fn pause(&self, id: TorrentId) -> Result<(), EngineError> {
        self.lock().calls.push(format!("pause {id}"));
        Ok(())
    }

    async fn resume(&self, id: TorrentId) -> Result<(), EngineError> {
        self.lock().calls.push(format!("resume {id}"));
        Ok(())
    }

    async fn remove(&self, id: TorrentId, delete_files: bool) -> Result<(), EngineError> {
        self.lock()
            .calls
            .push(format!("remove {id} delete_files={delete_files}"));
        Ok(())
    }

    async fn set_included_files(
        &self,
        id: TorrentId,
        included: Vec<usize>,
    ) -> Result<(), EngineError> {
        self.lock()
            .calls
            .push(format!("set_included_files {id} {included:?}"));
        Ok(())
    }

    async fn shutdown(&self) {
        self.lock().calls.push("shutdown".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_calls_in_order() {
        let engine = FakeEngine::default();
        engine.pause(TorrentId(1)).await.expect("pause works");
        engine.resume(TorrentId(1)).await.expect("resume works");
        engine
            .remove(TorrentId(2), true)
            .await
            .expect("remove works");
        engine
            .set_included_files(TorrentId(2), vec![0, 2])
            .await
            .expect("files work");
        engine.shutdown().await;

        assert_eq!(
            engine.calls(),
            vec![
                "pause 1",
                "resume 1",
                "remove 2 delete_files=true",
                "set_included_files 2 [0, 2]",
                "shutdown",
            ]
        );
    }
}
