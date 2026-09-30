use banager_core::events::EventSink;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UiEvent {
    Operation(banager_core::events::OperationEvent),
    SnapshotChanged { generation: u64 },
}

/// Fans every core event out to all registered Channels; a Channel whose
/// send fails (window closed) is dropped from the registry.
pub struct ChannelSink {
    channels: Mutex<Vec<Channel<UiEvent>>>,
}

impl ChannelSink {
    pub fn new() -> Arc<ChannelSink> {
        Arc::new(ChannelSink {
            channels: Mutex::new(Vec::new()),
        })
    }

    pub fn register(&self, channel: Channel<UiEvent>) {
        self.channels.lock().unwrap().push(channel);
    }

    pub fn broadcast(&self, event: UiEvent) {
        let mut channels = self.channels.lock().unwrap();
        channels.retain(|c| c.send(event.clone()).is_ok());
    }
}

impl EventSink for ChannelSink {
    fn emit(&self, event: banager_core::events::OperationEvent) {
        self.broadcast(UiEvent::Operation(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use banager_core::events::OperationEvent;
    use banager_core::model::OpStatus;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_broadcast_delivers_to_every_registered_channel() {
        let sink = ChannelSink::new();
        let received_a: Arc<Mutex<Vec<UiEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let received_b: Arc<Mutex<Vec<UiEvent>>> = Arc::new(Mutex::new(Vec::new()));

        let ra = received_a.clone();
        let channel_a: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            ra.lock().unwrap().push(event);
            Ok(())
        });
        let rb = received_b.clone();
        let channel_b: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            rb.lock().unwrap().push(event);
            Ok(())
        });

        sink.register(channel_a);
        sink.register(channel_b);
        sink.broadcast(UiEvent::SnapshotChanged { generation: 7 });

        assert_eq!(received_a.lock().unwrap().len(), 1);
        assert_eq!(received_b.lock().unwrap().len(), 1);
        // Bind the guard first, then match on it, and terminate the match
        // with a semicolon: matching directly on `&received_a.lock().unwrap()[0]`
        // as this function's tail expression makes the temporary `MutexGuard`
        // outlive the match (its drop is deferred to the end of the
        // enclosing statement, which here is the whole function body),
        // which rustc rejects with E0597 ("borrowed value does not live
        // long enough") since Rust 2021.
        let events = received_a.lock().unwrap();
        match &events[0] {
            UiEvent::SnapshotChanged { generation } => assert_eq!(*generation, 7),
            other => panic!("expected SnapshotChanged, got {other:?}"),
        };
    }

    #[test]
    fn test_broadcast_removes_a_channel_from_the_registry_after_its_send_fails() {
        // This manufactures the send failure directly; it does not — and,
        // from a unit test with no real webview, cannot — prove anything
        // about when (or whether) a real closed window actually makes
        // Channel::send fail. See this task's Interfaces note (N2 in the
        // design review): that must be confirmed by hand later, against a
        // running app.
        let sink = ChannelSink::new();
        let ok_count = Arc::new(AtomicUsize::new(0));

        let failing: Channel<UiEvent> = Channel::new(|_body| -> tauri::Result<()> {
            Err(tauri::Error::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "simulated send failure, e.g. from a closed window",
            )))
        });
        let oc = ok_count.clone();
        let healthy: Channel<UiEvent> = Channel::new(move |_body| {
            oc.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        sink.register(failing);
        sink.register(healthy);

        sink.broadcast(UiEvent::SnapshotChanged { generation: 1 });
        assert_eq!(ok_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            sink.channels.lock().unwrap().len(),
            1,
            "the failing channel must be dropped"
        );

        sink.broadcast(UiEvent::SnapshotChanged { generation: 2 });
        assert_eq!(ok_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_emit_wraps_operation_events_as_ui_event_operation() {
        let sink = ChannelSink::new();
        let received: Arc<Mutex<Vec<UiEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        sink.register(channel);

        EventSink::emit(
            sink.as_ref(),
            OperationEvent::Status {
                op_id: 1,
                status: OpStatus::Running,
            },
        );

        let events = received.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            UiEvent::Operation(OperationEvent::Status { op_id, status }) => {
                assert_eq!(*op_id, 1);
                assert_eq!(*status, OpStatus::Running);
            }
            other => panic!("expected Operation(Status), got {other:?}"),
        }
    }
}
