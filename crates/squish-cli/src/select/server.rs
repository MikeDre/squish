//! Loopback HTTP server backing `--select`.
//!
//! Two phases. **Select**: bind, hand the URL to a browser, serve the page, the
//! preview and live estimates until a rect arrives. **Report**: once a rect is
//! chosen the listener moves to a background thread serving only `/status` and
//! `/bye`, so the still-open page can be told how the run went.
//!
//! Nothing is ever exposed off-host in either phase: the listener binds to
//! 127.0.0.1 and every request must carry a 128-bit token minted per session.

use anyhow::{Context, Result};
use squish_core::{CropRect, CropSpec, Gravity};
use std::io::Read;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};

/// Everything the page needs to render, and the source it maps back onto.
pub(crate) struct Session {
    pub preview: squish_core::Preview,
    pub seed: CropRect,
    pub file_name: String,
    pub source_bytes: u64,
    /// Aspect ratio the page opens locked to, from an aspect `--crop`.
    pub lock: Option<(u32, u32)>,
    /// The settings a live estimate is produced with, e.g. "q75 webp".
    pub settings: String,
    /// The real input, re-encoded per selection to get an exact size.
    pub source_path: std::path::PathBuf,
    pub opts: squish_core::SquishOptions,
}

/// How a session ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    Cropped(CropRect),
    Cancelled,
    TimedOut,
}

/// What the page should be showing. The selector owns this until a rect
/// arrives; the CLI owns it from then on.
#[derive(Debug)]
pub(crate) enum Phase {
    Working,
    Done(Report),
    Failed(String),
}

/// The outcome of the run, in the shape the page renders.
///
/// `file` is a file *name*, not a path: a full path is noise in a browser and a
/// small information leak into screenshots. `output_bytes` is `None` when
/// nothing was written (`--dry-run`), so the card can omit the size line rather
/// than print a no-op transition.
#[derive(Debug, Clone)]
pub(crate) struct Report {
    pub file: String,
    pub input_bytes: u64,
    pub output_bytes: Option<u64>,
    /// "3000x2000+10+20", or "" for a whole-image selection.
    pub crop: String,
    pub note: Option<String>,
}

fn phase_json(p: &Phase) -> String {
    match p {
        Phase::Working => serde_json::json!({ "phase": "working" }).to_string(),
        Phase::Failed(e) => serde_json::json!({ "phase": "failed", "error": e }).to_string(),
        Phase::Done(r) => serde_json::json!({
            "phase": "done",
            "file": r.file,
            "in": r.input_bytes,
            "out": r.output_bytes,
            "pct": r.output_bytes.map(|out| pct(r.input_bytes, out)),
            "crop": r.crop,
            "note": r.note,
        })
        .to_string(),
    }
}

/// Size reduction, positive for a shrink — the same formula as
/// `SquishResult::reduction_percent`, rounded to one decimal so the card and the
/// terminal summary can never disagree in the digits they show.
fn pct(input: u64, output: u64) -> f64 {
    if input == 0 {
        return 0.0;
    }
    let delta = input as f64 - output as f64;
    ((delta / input as f64) * 1000.0).round() / 10.0
}

