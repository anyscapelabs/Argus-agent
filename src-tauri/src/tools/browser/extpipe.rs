use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

const REQ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FRAME: u32 = 64 * 1_048_576;

struct Registry {
    tx: StdMutex<Option<mpsc::Sender<String>>>,
    pending: StdMutex<std::collections::HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
}

static REG: OnceLock<Registry> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn reg() -> &'static Registry {
    REG.get_or_init(|| Registry {
        tx: StdMutex::new(None),
        pending: StdMutex::new(std::collections::HashMap::new()),
    })
}

pub fn connected() -> bool {
    let g = match reg().tx.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };

    g.as_ref().map(|t| !t.is_closed()).unwrap_or(false)
}

fn socket_path(dir: &PathBuf) -> PathBuf {
    dir.join("native.sock")
}

pub fn start_listener(data_dir: PathBuf) {
    let path = socket_path(&data_dir);
    let _ = std::fs::remove_file(&path);

    if let Some(dir) = path.parent() {
        let _ = std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    }

    tauri::async_runtime::spawn(async move {
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(_) => return,
        };

        while let Ok((stream, _)) = listener.accept().await {
            tauri::async_runtime::spawn(async move {
                serve_conn(stream).await;
            });
        }
    });
}

async fn serve_conn(stream: UnixStream) {
    let (rd, mut wr) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<String>(64);

    {
        let r = reg();

        if let Ok(mut g) = r.tx.lock() {
            *g = Some(tx.clone());
        }

        r.pending.lock().map(|mut p| p.clear()).ok();
    }

    let writer = tauri::async_runtime::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write_frame(&mut wr, &msg).await.is_err() {
                break;
            }
        }
    });

    read_loop(rd).await;

    if let Ok(mut g) = reg().tx.lock() {
        *g = None;
    }

    writer.abort();
    fail_pending("Argus extension disconnected");
}

async fn read_loop(mut rd: tokio::net::unix::OwnedReadHalf) {
    loop {
        let len = match read_u32(&mut rd).await {
            Some(n) => n,
            None => break,
        };

        if len == 0 || len > MAX_FRAME {
            break;
        }

        let mut buf = vec![0u8; len as usize];
        if rd.read_exact(&mut buf).await.is_err() {
            break;
        }

        let v: Value = match serde_json::from_slice(&buf) {
            Ok(v) => v,
            Err(_) => continue,
        };

        handle_msg(v);
    }
}

async fn read_u32(rd: &mut tokio::net::unix::OwnedReadHalf) -> Option<u32> {
    let mut b = [0u8; 4];
    rd.read_exact(&mut b).await.ok()?;
    Some(u32::from_ne_bytes(b))
}

async fn write_frame(wr: &mut tokio::net::unix::OwnedWriteHalf, msg: &str) -> std::io::Result<()> {
    let b = msg.as_bytes();
    wr.write_all(&(b.len() as u32).to_ne_bytes()).await?;
    wr.write_all(b).await?;
    wr.flush().await
}

fn handle_msg(v: Value) {
    let id = v.get("id").and_then(|i| i.as_u64());

    if v.get("kind").and_then(|k| k.as_str()) == Some("evt") {
        return;
    }

    let ok = v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);

    let result = match ok {
        true => Ok(v.get("data").cloned().unwrap_or(Value::Null)),
        false => Err(v
            .get("err")
            .and_then(|err| err.as_str())
            .unwrap_or("extension error")
            .to_string()),
    };

    let Some(id) = id else { return };

    if let Ok(mut p) = reg().pending.lock() {
        if let Some(tx) = p.remove(&id) {
            let _ = tx.send(result);
        }
    }
}

fn fail_pending(msg: &str) {
    if let Ok(mut p) = reg().pending.lock() {
        for (_, tx) in p.drain() {
            let _ = tx.send(Err(msg.into()));
        }
    }
}

pub async fn request(method: &str, data: Value) -> Result<Value, String> {
    let tx = match reg().tx.lock() {
        Ok(g) => g.as_ref().cloned(),
        Err(_) => None,
    };

    let tx = tx.ok_or("Argus extension not connected")?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

    let (rtx, rrx) = oneshot::channel();
    reg()
        .pending
        .lock()
        .map_err(|_| "extpipe lock poisoned")?
        .insert(id, rtx);

    let msg = serde_json::json!({
        "kind": "req",
        "id": id,
        "method": method,
        "data": data,
    });

    if tx.send(msg.to_string()).await.is_err() {
        if let Ok(mut p) = reg().pending.lock() {
            p.remove(&id);
        }
        return Err("Argus extension not connected".into());
    }

    match tokio::time::timeout(REQ_TIMEOUT, rrx).await {
        Ok(Ok(res)) => res,
        Ok(Err(_)) => Err("extension dropped the connection".into()),
        Err(_) => Err("extension timed out".into()),
    }
}

pub async fn run_stdio_host(socket: PathBuf) {
    use tokio::io::{stdin, stdout};

    let mut rd = stdin();

    loop {
        let mut wr = stdout();

        let stream = loop {
            match UnixStream::connect(&socket).await {
                Ok(s) => break s,
                Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            }
        };

        let (mut srd, mut swr) = stream.into_split();

        let up = tokio::spawn(async move {
            loop {
                let Some(len) = read_u32(&mut srd).await else {
                    break;
                };
                if len == 0 || len > MAX_FRAME {
                    break;
                }

                let mut buf = vec![0u8; len as usize];
                if srd.read_exact(&mut buf).await.is_err() {
                    break;
                }

                let _ = wr.write_all(&(buf.len() as u32).to_ne_bytes()).await;
                let _ = wr.write_all(&buf).await;
                let _ = wr.flush().await;
            }
        });

        let mut buf = vec![0u8; 65_536];
        let mut acc: Vec<u8> = vec![];

        loop {
            let n = match rd.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };

            acc.extend_from_slice(&buf[..n]);

            while acc.len() >= 4 {
                let len = u32::from_ne_bytes([acc[0], acc[1], acc[2], acc[3]]) as usize;

                if acc.len() < 4 + len {
                    break;
                }

                let frame: Vec<u8> = acc.drain(..4 + len).collect();

                if swr.write_all(&(len as u32).to_ne_bytes()).await.is_err()
                    || swr.write_all(&frame[4..]).await.is_err()
                {
                    break;
                }
            }

            if swr.flush().await.is_err() {
                break;
            }
        }

        up.abort();
    }
}
