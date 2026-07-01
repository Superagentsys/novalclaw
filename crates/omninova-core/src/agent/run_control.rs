use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::Notify;

#[derive(Clone, Debug)]
pub struct AgentCancellationToken {
    inner: Arc<AgentCancellationInner>,
}

#[derive(Debug)]
struct AgentCancellationInner {
    cancelled: AtomicBool,
    notify: Notify,
}

impl AgentCancellationToken {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AgentCancellationInner {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    pub fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::SeqCst) {
            self.inner.notify.notify_waiters();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.inner.notify.notified().await;
    }

    pub fn check(&self) -> anyhow::Result<()> {
        if self.is_cancelled() {
            Err(anyhow::anyhow!("agent run cancelled"))
        } else {
            Ok(())
        }
    }
}

impl Default for AgentCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