/// A bound-but-not-yet-serving server.
pub(crate) struct Bound {
    server: Server,
    pub addr: String,
    pub token: String,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// Bind a loopback listener on a kernel-assigned port and mint a session token.
pub(crate) fn bind() -> Result<Bound> {
    let server = Server::http("127.0.0.1:0").map_err(|e| {
        anyhow::anyhow!(
            "could not start the crop selector server: {e}\n\
             help: pass an explicit --crop WxH+X+Y to crop without the selector"
        )
    })?;
    let addr = server
        .server_addr()
        .to_ip()
        .context("selector server has no IP address")?
        .to_string();
    Ok(Bound {
        server,
        addr,
        token: mint_token()?,
    })
}

/// 128-bit hex token. `/dev/urandom` is cryptographically sound on both
/// supported platforms and costs no dependency.
fn mint_token() -> Result<String> {
    let mut buf = [0u8; 16];
    let mut f = std::fs::File::open("/dev/urandom").context("open /dev/urandom")?;
    f.read_exact(&mut buf).context("read /dev/urandom")?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Serve one selection to completion. Borrows the listener rather than
/// consuming it: on `/crop` the caller hands the same listener to a `Reporter`
/// so the page can be told how the run went.
fn serve_selection(
    server: &Server,
    session: &Session,
    token: &str,
    idle: Duration,
) -> Result<Outcome> {
    loop {
        let Some(mut req) = server.recv_timeout(idle)? else {
            return Ok(Outcome::TimedOut);
        };

        let url = req.url().to_string();
        let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));

        if !token_ok(query, token) {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            continue;
        }

        match (req.method(), path) {
            (Method::Get, "/") => {
                let page = page_html(session, token);
                let _ = req.respond(Response::from_string(page).with_header(html_header()));
            }
            (Method::Get, "/preview") => {
                let resp = Response::from_data(session.preview.bytes.clone())
                    .with_header(mime_header(session.preview.mime));
                let _ = req.respond(resp);
            }
            (Method::Post, "/cancel") => {
                let _ = req.respond(Response::from_string("ok"));
                return Ok(Outcome::Cancelled);
            }
            (Method::Post, "/crop") => {
                let mut body = String::new();
                if req.as_reader().read_to_string(&mut body).is_err() {
                    let _ =
                        req.respond(Response::from_string("unreadable body").with_status_code(400));
                    continue;
                }
                match validate(&body, session) {
                    Ok(rect) => {
                        let _ = req.respond(Response::from_string("ok"));
                        return Ok(Outcome::Cropped(rect));
                    }
                    Err(msg) => {
                        let _ = req.respond(Response::from_string(msg).with_status_code(400));
                    }
                }
            }
            (Method::Post, "/estimate") => {
                let mut body = String::new();
                if req.as_reader().read_to_string(&mut body).is_err() {
                    let _ = req.respond(Response::from_string("{}").with_status_code(400));
                    continue;
                }
                let rect = match validate(&body, session) {
                    Ok(r) => r,
                    Err(msg) => {
                        let _ = req.respond(Response::from_string(msg).with_status_code(400));
                        continue;
                    }
                };
                // Encoding can take a second or more; do it off the accept loop
                // so Cancel is never queued behind an estimate.
                let source = session.source_path.clone();
                let opts = session.opts.clone();
                let tag = format!("{}-{}", token, estimate_seq());
                std::thread::spawn(move || {
                    let payload = match super::estimate::estimate(&source, &opts, rect, &tag) {
                        Ok(super::estimate::EstimateOutcome::Bytes(n)) => {
                            serde_json::json!({ "bytes": n })
                        }
                        Ok(super::estimate::EstimateOutcome::Skipped(why)) => {
                            serde_json::json!({ "skipped": why })
                        }
                        Err(e) => serde_json::json!({ "skipped": e.to_string() }),
                    };
                    let _ = req.respond(
                        Response::from_string(payload.to_string()).with_header(json_header()),
                    );
                });
            }
            _ => {
                let _ = req.respond(Response::from_string("not found").with_status_code(404));
            }
        }
    }
}

/// How long the phase-B loop blocks before re-checking its stop flag.
const REPORT_TICK: Duration = Duration::from_millis(100);

/// The live half of a finished selection: the page is still open and polling, so
/// the CLI can tell it how the run went.
///
/// Phase B deliberately serves only `/status` and `/bye`. `/preview` and
/// `/estimate` are gone, so no decode or re-encode is reachable once a rect has
/// been chosen. The listener is still loopback-only and still token-gated, and it
/// dies with the process — at most ~1.5s after the run finishes.
pub(crate) struct Reporter {
    phase: Arc<Mutex<Phase>>,
    /// Set once the page has read a terminal phase, or announced it is leaving.
    seen: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Reporter {
    /// Take over `server` and start answering `/status` on a background thread.
    fn spawn(server: Server, token: String) -> Self {
        let phase = Arc::new(Mutex::new(Phase::Working));
        let seen = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s, st) = (phase.clone(), seen.clone(), stop.clone());
        let handle = std::thread::spawn(move || report_loop(server, token, p, s, st));
        Self {
            phase,
            seen,
            stop,
            handle: Some(handle),
        }
    }

    /// Publish the run's outcome. The next `/status` poll picks it up.
    pub(crate) fn finish(&self, phase: Phase) {
        *self.phase.lock().expect("phase mutex") = phase;
    }

