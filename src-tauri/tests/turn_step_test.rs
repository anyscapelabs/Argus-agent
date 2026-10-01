//! The step boundary, driven through the real loop.
//!
//! `StreamEvent::Step` is the only event the frontend uses to clear its text
//! buffer (`src/stores/sessions.ts`). It used to be emitted *after* the
//! tool-execution loop, so it only ever fired on a step that ran tools. The
//! five paths that `continue` past that point — empty-reply retry,
//! `Truncation::Retry`, the claim nudge, the forced summary, reflection —
//! had already persisted their reply as a row and never cleared the buffer.
//! The next step then streamed on top of it, and `ChatTranscript` stacks the
//! live row above the loaded rows, so the same reply rendered twice.
//!
//! There was no network seam in `send()` before this, which is why the loop
//! had no test. There is one now: `router::key_for` exempts a local base_url
//! from the keyring, so a `127.0.0.1` mock provider drives the genuine
//! `stream_run` -> adapter -> SSE path with nothing stubbed but the socket.
//! The `&dyn ChatSink` seam records the event order, which is the whole
//! contract.

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use argus_lib::gateway::schema::{Provider, StreamEvent};
use argus_lib::gateway::{EventSink, Gateway};
use argus_lib::sessions::schema::{NewMsg, NewSession};
use argus_lib::sessions::sink::ChatSink;
use argus_lib::sessions::turn::Turn;

const MODEL: &str = "mock-model";
const PROVIDER: &str = "mock-prov";

/// One event as the frontend receives it.
///
/// `StreamEvent` is `Serialize` only, so the test reads the wire the store
/// reads rather than a Rust mirror of it. That is the more honest instrument
/// anyway: the store dispatches on `ev.type`, and a tag renamed on either
/// side should break here rather than pass on a stale copy.
#[derive(Debug, Clone, PartialEq)]
enum Ev {
    Delta(String),
    Step,
    Reset,
    /// Anything else, by its tag. Kept so the log stays a faithful record.
    Other(String),
}

impl Ev {
    fn parse(body: tauri::ipc::InvokeResponseBody) -> Option<Self> {
        let v: serde_json::Value = body.deserialize().ok()?;
        let ty = v["type"].as_str()?;

        Some(match ty {
            "delta" => Ev::Delta(v["text"].as_str().unwrap_or_default().to_string()),
            "step" => Ev::Step,
            "reset" => Ev::Reset,
            other => Ev::Other(other.to_string()),
        })
    }
}

/// Records what the turn emitted, in order. The assertion is about sequence,
/// so a set would hide the very bug this pins.
///
/// The log is fed by two paths, because the loop uses two: model text goes to
/// the `Channel` the turn is handed, while the step boundary goes to the
/// sink. The frontend sees both on one wire, so the test has to as well —
/// recording only the sink sees no deltas at all, and every buffer assertion
/// below would pass vacuously.
#[derive(Clone)]
struct Recorder {
    evs: Arc<Mutex<Vec<Ev>>>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            evs: Arc::new(Mutex::new(vec![])),
        }
    }

    fn push(&self, ev: Ev) {
        self.evs.lock().unwrap().push(ev);
    }

    /// A channel that lands in this same log, in the order it was sent.
    fn chan(&self) -> tauri::ipc::Channel<StreamEvent> {
        let me = self.clone();
        tauri::ipc::Channel::new(move |body| {
            if let Some(ev) = Ev::parse(body) {
                me.push(ev);
            }
            Ok(())
        })
    }

    fn deltas(&self) -> Vec<String> {
        self.evs
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Ev::Delta(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every offset in the event log at which the buffer would be cleared.
    fn clears(&self) -> Vec<usize> {
        self.evs
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, Ev::Step | Ev::Reset))
            .map(|(i, _)| i)
            .collect()
    }

    fn has_reset(&self) -> bool {
        self.evs
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Ev::Reset))
    }

    /// Replay the stream the way the store does, and return the buffer at the
    /// moment of the nth clear. If the loop duplicates a reply this shows it:
    /// the buffer holds two replies where the transcript has one.
    fn buffer_at_clear(&self, nth: usize) -> String {
        let mut buf = String::new();
        let mut seen = 0;

        for ev in self.evs.lock().unwrap().iter() {
            match ev {
                Ev::Delta(text) => buf.push_str(text),
                Ev::Step | Ev::Reset => {
                    if seen == nth {
                        return buf;
                    }
                    seen += 1;
                    buf.clear();
                }
                _ => {}
            }
        }

        buf
    }
}

