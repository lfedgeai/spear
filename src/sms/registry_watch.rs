//! Registry watch utility (revision + bounded recent events + broadcast)
//! 注册表 watch 工具（revision + 有界最近事件 + 广播）

use futures::Stream;
use futures::StreamExt;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{broadcast, Mutex};
use tonic::Status;

pub type WatchStream<E> = Pin<Box<dyn Stream<Item = Result<E, Status>> + Send + 'static>>;

#[derive(Debug)]
pub struct RegistryWatchHub<E> {
    revision: AtomicU64,
    recent_events: Mutex<VecDeque<E>>,
    event_tx: broadcast::Sender<E>,
    max_recent_events: usize,
}

impl<E> RegistryWatchHub<E>
where
    E: Clone + Send + Sync + 'static,
{
    pub fn new(max_recent_events: usize, broadcast_buffer_size: usize) -> Self {
        let (event_tx, _rx) = broadcast::channel(broadcast_buffer_size.max(1));
        Self {
            revision: AtomicU64::new(0),
            recent_events: Mutex::new(VecDeque::with_capacity(max_recent_events.max(1))),
            event_tx,
            max_recent_events: max_recent_events.max(1),
        }
    }

    pub fn current_revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    pub fn bump_revision(&self) -> u64 {
        self.revision
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }

    pub async fn push_event(&self, event: E) {
        {
            let mut events = self.recent_events.lock().await;
            if events.len() >= self.max_recent_events {
                events.pop_front();
            }
            events.push_back(event.clone());
        }
        let _ = self.event_tx.send(event);
    }

    pub async fn watch(
        &self,
        since_revision: u64,
        revision_of: fn(&E) -> u64,
    ) -> Result<WatchStream<E>, Status> {
        let cursor_revision = since_revision;
        let mut pending = VecDeque::new();
        {
            let events = self.recent_events.lock().await;
            if let Some(oldest) = events.front().map(revision_of) {
                if cursor_revision != 0 && cursor_revision < oldest {
                    return Err(Status::failed_precondition(
                        "since_revision too old; resync required",
                    ));
                }
            }
            for e in events.iter() {
                if revision_of(e) > cursor_revision {
                    pending.push_back(e.clone());
                }
            }
        }

        let rx = self.event_tx.subscribe();
        let pending_stream = tokio_stream::iter(pending.into_iter().map(Ok));
        let live_stream = futures::stream::unfold(rx, |mut r| async move {
            match r.recv().await {
                Ok(event) => Some((Ok(event), r)),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    Some((Err(Status::aborted("watch lagged; resync required")), r))
                }
                Err(broadcast::error::RecvError::Closed) => None,
            }
        });
        Ok(Box::pin(pending_stream.chain(live_stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[derive(Clone, Debug)]
    struct TestEvent {
        revision: u64,
    }

    #[tokio::test]
    async fn test_watch_since_too_old_requires_resync() {
        let hub = RegistryWatchHub::<TestEvent>::new(2, 16);
        hub.push_event(TestEvent {
            revision: 10,
        })
        .await;
        hub.push_event(TestEvent {
            revision: 11,
        })
        .await;

        let err = match hub.watch(9, |e| e.revision).await {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    }

    #[tokio::test]
    async fn test_watch_lagged_requires_resync() {
        let hub = RegistryWatchHub::<TestEvent>::new(8, 1);
        let mut stream = hub.watch(0, |e| e.revision).await.unwrap();

        for i in 0..16u64 {
            hub.push_event(TestEvent {
                revision: 100 + i,
            })
            .await;
        }

        let first = stream.next().await.unwrap();
        assert_eq!(first.unwrap_err().code(), tonic::Code::Aborted);
    }
}