    /// Block until the page has read a terminal phase, or `max` elapses.
    ///
    /// The point is not to make the CLI wait — it is to stop the CLI exiting out
    /// from under a page that is 200ms away from showing the result.
    pub(crate) fn wait_for_pickup(&self, max: Duration) {
        let deadline = std::time::Instant::now() + max;
        while !self.seen.load(Ordering::Acquire) {
            if std::time::Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn report_loop(
    server: Server,
    token: String,
    phase: Arc<Mutex<Phase>>,
    seen: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        let req = match server.recv_timeout(REPORT_TICK) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            // A dead listener cannot recover; break rather than spin.
            Err(_) => break,
        };

        let url = req.url().to_string();
        let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
        if !token_ok(query, &token) {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            continue;
        }

        match (req.method(), path) {
            (Method::Get, "/status") => {
                // One lock for both the body and the terminal check, so a
                // finish() landing mid-request cannot flip the flag for a
                // "working" body the page never saw as terminal.
                let (body, terminal) = {
                    let p = phase.lock().expect("phase mutex");
                    (phase_json(&p), !matches!(*p, Phase::Working))
                };
                if terminal {
                    seen.store(true, Ordering::Release);
                }
                let _ = req.respond(Response::from_string(body).with_header(json_header()));
            }
            (Method::Post, "/bye") => {
                seen.store(true, Ordering::Release);
                let _ = req.respond(Response::from_string("ok"));
            }
            _ => {
                let _ = req.respond(Response::from_string("not found").with_status_code(404));
            }
        }
    }
}

/// Bind, announce, and serve with the production idle timeout.
pub(crate) fn run(session: &Session) -> Result<(Outcome, Option<Reporter>)> {
    run_with_timeout(session, IDLE_TIMEOUT)
}

/// Serve one selection, then — if a rect was chosen — keep the listener alive so
/// the CLI can report the run's outcome back to the still-open page.
pub(crate) fn run_with_timeout(
    session: &Session,
    idle: Duration,
) -> Result<(Outcome, Option<Reporter>)> {
    let bound = bind()?;
    let url = format!("http://{}/?t={}", bound.addr, bound.token);

    // Test/automation seam: make the URL discoverable without a browser.
    if let Some(p) = std::env::var_os("SQUISH_SELECT_URL_FILE") {
        std::fs::write(p, &url).context("write SQUISH_SELECT_URL_FILE")?;
    }

    let launched = if std::env::var_os("SQUISH_SELECT_NO_OPEN").is_some() {
        false
    } else {
        open_browser(&url)
    };
    if launched {
        eprintln!("Opening crop selector in your browser… (Ctrl-C to cancel)");
    } else {
        eprintln!("Open this URL to pick the crop region (Ctrl-C to cancel):\n  {url}");
    }

    let Bound { server, token, .. } = bound;
    let outcome = serve_selection(&server, session, &token, idle)?;
    // Only a chosen rect leads to work worth reporting; cancel and timeout drop
    // the listener here, exactly as the one-shot server always did.
    let reporter = match outcome {
        Outcome::Cropped(_) => Some(Reporter::spawn(server, token)),
        Outcome::Cancelled | Outcome::TimedOut => None,
    };
    Ok((outcome, reporter))
}

/// Re-validate a rect from the browser through the existing crop engine. The
/// page is untrusted input: a JS bug must not be able to produce a crop the
/// non-interactive path would have rejected.
fn validate(body: &str, session: &Session) -> Result<CropRect, String> {
    #[derive(serde::Deserialize)]
    struct RectBody {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    }
    let r: RectBody = serde_json::from_str(body).map_err(|e| format!("bad rect: {e}"))?;
    if r.w == 0 || r.h == 0 {
        return Err("selection has zero width or height".into());
    }
    let (sw, sh) = (session.preview.source_w, session.preview.source_h);
    let spec = CropSpec::Exact {
        w: r.w,
        h: r.h,
        x: r.x,
        y: r.y,
    };
    match spec.resolve(Gravity::Center, sw, sh) {
        Ok(Some(rect)) => Ok(rect),
        // A full-image selection is a no-op crop; hand back the whole image and
        // let the caller report it.
        Ok(None) => Ok(CropRect {
            x: 0,
            y: 0,
            w: sw,
            h: sh,
        }),
        Err(reason) => Err(reason),
    }
}

fn token_ok(query: &str, token: &str) -> bool {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .any(|(k, v)| k == "t" && v == token)
}

fn html_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
        .expect("static header")
}

