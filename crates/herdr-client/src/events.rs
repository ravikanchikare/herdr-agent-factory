//! The long-lived Herdr subscription.
//!
//! Herdr is the authority on pane and agent lifecycle state. Agent Factory keeps
//! one subscription open and treats every event as an invalidation of its live
//! snapshot. The reader runs on its own thread and hands invalidation markers
//! to the runtime through a channel it drains each tick.

use std::collections::BTreeSet;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

use crate::error::HerdrError;
use crate::model::GLOBAL_SUBSCRIPTIONS;
use crate::transport::{Connection, into_result};

pub struct HerdrEvents {
    receiver: Receiver<()>,
    shutdown: UnixStream,
    connected: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    agent_pane_ids: BTreeSet<String>,
}

impl HerdrEvents {
    pub(crate) fn open(
        socket: &Path,
        timeout: Duration,
        agent_pane_ids: impl IntoIterator<Item = String>,
    ) -> Result<Self, HerdrError> {
        let mut connection = Connection::open(socket, timeout)?;
        let agent_pane_ids = agent_pane_ids.into_iter().collect::<BTreeSet<_>>();
        let mut subscriptions = GLOBAL_SUBSCRIPTIONS
            .iter()
            .map(|topic| json!({"type": topic}))
            .collect::<Vec<_>>();
        subscriptions.extend(agent_pane_ids.iter().map(|pane_id| {
            json!({
                "type": "pane.agent_status_changed",
                "pane_id": pane_id,
            })
        }));
        connection.send("events.subscribe", json!({"subscriptions": subscriptions}))?;

        let acknowledgement = connection
            .read_frame()?
            .ok_or_else(|| HerdrError::Protocol("Herdr closed the event stream".into()))?;
        let result = into_result(acknowledgement)?;
        if result.get("type").and_then(Value::as_str) != Some("subscription_started") {
            return Err(HerdrError::Protocol(
                "Herdr did not acknowledge the subscription".into(),
            ));
        }

        connection.clear_read_timeout()?;
        let shutdown = connection.shutdown_handle()?;
        let connected = Arc::new(AtomicBool::new(true));
        let (sender, receiver) = channel();
        let live = Arc::clone(&connected);
        let reader = std::thread::Builder::new()
            .name("herdr-events".into())
            .spawn(move || {
                while let Ok(Some(frame)) = connection.read_frame() {
                    if frame.get("event").and_then(Value::as_str).is_none()
                        || frame.get("data").is_none()
                    {
                        continue;
                    }
                    if sender.send(()).is_err() {
                        break;
                    }
                }
                live.store(false, Ordering::Release);
            })?;

        Ok(Self {
            receiver,
            shutdown,
            connected,
            reader: Some(reader),
            agent_pane_ids,
        })
    }

    /// Count every invalidation received since the last drain.
    pub fn drain(&self) -> usize {
        let mut count = 0;
        while self.receiver.try_recv().is_ok() {
            count += 1;
        }
        count
    }

    /// Whether the stream is still live. A dropped stream means Herdr went away.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    pub fn covers_agent_panes(&self, pane_ids: &BTreeSet<String>) -> bool {
        self.agent_pane_ids == *pane_ids
    }
}

impl Drop for HerdrEvents {
    fn drop(&mut self) {
        let _ = self.shutdown.shutdown(Shutdown::Both);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