impl ChatSink for Recorder {
    fn emit(&self, ev: StreamEvent) {
        // Round-trip through the wire so both paths land in one vocabulary.
        if let Ok(body) = serde_json::to_value(&ev) {
            if let Some(parsed) = Ev::parse(body.to_string().into()) {
                self.push(parsed);
            }
        }
    }
}

fn gw_and_dirs(tag: &str) -> (Gateway, std::path::PathBuf) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let base = std::env::temp_dir().join(format!("argus-turn-step-{}-{tag}", std::process::id()));
    for sub in ["skills", "library", "logos"] {
        std::fs::create_dir_all(base.join(sub)).unwrap();
    }

    let gw = Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: base.join("skills"),
        library_dir: base.join("library"),
        logos_dir: base.join("logos"),
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
        jobs_dir: base.join("jobs"),
        jobs: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        turns: Mutex::new(HashSet::new()),
        watching: Mutex::new(HashSet::new()),
    };

    (gw, base)
}

/// Point a model at the mock socket. `connected = 1` is what `list_avail`
/// filters on, and a `127.0.0.1` base_url is what `key_for` accepts without a
/// keyring entry.
fn wire_provider(gw: &Gateway, base_url: &str) {
    let conn = gw.conn.lock().unwrap();

    argus_lib::gateway::store::upsert_provider(
        &conn,
        &Provider {
            id: PROVIDER.into(),
            name: "mock".into(),
            compatible: "openai".into(),
            base_url: base_url.into(),
            api_key_ref: None,
            connected: true,
            free: true,
            priority: 0,
            logo_url: None,
            doc_url: None,
        },
    )
    .unwrap();

    conn.execute(
        "INSERT OR REPLACE INTO models (id, display_name, enabled) VALUES (?1, ?2, 1)",
        rusqlite::params![MODEL, "mock"],
    )
    .unwrap();

    conn.execute(
        "INSERT OR REPLACE INTO model_providers (model_id, provider_id, remote_model_id)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![MODEL, PROVIDER, "mock-remote"],
    )
    .unwrap();
}