fn json_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("static header")
}

/// Monotonic counter so each estimate gets its own scratch directory.
fn estimate_seq() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn mime_header(mime: &str) -> Header {
    Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).expect("static header")
}

/// The selector page: one HTML file with the CSS and JS inlined, plus the
/// session config as JSON. No external requests — a strict-CSP-friendly,
/// offline-safe page, and nothing to build.
///
/// The token is deliberately *not* interpolated: the browser already carries it
/// in `location.search`, so baking it into the markup would only widen its
/// exposure (a saved page, a screenshot, a devtools copy-as-HTML).
fn page_html(session: &Session, _token: &str) -> String {
    let cfg = serde_json::json!({
        "source_w": session.preview.source_w,
        "source_h": session.preview.source_h,
        "preview_w": session.preview.w,
        "preview_h": session.preview.h,
        "seed": {
            "x": session.seed.x,
            "y": session.seed.y,
            "w": session.seed.w,
            "h": session.seed.h,
        },
        "file_name": session.file_name,
        "source_bytes": session.source_bytes,
        "lock": session.lock.map(|(w, h)| [w, h]),
        "settings": session.settings,
    });

    // The config lands inside a <script> block, and a file name is untrusted
    // text: `<` must not be able to close that block. A `<` can only occur
    // inside a JSON string here, and the `<` escape is the same string to
    // any JSON or JS parser, so escaping it costs nothing.
    let cfg = cfg
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");

    // Config last: a `__SQUISH_CONFIG__` literal arriving from the CSS or JS
    // file can never be substituted.
    include_str!("assets/selector.html")
        .replace("__SQUISH_CSS__", include_str!("assets/selector.css"))
        .replace("__SQUISH_JS__", include_str!("assets/selector.js"))
        .replace("__SQUISH_CONFIG__", &cfg)
}

