use banager_core::events::EventSink;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UiEvent {
    Operation(banager_core::events::OperationEvent),
    SnapshotChanged {
        generation: u64,
    },
    /// What the first refresh round since launch found installed, before
    /// its update checks are done (`Session::refresh_recording`'s
    /// `preview`). The page shows the Installed list from it, every Update
    /// and Uninstall off, while it still has only the startup placeholder,
    /// and drops it when a snapshot arrives. Not a snapshot: nothing was
    /// committed, so `get_snapshot` still answers with the placeholder.
    /// Sent by `ipc::refresh_for` alone.
    InventoryPreview(banager_core::session::InventoryPreview),
    /// What `get_sizes` answers has moved, for the snapshot of `round`
    /// (`EventSink::sizes_changed`): the window asks again. A struct
    /// variant, so it is an object on the wire as the others are --
    /// `src/lib/events.ts` tells them apart with `in`.
    SizesChanged {
        round: u64,
    },
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

    fn sizes_changed(&self, round: u64) {
        self.broadcast(UiEvent::SizesChanged { round });
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
    fn test_inventory_preview_wire_shape_is_what_the_typescript_mirror_expects() {
        // Byte for byte what `src/lib/types.test.ts` builds as a typed
        // `UiEvent`: a newtype variant, so a one-key object carrying the
        // preview, its fields in declaration order.
        let empty = UiEvent::InventoryPreview(banager_core::session::InventoryPreview {
            round: 1,
            instances: vec![],
            artifacts: vec![],
        });
        assert_eq!(
            serde_json::to_string(&empty).expect("serialize"),
            r#"{"InventoryPreview":{"round":1,"instances":[],"artifacts":[]}}"#
        );

        // And a full one comes back whole through a Channel, as the page
        // receives it.
        let preview = banager_core::session::InventoryPreview {
            round: 3,
            instances: vec![banager_core::testing::manager_instance("brew", "brew:1")],
            artifacts: vec![banager_core::model::InstalledArtifact {
                key: banager_core::model::ArtifactKey {
                    instance_id: "brew:1".to_string(),
                    kind: banager_core::model::ArtifactKind::Formula,
                    name: "jq".to_string(),
                },
                display_name: "jq".to_string(),
                version: "1.8.2".to_string(),
                reason: banager_core::model::InstallReason::Requested,
                description: None,
                homepage: None,
                size_bytes: None,
                installed_at: None,
                path: None,
                auto_updates: false,
                uninstall_blocked: None,
                facts: Default::default(),
            }],
        };
        let sink = ChannelSink::new();
        let received: Arc<Mutex<Vec<UiEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        sink.register(channel);
        sink.broadcast(UiEvent::InventoryPreview(preview.clone()));
        let events = received.lock().unwrap();
        match events.as_slice() {
            [UiEvent::InventoryPreview(back)] => assert_eq!(*back, preview),
            other => panic!("expected one InventoryPreview, got {other:?}"),
        };
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

    #[test]
    fn test_sizes_changed_reaches_the_window_as_an_object_with_its_round() {
        // `src/lib/types.ts` spells it `{ SizesChanged: { round: number } }`
        // and `src/lib/events.ts` tells it from the other two by `in`,
        // which a bare string would make throw.
        let sink = ChannelSink::new();
        let received: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: serde_json::Value = body.deserialize().expect("deserialize");
            r.lock().unwrap().push(event);
            Ok(())
        });
        sink.register(channel);
        EventSink::sizes_changed(sink.as_ref(), 12);
        assert_eq!(
            received.lock().unwrap().clone(),
            vec![serde_json::json!({ "SizesChanged": { "round": 12 } })]
        );
        assert_eq!(
            serde_json::to_string(&UiEvent::SizesChanged { round: 12 }).unwrap(),
            r#"{"SizesChanged":{"round":12}}"#
        );
    }
}
