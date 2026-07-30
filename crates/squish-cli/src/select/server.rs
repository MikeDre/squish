//! One-shot loopback HTTP server backing `--select`.
//!
//! Lifetime: bind, hand the URL to a browser, serve exactly one selection,
//! shut down. Nothing is ever exposed off-host: the listener binds to
//! 127.0.0.1 and every request must carry a 128-bit token minted per session.

use anyhow::{Context, Result};
use squish_core::{CropRect, CropSpec, Gravity};
use std::io::Read;
use std::process::Stdio;
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};

/// Everything the page needs to render, and the source it maps back onto.
pub(crate) struct Session {
    pub preview: squish_core::Preview,
    pub seed: CropRect,
    pub file_name: String,
    pub source_bytes: u64,
}

/// How a session ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    Cropped(CropRect),
    Cancelled,
    TimedOut,
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

/// Serve one selection to completion.
pub(crate) fn serve(bound: Bound, session: &Session, idle: Duration) -> Result<Outcome> {
    let Bound {
        server,
        addr: _,
        token,
    } = bound;

    loop {
        let Some(mut req) = server.recv_timeout(idle)? else {
            return Ok(Outcome::TimedOut);
        };

        let url = req.url().to_string();
        let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));

        if !token_ok(query, &token) {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            continue;
        }

        match (req.method(), path) {
            (Method::Get, "/") => {
                let page = page_html(session, &token);
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
            _ => {
                let _ = req.respond(Response::from_string("not found").with_status_code(404));
            }
        }
    }
}

/// Bind, announce, and serve with the production idle timeout.
pub(crate) fn run(session: &Session) -> Result<Outcome> {
    run_with_timeout(session, IDLE_TIMEOUT)
}

pub(crate) fn run_with_timeout(session: &Session, idle: Duration) -> Result<Outcome> {
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

    serve(bound, session, idle)
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

    /// Start a server on a background thread, returning its address, token and
    /// a handle that yields the Outcome.
    fn start(
        session: Session,
        idle: Duration,
    ) -> (String, String, std::thread::JoinHandle<Outcome>) {
        let bound = bind().unwrap();
        let addr = bound.addr.clone();
        let token = bound.token.clone();
        let handle = std::thread::spawn(move || serve(bound, &session, idle).unwrap());
        (addr, token, handle)
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
    fn page_does_not_leak_the_token_into_markup() {
        // The browser already has the token in its URL; the page uses
        // location.search, so the token must not be baked into the HTML.
        let html = page_html(&session(), "sekrit");
        assert!(!html.contains("sekrit"));
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
    fn idle_timeout_ends_the_session() {
        let (_addr, _token, handle) = start(session(), Duration::from_millis(50));
        assert!(matches!(handle.join().unwrap(), Outcome::TimedOut));
    }
}