fn open_browser(url: &str) -> bool {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    fn session() -> Session {
        Session {
            preview: squish_core::Preview {
                bytes: vec![0xff, 0xd8, 0xff, 0xd9],
                mime: "image/jpeg",
                w: 640,
                h: 480,
                source_w: 640,
                source_h: 480,
            },
            seed: CropRect {
                x: 0,
                y: 0,
                w: 320,
                h: 240,
            },
            file_name: "hero.jpg".into(),
            source_bytes: 4096,
            lock: None,
            settings: "q75 jpg".into(),
            source_path: std::path::PathBuf::from("hero.jpg"),
            opts: Default::default(),
        }
    }

    /// Send a raw request and return the full response text.
    fn request(addr: &str, method: &str, target: &str, body: Option<&str>) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        let body = body.unwrap_or("");
        let req = format!(
            "{method} {target} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).unwrap();
        // Read raw bytes rather than `read_to_string`: the /preview response
        // carries a binary body, which is not valid UTF-8 on its own, and
        // `read_to_string` errors on the first invalid byte anywhere in the
        // stream. A lossy conversion keeps every assertion below intact —
        // they all match ASCII text (status line, headers) that appears
        // before any binary payload.
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Start a phase-A server on a background thread, returning its address,
    /// token and a handle that yields the Outcome.
    fn start(
        session: Session,
        idle: Duration,
    ) -> (String, String, std::thread::JoinHandle<Outcome>) {
        let bound = bind().unwrap();
        let addr = bound.addr.clone();
        let token = bound.token.clone();
        let Bound { server, .. } = bound;
        let tok = token.clone();
        let handle =
            std::thread::spawn(move || serve_selection(&server, &session, &tok, idle).unwrap());
        (addr, token, handle)
    }

    /// Start a phase-B server directly: bind, hand the listener to a Reporter,
    /// and return its address, token and the Reporter.
    fn start_reporting() -> (String, String, Reporter) {
        let bound = bind().unwrap();
        let addr = bound.addr.clone();
        let token = bound.token.clone();
        let Bound {
            server, token: tok, ..
        } = bound;
        (addr, token, Reporter::spawn(server, tok))
    }

    fn done_report() -> Report {
        Report {
            file: "hero_squished.png".into(),
            input_bytes: 4096,
            output_bytes: Some(1024),
            crop: "40x30+5+6".into(),
            note: None,
        }
    }

    #[test]
    fn rejects_requests_without_the_token() {
        let (addr, token, handle) = start(session(), Duration::from_secs(5));
        let resp = request(&addr, "GET", "/", None);
        assert!(resp.starts_with("HTTP/1.1 403"), "got: {resp}");

        // Shut it down so the thread joins.
        request(&addr, "POST", &format!("/cancel?t={token}"), None);
        assert!(matches!(handle.join().unwrap(), Outcome::Cancelled));
    }

    #[test]
    fn crop_post_returns_the_rect() {
        let (addr, token, handle) = start(session(), Duration::from_secs(5));
        let resp = request(
            &addr,
            "POST",
            &format!("/crop?t={token}"),
            Some(r#"{"x":10,"y":20,"w":300,"h":200}"#),
        );
        assert!(resp.starts_with("HTTP/1.1 200"), "got: {resp}");

        match handle.join().unwrap() {
            Outcome::Cropped(r) => assert_eq!((r.x, r.y, r.w, r.h), (10, 20, 300, 200)),
            other => panic!("expected Cropped, got {other:?}"),
        }
    }

    #[test]
    fn out_of_bounds_rect_is_rejected_and_the_session_continues() {
        let (addr, token, handle) = start(session(), Duration::from_secs(5));
        let resp = request(
            &addr,
            "POST",
            &format!("/crop?t={token}"),
            Some(r#"{"x":9999,"y":0,"w":10,"h":10}"#),
        );
        assert!(resp.starts_with("HTTP/1.1 400"), "got: {resp}");

        // Still alive: a following cancel is what ends it.
        request(&addr, "POST", &format!("/cancel?t={token}"), None);
        assert!(matches!(handle.join().unwrap(), Outcome::Cancelled));
    }

    #[test]
    fn zero_sized_rect_is_rejected() {
        let (addr, token, handle) = start(session(), Duration::from_secs(5));
        let resp = request(
            &addr,
            "POST",
            &format!("/crop?t={token}"),
            Some(r#"{"x":0,"y":0,"w":0,"h":100}"#),
        );
        assert!(resp.starts_with("HTTP/1.1 400"), "got: {resp}");
        request(&addr, "POST", &format!("/cancel?t={token}"), None);
        handle.join().unwrap();
    }

    #[test]
    fn full_image_selection_is_returned_as_the_whole_image() {
        let (addr, token, handle) = start(session(), Duration::from_secs(5));
        request(
            &addr,
            "POST",
            &format!("/crop?t={token}"),
            Some(r#"{"x":0,"y":0,"w":640,"h":480}"#),
        );
        match handle.join().unwrap() {
            Outcome::Cropped(r) => assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 640, 480)),
            other => panic!("expected Cropped, got {other:?}"),
        }
    }

    #[test]
    fn preview_endpoint_serves_the_preview_bytes() {
        let (addr, token, handle) = start(session(), Duration::from_secs(5));
        let resp = request(&addr, "GET", &format!("/preview?t={token}"), None);
        assert!(resp.starts_with("HTTP/1.1 200"), "got: {resp}");
        assert!(resp.contains("image/jpeg"));

        request(&addr, "POST", &format!("/cancel?t={token}"), None);
        handle.join().unwrap();
    }

    #[test]
    fn page_is_served_with_the_token() {
        let (addr, token, handle) = start(session(), Duration::from_secs(5));
        let resp = request(&addr, "GET", &format!("/?t={token}"), None);
        assert!(resp.starts_with("HTTP/1.1 200"), "got: {resp}");
        assert!(resp.contains("hero.jpg"));

        request(&addr, "POST", &format!("/cancel?t={token}"), None);
        handle.join().unwrap();
    }

    #[test]
    fn page_embeds_the_session_config() {
        let s = session();
        let html = page_html(&s, "tok");
        assert!(html.contains("\"source_w\":640"));
        assert!(html.contains("\"source_h\":480"));
        assert!(html.contains("\"preview_w\":640"));
        assert!(html.contains("\"w\":320"), "seed rect present");
        assert!(html.contains("hero.jpg"));
    }

    #[test]
    fn page_is_self_contained() {
        let html = page_html(&session(), "tok");
        assert!(!html.contains("https://"), "no external requests");
        assert!(!html.contains("script src"), "no external scripts");
        assert!(!html.contains("stylesheet"), "no external stylesheets");
        assert!(html.contains("<style>") && html.contains("<script>"));
    }

    #[test]
    fn page_carries_the_overlay_scaffolding() {
        let html = page_html(&session(), "tok");
        for id in [
            "id=\"overlay\"",
            "id=\"card\"",
            "id=\"card-icon\"",
            "id=\"card-title\"",
            "id=\"card-detail\"",
            "id=\"card-foot\"",
            "id=\"progress\"",
        ] {
            assert!(html.contains(id), "missing {id}");
        }
    }

    #[test]
    fn page_keeps_the_header_bar_id_distinct_from_the_progress_bar() {
        // #bar is the header; the indeterminate bar must not collide with it.
        let html = page_html(&session(), "tok");
        assert!(html.contains("id=\"bar\""));
        assert!(html.contains("id=\"progress\""));
    }

    #[test]
    fn page_does_not_leak_the_token_into_markup() {
        // The browser already has the token in its URL; the page uses
        // location.search, so the token must not be baked into the HTML.
        let html = page_html(&session(), "sekrit");
        assert!(!html.contains("sekrit"));
    }

    #[test]
    fn page_config_carries_the_ratio_lock() {
        let mut s = session();
        s.lock = Some((16, 9));
        assert!(page_html(&s, "tok").contains("\"lock\":[16,9]"));

        let s2 = session();
        assert!(page_html(&s2, "tok").contains("\"lock\":null"));
    }

    #[test]
    fn page_config_cannot_close_the_script_block() {
        // A file name is untrusted text; it must not be able to break out of
        // the <script> block the config is injected into.
        let mut s = session();
        s.file_name = "a</script><img src=x onerror=alert(1)>.png".into();
        let html = page_html(&s, "tok");
        assert!(!html.contains("</script><img"));
        assert!(html.contains("\\u003c/script\\u003e"), "escaped instead");
    }

    #[test]
    fn status_reports_working_until_the_run_finishes() {
        let (addr, token, reporter) = start_reporting();

        let resp = request(&addr, "GET", &format!("/status?t={token}"), None);
        assert!(resp.starts_with("HTTP/1.1 200"), "got: {resp}");
        assert!(resp.contains(r#""phase":"working""#), "got: {resp}");

        reporter.finish(Phase::Done(done_report()));

        let resp = request(&addr, "GET", &format!("/status?t={token}"), None);
        assert!(resp.contains(r#""phase":"done""#), "got: {resp}");
        assert!(resp.contains(r#""file":"hero_squished.png""#), "got: {resp}");
        assert!(resp.contains(r#""out":1024"#), "got: {resp}");
    }

    #[test]
    fn status_requires_the_token() {
        let (addr, _token, _reporter) = start_reporting();
        let resp = request(&addr, "GET", "/status", None);
        assert!(resp.starts_with("HTTP/1.1 403"), "got: {resp}");
    }

    #[test]
    fn phase_b_does_not_serve_the_selection_routes() {
        // Once a rect is chosen, no re-encoding must be reachable.
        let (addr, token, _reporter) = start_reporting();
        for target in ["/", "/preview", "/estimate"] {
            let resp = request(&addr, "GET", &format!("{target}?t={token}"), None);
            assert!(
                resp.starts_with("HTTP/1.1 404"),
                "{target} should be gone in phase B, got: {resp}"
            );
        }
    }

    #[test]
    fn reading_a_terminal_status_releases_the_cli() {
        let (addr, token, reporter) = start_reporting();
        reporter.finish(Phase::Done(done_report()));

        // Not picked up yet: the wait burns its whole (short) timeout.
        let t0 = std::time::Instant::now();
        reporter.wait_for_pickup(Duration::from_millis(200));
        assert!(
            t0.elapsed() >= Duration::from_millis(150),
            "should have waited"
        );

        request(&addr, "GET", &format!("/status?t={token}"), None);

        let t1 = std::time::Instant::now();
        reporter.wait_for_pickup(Duration::from_secs(5));
        assert!(
            t1.elapsed() < Duration::from_millis(500),
            "a picked-up status must release the wait immediately, took {:?}",
            t1.elapsed()
        );
    }

    #[test]
    fn reading_a_working_status_does_not_release_the_cli() {
        let (addr, token, reporter) = start_reporting();
        request(&addr, "GET", &format!("/status?t={token}"), None);

        let t0 = std::time::Instant::now();
        reporter.wait_for_pickup(Duration::from_millis(200));
        assert!(
            t0.elapsed() >= Duration::from_millis(150),
            "a 'working' read is not a pickup"
        );
    }

    #[test]
    fn bye_releases_the_cli() {
        let (addr, token, reporter) = start_reporting();
        let resp = request(&addr, "POST", &format!("/bye?t={token}"), None);
        assert!(resp.starts_with("HTTP/1.1 200"), "got: {resp}");

        let t0 = std::time::Instant::now();
        reporter.wait_for_pickup(Duration::from_secs(5));
        assert!(
            t0.elapsed() < Duration::from_millis(500),
            "a closed tab must let the CLI exit at once, took {:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn a_cancelled_session_never_enters_phase_b() {
        let session = session();
        let bound = bind().unwrap();
        let addr = bound.addr.clone();
        let token = bound.token.clone();
        let handle = std::thread::spawn(move || {
            let Bound {
                server, token: tok, ..
            } = bound;
            let outcome = serve_selection(&server, &session, &tok, Duration::from_secs(5)).unwrap();
            match outcome {
                Outcome::Cropped(_) => Some(Reporter::spawn(server, tok)),
                _ => None,
            }
        });
        request(&addr, "POST", &format!("/cancel?t={token}"), None);
        assert!(
            handle.join().unwrap().is_none(),
            "cancel must not leave a server running"
        );
    }

    #[test]
    fn working_phase_serializes_to_a_bare_phase_field() {
        assert_eq!(phase_json(&Phase::Working), r#"{"phase":"working"}"#);
    }

    #[test]
    fn done_phase_carries_the_numbers_and_a_derived_percentage() {
        let json = phase_json(&Phase::Done(Report {
            file: "big-sq.jpg".into(),
            input_bytes: 1_346_291,
            output_bytes: Some(78_210),
            crop: "3000x2000+10+20".into(),
            note: None,
        }));
        assert!(json.contains(r#""phase":"done""#), "got: {json}");
        assert!(json.contains(r#""file":"big-sq.jpg""#), "got: {json}");
        assert!(json.contains(r#""in":1346291"#), "got: {json}");
        assert!(json.contains(r#""out":78210"#), "got: {json}");
        assert!(json.contains(r#""pct":94.2"#), "got: {json}");
        assert!(json.contains(r#""crop":"3000x2000+10+20""#), "got: {json}");
        assert!(json.contains(r#""note":null"#), "got: {json}");
    }

    #[test]
    fn a_dry_run_report_has_no_output_size_and_no_percentage() {
        // Nothing was written, so a "1.3 MB → 1.3 MB (0%)" line would be a lie.
        let json = phase_json(&Phase::Done(Report {
            file: "hero.png".into(),
            input_bytes: 4096,
            output_bytes: None,
            crop: "40x30+5+6".into(),
            note: Some("nothing written (--dry-run)".into()),
        }));
        assert!(json.contains(r#""out":null"#), "got: {json}");
        assert!(json.contains(r#""pct":null"#), "got: {json}");
        assert!(
            json.contains(r#""note":"nothing written (--dry-run)""#),
            "got: {json}"
        );
    }

    #[test]
    fn failed_phase_carries_the_error() {
        let json = phase_json(&Phase::Failed("permission denied".into()));
        assert!(json.contains(r#""phase":"failed""#), "got: {json}");
        assert!(json.contains(r#""error":"permission denied""#), "got: {json}");
    }

    #[test]
    fn percentage_matches_reduction_percent_to_one_decimal() {
        // Same formula as SquishResult::reduction_percent, rounded for display.
        assert_eq!(pct(1_346_291, 78_210), 94.2);
        assert_eq!(pct(1000, 1000), 0.0);
        assert_eq!(pct(0, 0), 0.0, "an empty input must not divide by zero");
        assert_eq!(pct(100, 150), -50.0, "growth reports negative");
    }

    #[test]
    fn idle_timeout_ends_the_session() {
        let (_addr, _token, handle) = start(session(), Duration::from_millis(50));
        assert!(matches!(handle.join().unwrap(), Outcome::TimedOut));
    }
}
