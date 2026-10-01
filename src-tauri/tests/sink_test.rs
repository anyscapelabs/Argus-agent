//! Where a turn's events go, and how a running command reaches the user.
//!
//! This is the `BusSink` half of the split that used to hide in plain sight.
//! Events were threaded down from the sink as a borrowed Tauri
//! `Channel<StreamEvent>`, and only a raw channel could supply one — so in the
//! chat path, the one that passes a `BusSink`, a streaming reply and every byte
//! of a running command's output went nowhere at all. The turn still completed
//! and the answer still appeared, from the database, which is why it read as
//! slowness rather than as a bug.

use argus_lib::gateway::schema::StreamEvent;
use argus_lib::gateway::EventSink;
use argus_lib::gateway::Gateway;
use argus_lib::sessions::sink::{BusSink, ChatSink};

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

fn test_gw() -> Gateway {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();

    Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: PathBuf::from("/tmp"),
        library_dir: PathBuf::from("/tmp"),
        logos_dir: PathBuf::from("/tmp"),
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
        jobs: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        turns: Mutex::new(HashSet::new()),
        watching: Mutex::new(HashSet::new()),
    }
}

/// The bus transport has to actually carry an event. `run_child`'s pumps are
/// spawned, so they get an owned `EventSink` rather than a borrow, and this is
/// that owned thing doing its one job.
#[tokio::test]
async fn a_bus_sink_carries_events_to_a_subscriber() {
    let (tx, mut rx) = tokio::sync::broadcast::channel(8);
    let sink = EventSink::Bus(tx);

    sink.send(StreamEvent::Term {
        idx: 3,
        chunk: "cloning".into(),
    })
    .expect("a live receiver means a good send");

    match rx.recv().await.expect("the pump sent an event") {
        StreamEvent::Term { idx, chunk } => {
            assert_eq!(idx, 3);
            assert_eq!(chunk, "cloning");
        }
        other => panic!("wrong event: {other:?}"),
    }
}

/// `EventSink::null()` stands in for a caller that does not want events, such
/// as a title-generation pass. With no receiver on the far side the send
/// fails, and that has to be a value the caller can ignore rather than a panic.
#[test]
fn the_null_sink_discards_without_panicking() {
    EventSink::null()
        .send(StreamEvent::Term {
            idx: 0,
            chunk: "dropped".into(),
        })
        .ok();
}

/// The regression itself: a `BusSink` has to be able to hand out an emitter,
/// because the chat path passes one and everything else went silent when it
/// could not. Returning `None` here is what starved a running command's output.
#[tokio::test]
async fn a_bus_sink_hands_out_an_emitter_that_reaches_the_session() {
    let gw = test_gw();
    let sink = BusSink {
        gw: &gw,
        session_id: "s1".into(),
    };

    let ev = sink
        .event_sink()
        .expect("the chat path's sink must be able to emit from a spawned task");

    let mut rx = gw.subscribe("s1");

    // What `run_child` does with the sink it was handed, from its own task.
    ev.send(StreamEvent::Term {
        idx: 0,
        chunk: "Cloning into 'repo'...".into(),
    })
    .expect("a live subscriber means a good send");

    match rx
        .recv()
        .await
        .expect("the command's output reached the bus")
    {
        StreamEvent::Term { chunk, .. } => assert!(chunk.contains("Cloning")),
        other => panic!("wrong event: {other:?}"),
    }
}

/// `emit` on the sink and the emitter it hands out have to reach the same
/// place, or the turn would show one set of events and a running command
/// another.
#[tokio::test]
async fn both_routes_off_a_bus_sink_reach_the_same_bus() {
    let gw = test_gw();
    let sink = BusSink {
        gw: &gw,
        session_id: "s2".into(),
    };
    let mut rx = gw.subscribe("s2");

    let ev = sink.event_sink().expect("a bus sink must emit");
    sink.emit(StreamEvent::Term {
        idx: 1,
        chunk: "direct".into(),
    });
    ev.send(StreamEvent::Term {
        idx: 2,
        chunk: "emitter".into(),
    })
    .expect("a live subscriber means a good send");

    let seen: Vec<String> = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut out = vec![];
        while out.len() < 2 {
            match rx.recv().await {
                Ok(StreamEvent::Term { chunk, .. }) => out.push(chunk),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        out
    })
    .await
    .unwrap_or_default();

    assert_eq!(seen, vec!["direct", "emitter"]);
}