fn new_session(gw: &Gateway) -> String {
    let conn = gw.conn.lock().unwrap();
    let s = argus_lib::sessions::store::create_session(
        &conn,
        &NewSession {
            title: "t".into(),
            model_id: Some(MODEL.into()),
            permission: Some("never".into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap();
    s.id
}

fn add_user(gw: &Gateway, session_id: &str, content: &str) {
    let conn = gw.conn.lock().unwrap();
    argus_lib::sessions::store::add_msg(
        &conn,
        &NewMsg {
            session_id: session_id.into(),
            role: "user".into(),
            content: content.into(),
            ..Default::default()
        },
    )
    .unwrap();
}

fn assistant_rows(gw: &Gateway, session_id: &str) -> Vec<String> {
    let conn = gw.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT content FROM messages WHERE session_id = ?1 AND role = 'assistant' ORDER BY seq")
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![session_id], |r| r.get::<_, String>(0))
        .unwrap();

    rows.map(|r| r.unwrap()).collect()
}

/// One SSE reply, chunked the way a provider chunks it: several deltas and a
/// terminator. Returns the raw bytes so a test can hand back something
/// deliberately broken.
fn sse(chunks: &[&str], finish: &str) -> String {
    let mut out = String::new();

    for c in chunks {
        out.push_str(&format!(
            "data: {}\n\n",
            serde_json::json!({
                "choices": [{ "delta": { "content": c } }]
            })
        ));
    }

    out.push_str(&format!(
        "data: {}\n\n",
        serde_json::json!({
            "choices": [{ "delta": {}, "finish_reason": finish }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5 }
        })
    ));

    out.push_str("data: [DONE]\n\n");
    out
}

/// Serve `replies` in order, one connection per request.
///
/// Every bound here exists so a wrong script fails fast and legibly instead of
/// stalling the suite. A turn that asks for one reply more than the script has
/// used to park the accept loop for good, and — once the listener was gone —
/// the client got a refused socket, which `CallError::retryable` calls
/// retryable, so the router walked the backoff ladder for minutes first.
const ACCEPT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const OVERSHOOT_GRACE: std::time::Duration = std::time::Duration::from_millis(500);

fn mock_provider(replies: Vec<String>) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();

    let handle = std::thread::spawn(move || {
        for body in replies {
            let Some(mut stream) = accept(&listener) else {
                return;
            };

            stream.set_nonblocking(false).ok();
            stream.set_read_timeout(Some(READ_TIMEOUT)).ok();

            drain_request(&mut stream);

            if write_reply(&mut stream, 200, "OK", &body).is_err() {
                return;
            }
        }

        refuse_overshoot(&listener);
    });

    (base_url, handle)
}

fn accept(listener: &std::net::TcpListener) -> Option<std::net::TcpStream> {
    let started = std::time::Instant::now();

    loop {
        match listener.accept() {
            Ok((s, _)) => return Some(s),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() > ACCEPT_DEADLINE {
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
}

/// Read a whole request: headers through the blank line, then exactly
/// `content-length` body bytes.
///
/// Reading once and answering is not enough, and the failure is nasty. A turn's
/// payload runs well past 8 KB, so the tail is still in flight when a single
/// `read` returns. Answering and dropping then leaves unread bytes queued, so
/// the kernel sends RST instead of FIN — and the client reads that as a
/// connection error carrying no status, which the router considers retryable.
/// It hung this file roughly one run in six, and the backoff it slept through
/// was minutes rather than milliseconds.
fn drain_request(stream: &mut std::net::TcpStream) {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];

    let head_end = loop {
        if let Some(at) = find_head_end(&buf) {
            break at;
        }

        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };

    let want = content_length(&buf[..head_end]);

    while buf.len() - head_end < want {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
}

fn content_length(head: &[u8]) -> usize {
    let text = String::from_utf8_lossy(head).to_lowercase();

    text.lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

fn write_reply(
    stream: &mut std::net::TcpStream,
    code: u16,
    reason: &str,
    body: &str,
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\ncontent-type: text/event-stream\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

/// Keep the port open briefly once the script is spent, answering `400`.
///
/// `400` is not retryable, so an over-long turn fails in milliseconds with the
/// status in the message. Letting the listener drop instead hands the client a
/// refused socket, which the router does retry — see `ACCEPT_DEADLINE`.
fn refuse_overshoot(listener: &std::net::TcpListener) {
    let started = std::time::Instant::now();

    while started.elapsed() <= OVERSHOOT_GRACE {
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            Err(_) => return,
        };

        stream.set_nonblocking(false).ok();
        stream.set_read_timeout(Some(READ_TIMEOUT)).ok();

        drain_request(&mut stream);
        let _ = write_reply(&mut stream, 400, "Bad Request", "mock script is spent");
    }
}

fn run_turn(
    gw: &Gateway,
    app: &tauri::AppHandle<tauri::test::MockRuntime>,
    session_id: &str,
    rec: &Recorder,
) -> Result<(), String> {
    let model_chan = EventSink::Channel(rec.chan());
    let mut turn = Turn::new();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(turn.run(
        gw,
        app,
        session_id,
        rec,
        &model_chan,
        "never",
        false,
        &[],
        1_000_000,
    ))
}

/// The regression itself.
///
/// Step 1 replies with prose and no `<final/>`, so the loop nudges and
/// `continue`s. Step 2 closes properly. Both persist an assistant row, so
/// the buffer must be handed over twice — once per row — or the second reply
/// is rendered on top of the first and the same words appear twice.
#[test]
fn a_nudged_reply_clears_the_buffer_before_the_next_one() {
    let first = "Working on it.";

    let (url, server) = mock_provider(vec![
        sse(&["Working ", "on it."], "stop"),
        sse(&["All ", "done.\n<final/>"], "stop"),
    ]);

    let (gw, _base) = gw_and_dirs("nudge");
    wire_provider(&gw, &url);
    let session_id = new_session(&gw);
    add_user(&gw, &session_id, "do the thing");

    let app = tauri::test::mock_app().handle().clone();
    let rec = Recorder::new();

    let out = run_turn(&gw, &app, &session_id, &rec);
    server.join().unwrap();

    assert!(out.is_ok(), "turn failed: {out:?}");

    let rows = assistant_rows(&gw, &session_id);
    assert_eq!(rows.len(), 2, "expected a row per step, got {rows:?}");

    // The invariant: one clear per persisted row, in order.
    let clears = rec.clears();
    assert!(
        clears.len() >= 2,
        "each persisted reply must hand over its buffer; got {} clear(s) for {} rows: {:?}",
        clears.len(),
        rows.len(),
        rec.deltas()
    );

    // And each hand-over carries exactly its own reply, never both. This is
    // the duplication: the buffer would read "Working on it.All done." while
    // the transcript already shows the first line as a row.
    //
    // The marker is still in the buffer — `split_commit` strips it for the
    // row, not for the stream the frontend is holding.
    assert_eq!(
        rec.buffer_at_clear(0),
        first,
        "the first clear must hand over the first reply and nothing else"
    );
    assert_eq!(
        rec.buffer_at_clear(1),
        "All done.\n<final/>",
        "the second clear must hand over the second reply, not the first one again"
    );
}

/// A complete answer missing only its close marker is finished, not broken.
/// Nudging it resends the whole answer, which is the duplication: the retry
/// streams on top of a row that already holds the same words.
#[test]
fn an_unclosed_answer_is_accepted_not_resent() {
    let answer = "Here is the full story of everything you asked about, written out \
         completely in this one reply with every finding included.";

    assert!(
        answer.chars().count() >= 100,
        "the fixture must read as an answer, not a stub"
    );

    // One reply in the script: a nudge would ask for a second and fail fast
    // on the spent mock (400, not retryable) instead of passing.
    let (url, server) = mock_provider(vec![sse(&[answer], "stop")]);

    let (gw, _base) = gw_and_dirs("unclosed");
    wire_provider(&gw, &url);
    let session_id = new_session(&gw);
    add_user(&gw, &session_id, "tell me everything");
    let app = tauri::test::mock_app().handle().clone();
    let rec = Recorder::new();

    let out = run_turn(&gw, &app, &session_id, &rec);
    server.join().unwrap();

    assert!(out.is_ok(), "turn failed: {out:?}");

    let rows = assistant_rows(&gw, &session_id);
    assert_eq!(rows.len(), 1, "one answer, one row: {rows:?}");
    assert!(rows[0].contains("full story"), "got {:?}", rows[0]);
}

/// An empty reply is thrown away, never persisted, so no `Step` is owed — but
/// the whitespace that did arrive is still in the buffer. `Reset` is what
/// clears it, and before the fix nothing did: the retry streamed on top of
/// the discarded whitespace.
#[test]
fn an_empty_reply_resets_the_buffer_it_leaves_behind() {
    let (url, server) = mock_provider(vec![
        sse(&["   ", "\n"], "stop"),
        sse(&["Recovered.\n<final/>"], "stop"),
    ]);

    let (gw, _base) = gw_and_dirs("empty");
    wire_provider(&gw, &url);
    let session_id = new_session(&gw);
    add_user(&gw, &session_id, "do the thing");

    let app = tauri::test::mock_app().handle().clone();
    let rec = Recorder::new();

    let out = run_turn(&gw, &app, &session_id, &rec);
    server.join().unwrap();

    assert!(out.is_ok(), "turn failed: {out:?}");
    assert!(
        rec.has_reset(),
        "a discarded reply must reset the buffer, not leave its whitespace in it"
    );

    // Whitespace is not a row, so the only persisted reply is the recovery.
    let rows = assistant_rows(&gw, &session_id);
    assert_eq!(rows.len(), 1, "empty replies are not persisted: {rows:?}");
}

/// The empty path gives up rather than loop forever, and it must not pretend
/// the turn produced anything. This pins the arming of `empty_retries` from
/// the other side: one retry, then the error.
///
/// The script is exactly two replies, and that count is the assertion. A
/// third request would find the listener gone — the serving thread exits once
/// it runs out — and fail on a connection error rather than the guard, which
/// is a different failure with a different message.
#[test]
fn a_second_empty_reply_ends_the_turn() {
    let (url, server) = mock_provider(vec![sse(&[" "], "stop"), sse(&["\t\n"], "stop")]);

    let (gw, _base) = gw_and_dirs("empty2");
    wire_provider(&gw, &url);
    let session_id = new_session(&gw);
    add_user(&gw, &session_id, "do the thing");

    let app = tauri::test::mock_app().handle().clone();
    let rec = Recorder::new();

    let out = run_turn(&gw, &app, &session_id, &rec);
    server.join().unwrap();

    let err = out.unwrap_err();
    assert!(
        err.contains("empty reply"),
        "expected the empty-reply guard to stop the turn, got {err:?}"
    );
    assert!(
        assistant_rows(&gw, &session_id).is_empty(),
        "nothing was said, so nothing may be stored"
    );
}

/// A turn whose first step is already final still clears exactly once. The
/// obvious over-correction — emitting `Step` on every path *and* keeping the
/// old one — would show up here as a clear with an empty buffer in front of
/// the row that follows it.
#[test]
fn a_single_final_reply_clears_once_and_hands_over_that_reply() {
    let (url, server) = mock_provider(vec![sse(&["Here you go.\n<final/>"], "stop")]);

    let (gw, _base) = gw_and_dirs("clean");
    wire_provider(&gw, &url);
    let session_id = new_session(&gw);
    add_user(&gw, &session_id, "do the thing");

    let app = tauri::test::mock_app().handle().clone();
    let rec = Recorder::new();

    let out = run_turn(&gw, &app, &session_id, &rec);
    server.join().unwrap();

    assert!(out.is_ok(), "turn failed: {out:?}");

    let rows = assistant_rows(&gw, &session_id);
    assert_eq!(rows.len(), 1, "{rows:?}");

    // The old emission site also ran on this path, so the fix must not have
    // left both in place.
    assert_eq!(
        rec.clears().len(),
        1,
        "one reply, one hand-over — got {:?}",
        rec.deltas()
    );
    assert_eq!(
        rec.buffer_at_clear(0),
        "Here you go.\n<final/>",
        "the hand-over must carry the reply, not an empty or doubled buffer"
    );
}

/// The mock must be reachable the way the router reaches it, or the test
/// proves nothing about the loop. This asserts the socket and the SSE frame
/// the adapter parses are the ones the test believes in.
#[test]
fn the_mock_speaks_the_wire_the_adapter_reads() {
    let (url, server) = mock_provider(vec![sse(&["alpha", " beta"], "stop")]);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let text = rt.block_on(async {
        let http = reqwest::Client::new();
        let mut deltas: Vec<String> = vec![];
        let mut sink = |c: &str| -> Result<(), String> {
            deltas.push(c.to_string());
            Ok(())
        };

        let done = argus_lib::gateway::adapters::openai_compat::stream(
            &http,
            &url,
            None,
            "mock-remote",
            &[],
            &[],
            &mut sink,
        )
        .await
        .unwrap();

        assert_eq!(done.tok_in, Some(10));
        assert_eq!(done.tok_out, Some(5));
        assert!(!done.truncated);
        done.text
    });
    server.join().unwrap();

    assert_eq!(text, "alpha beta");
}

/// Guards the premise the other tests stand on: a `127.0.0.1` provider needs
/// no stored key. If `is_local` ever tightens, every test above fails for a
/// reason that has nothing to do with the step boundary, and the diff that
/// caused it would be hard to read.
#[test]
fn a_local_provider_needs_no_stored_key() {
    assert!(argus_lib::gateway::adapters::is_local(
        "http://127.0.0.1:9/v1"
    ));
    assert!(argus_lib::gateway::adapters::is_local("http://localhost:9"));
    assert!(!argus_lib::gateway::adapters::is_local(
        "https://api.example.com"
    ));
}

/// The recorder is the instrument, so it has to be trustworthy: a clear that
/// returned the wrong buffer would make every assertion above vacuous.
#[test]
fn the_recorder_replays_the_frontend_buffer_contract() {
    let rec = Recorder::new();
    rec.push(Ev::Delta("a".into()));
    rec.push(Ev::Delta("b".into()));
    rec.push(Ev::Step);
    rec.push(Ev::Delta("c".into()));
    rec.push(Ev::Reset);

    assert_eq!(rec.clears(), vec![2, 4]);
    assert_eq!(rec.buffer_at_clear(0), "ab");
    assert_eq!(rec.buffer_at_clear(1), "c");
    assert!(rec.has_reset());
}

// --- A provider that stops talking -------------------------------------
//
// The shared client used to be built with no timeout at all, so a provider
// that accepted a request and then went silent left the socket open forever.
// Nothing downstream bounded it: the request never errored, so the router's
// retry ladder never engaged and the turn simply stopped making progress with
// no failure to show for it.
//
// `gateway::http_client` bounds that with a per-read `read_timeout`. It is the
// right shape for a stream because it resets on progress — a slow-but-alive
// reply is never killed, only one that stops talking. A total timeout would be
// wrong here, since it runs until the body finishes and would cap every long
// reply.
//
// These drive the real `adapters::dispatch_stream` against mock sockets. That
// is the layer the bound lives on; going through `Turn::run` instead would
// measure the retry ladder (1+2+4+...+120s), not the stall.

/// Short bounds, so a stall is a one-second wait rather than a two-minute one.
const TEST_CONNECT: std::time::Duration = std::time::Duration::from_secs(5);
const TEST_STALL: std::time::Duration = std::time::Duration::from_secs(1);

fn stall_client() -> reqwest::Client {
    argus_lib::gateway::http_client_with(TEST_CONNECT, TEST_STALL).unwrap()
}

fn mock_prov(base_url: &str) -> Provider {
    Provider {
        id: "stall-prov".into(),
        name: "mock".into(),
        compatible: "openai".into(),
        base_url: base_url.into(),
        api_key_ref: None,
        connected: true,
        free: true,
        priority: 0,
        logo_url: None,
        doc_url: None,
    }
}

/// A mock that serves `done` whole replies, then accepts one more connection
/// and says nothing: no headers, no body, just an open socket. It never
/// returns an error — the client has to give up on its own.
fn stalling_provider(done: Vec<String>) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();

    let handle = std::thread::spawn(move || {
        for body in done {
            let Some(mut stream) = accept(&listener) else {
                return;
            };

            stream.set_nonblocking(false).ok();
            stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
            drain_request(&mut stream);

            if write_reply(&mut stream, 200, "OK", &body).is_err() {
                return;
            }
        }

        let Some(held) = accept(&listener) else {
            return;
        };

        let _ = held.set_read_timeout(None);
        std::thread::sleep(std::time::Duration::from_secs(20));
    });

    (base_url, handle)
}

/// A complete reply, shaped the way the openai adapter expects.
fn whole_reply(text: &str) -> String {
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        serde_json::json!({
            "choices": [{
                "delta": {"content": text},
                "finish_reason": "stop",
            }]
        })
    )
}

/// Sends `chunks` events `gap` apart, then closes. Surviving a total
/// duration longer than the stall bound is the whole claim: the bound is
/// per-read, so progress keeps resetting it.
fn slow_provider(
    text: String,
    chunks: usize,
    gap: std::time::Duration,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();

    let handle = std::thread::spawn(move || {
        let Some(mut stream) = accept(&listener) else {
            return;
        };

        stream.set_nonblocking(false).ok();
        stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
        drain_request(&mut stream);

        let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                    transfer-encoding: chunked\r\n\r\n";

        if stream.write_all(head.as_bytes()).is_err() {
            return;
        }

        let body = whole_reply(&text);

        for i in 0..chunks {
            if i > 0 {
                std::thread::sleep(gap);
            }

            let framed = format!("{:x}\r\n{}\r\n", body.len(), body);

            if stream.write_all(framed.as_bytes()).is_err() {
                return;
            }

            let _ = stream.flush();
        }

        let _ = stream.write_all(b"0\r\n\r\n");
        let _ = stream.flush();
        std::thread::sleep(std::time::Duration::from_millis(200));
    });

    (base_url, handle)
}

async fn stream_once(
    client: &reqwest::Client,
    base_url: &str,
    collect: &mut Vec<String>,
) -> Result<(), argus_lib::gateway::adapters::CallError> {
    let prov = mock_prov(base_url);
    let msgs = vec![argus_lib::gateway::schema::WireMsg {
        role: "user".into(),
        content: "hello".into(),
        ..Default::default()
    }];

    argus_lib::gateway::adapters::dispatch_stream(
        client,
        &prov,
        "mock-remote",
        None,
        &msgs,
        &[],
        &mut |d: &str| {
            collect.push(d.to_string());
            Ok(())
        },
    )
    .await
    .map(|_| ())
}

/// The regression. Unbounded, this never returns at all.
#[tokio::test]
async fn a_provider_that_stops_talking_errors_instead_of_hanging() {
    let (url, _handle) = stalling_provider(vec![]);
    let client = stall_client();
    let mut got = vec![];

    let t0 = std::time::Instant::now();
    let out = stream_once(&client, &url, &mut got).await;
    let ms = t0.elapsed().as_millis();

    let err = out.expect_err("a silent provider must not stream forever");
    assert!(ms < 15_000, "took {ms}ms; the stall bound did not bite");
    assert!(err.retryable(), "a stall should be worth another attempt");

    // A stall that produced nothing is a different failure from one that died
    // mid-reply, and the message is what a user reads on a stuck turn.
    assert!(
        err.msg.contains("accepted the request then went silent"),
        "{err:?}"
    );
    assert!(got.is_empty(), "nothing should have streamed: {got:?}");
}

/// The other half. The bound must not fire on a stream that is merely slow —
/// every gap here is wider than the stall, so a client that timed out on total
/// elapsed time would break a working provider.
#[tokio::test]
async fn a_slow_but_alive_provider_is_not_cut_off() {
    // Eight chunks, 400ms apart: every gap is under the one-second bound, but
    // the stream runs for over three seconds, well past it. A total timeout
    // would cut this off at one second; a per-read bound never does.
    let (url, _handle) = slow_provider(
        "still here.".into(),
        8,
        std::time::Duration::from_millis(400),
    );
    let client = stall_client();
    let mut got = vec![];

    stream_once(&client, &url, &mut got)
        .await
        .expect("a slow stream must survive a per-read stall bound");

    assert!(
        got.iter().any(|d| d.contains("still here.")),
        "the slow reply must arrive intact: {got:?}"
    );
}

/// A stream that stalls after real content has arrived is the case a total
/// timeout would also catch, but for the wrong reason: the partial reply is
/// already on screen and the error has to say the reply was cut off.
#[tokio::test]
async fn a_mid_reply_stall_says_the_reply_was_cut_off() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();

    let handle = std::thread::spawn(move || {
        let Some(mut stream) = accept(&listener) else {
            return;
        };

        stream.set_nonblocking(false).ok();
        stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
        drain_request(&mut stream);

        let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                    transfer-encoding: chunked\r\n\r\n";
        let _ = stream.write_all(head.as_bytes());

        let body = whole_reply("half a th");
        let framed = format!("{:x}\r\n{}\r\n", body.len(), body);
        let _ = stream.write_all(framed.as_bytes());
        let _ = stream.flush();

        // Now go quiet with the stream open.
        std::thread::sleep(std::time::Duration::from_secs(20));
    });

    let client = stall_client();
    let mut got = vec![];

    let out = stream_once(&client, &url, &mut got).await;

    assert!(
        got.iter().any(|d| d.contains("half a th")),
        "the partial reply must have been delivered: {got:?}"
    );

    let err = out.expect_err("the stall must end the stream");
    assert!(err.msg.contains("stopped sending mid-reply"), "{err:?}");
    drop(handle);
}
