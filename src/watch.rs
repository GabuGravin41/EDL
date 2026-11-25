//! `edl watch` — live co-editing bridge (zero dependencies, std only).
//!
//! * Watches the `.edl` source (AI or human edits) and hot-reloads an SVG preview in the browser.
//! * With `--kicad <file.kicad_sch>`: every EDL change re-exports the schematic for KiCad, and every save
//!   made in KiCad is diffed back into the `.edl` source through `edl sync` (verified before it is written).
//! * The last *good* render is always kept; compile errors are shown as an overlay instead of blanking the view.

use crate::core::span::Severity;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub struct WatchState {
    pub edl: PathBuf,
    pub kicad: Option<PathBuf>,
    pub version: u64,
    pub ok: bool,
    pub svg: String,
    pub summary: Value,
    pub errors: Vec<Value>,
    pub events: Vec<String>,
    last_edl_mtime: Option<SystemTime>,
    last_kicad_mtime: Option<SystemTime>,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl WatchState {
    pub fn new(edl: &Path, kicad: Option<&Path>) -> Self {
        let mut s = WatchState {
            edl: edl.to_path_buf(),
            kicad: kicad.map(|p| p.to_path_buf()),
            version: 0,
            ok: false,
            svg: String::new(),
            summary: json!({}),
            errors: Vec::new(),
            events: Vec::new(),
            last_edl_mtime: None,
            last_kicad_mtime: None,
        };
        s.recompile(true);
        s.last_edl_mtime = mtime(&s.edl);
        s
    }

    fn log(&mut self, msg: impl Into<String>) {
        self.events.push(msg.into());
        if self.events.len() > 30 {
            self.events.remove(0);
        }
    }

    /// Recompile; on success refresh preview (+ KiCad export when `export`). Returns true if state changed.
    fn recompile(&mut self, export: bool) {
        match crate::compile_file(&self.edl) {
            Err(d) => {
                self.ok = false;
                self.errors = vec![serde_json::to_value(&d).unwrap_or(json!({"message": d.message}))];
                self.log(format!("compile error {}: {}", d.code, d.message));
            }
            Ok(ir) => {
                self.svg = crate::render_svg(&ir, false);
                let total = ir.evidence.len();
                let exact = ir
                    .evidence
                    .iter()
                    .filter(|e| format!("{:?}", e.status) == "ExactlyChecked")
                    .count();
                self.errors = ir
                    .diagnostics
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .map(|d| serde_json::to_value(d).unwrap_or(json!({})))
                    .collect();
                self.ok = !ir.project.has_errors;
                self.summary = json!({
                    "project": ir.project.name,
                    "domain": ir.project.domain,
                    "evidence_total": total,
                    "evidence_exact": exact,
                    "has_errors": ir.project.has_errors,
                });
                self.log(format!(
                    "compiled {} ({}/{} checks exact{})",
                    ir.project.name,
                    exact,
                    total,
                    if ir.project.has_errors { ", ERRORS" } else { "" }
                ));
                if export && ir.project.domain == "electronics" {
                    if let Some(k) = self.kicad.clone() {
                        match std::fs::write(&k, crate::export_kicad_sch(&ir)) {
                            Ok(_) => {
                                self.last_kicad_mtime = mtime(&k);
                                self.log(format!("exported {}", k.display()));
                            }
                            Err(e) => self.log(format!("kicad export failed: {e}")),
                        }
                    }
                }
            }
        }
        self.version += 1;
    }

    /// Poll the filesystem once. Returns true when something changed.
    pub fn refresh(&mut self) -> bool {
        let before = self.version;
        let mut export = true;

        // 1. Human saved in KiCad -> sync back into EDL (verified), then recompile without clobbering their file.
        if let Some(k) = self.kicad.clone() {
            let now = mtime(&k);
            if now.is_some() && now != self.last_kicad_mtime {
                self.last_kicad_mtime = now;
                match crate::core::sync::run_sync(&self.edl, Some(&k), None, true) {
                    Ok((plan, res)) => {
                        self.log(format!(
                            "kicad save detected: {} patch(es) — {}",
                            plan.patch_count(),
                            res.message
                        ));
                        for a in plan.actions.iter().filter(|a| a.status != "patch") {
                            self.log(format!("  [{}] {}", a.status, a.description));
                        }
                        if res.written {
                            self.recompile(false);
                            self.last_edl_mtime = mtime(&self.edl);
                            export = false;
                        }
                    }
                    Err(e) => self.log(format!("kicad sync failed: {e}")),
                }
            }
        }

        // 2. EDL changed (AI / editor) -> recompile + re-export for KiCad.
        let m = mtime(&self.edl);
        if m != self.last_edl_mtime {
            self.last_edl_mtime = m;
            if export {
                self.recompile(true);
            }
        }
        self.version != before
    }

    pub fn state_json(&self) -> Value {
        json!({
            "version": self.version,
            "changed": true,
            "ok": self.ok,
            "file": self.edl.display().to_string(),
            "summary": self.summary,
            "errors": self.errors,
            "events": self.events,
            "kicad": self.kicad.as_ref().map(|p| p.display().to_string()),
            "svg": self.svg,
        })
    }
}

pub fn handle_request(state: &Mutex<WatchState>, path_and_query: &str) -> (u16, &'static str, Vec<u8>) {
    let (path, query) = path_and_query.split_once('?').unwrap_or((path_and_query, ""));
    match path {
        "/" | "/index.html" => (200, "text/html; charset=utf-8", INDEX_HTML.as_bytes().to_vec()),
        "/api/state" => {
            let st = state.lock().unwrap();
            let since: Option<u64> = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("since="))
                .and_then(|v| v.parse().ok());
            let body = if since == Some(st.version) {
                json!({"version": st.version, "changed": false})
            } else {
                st.state_json()
            };
            (200, "application/json", serde_json::to_vec(&body).unwrap_or_default())
        }
        "/preview.svg" => {
            let st = state.lock().unwrap();
            (200, "image/svg+xml", st.svg.clone().into_bytes())
        }
        _ => (404, "text/plain", b"not found".to_vec()),
    }
}

fn serve_conn(mut stream: TcpStream, state: Arc<Mutex<WatchState>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1500)));
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    let first = req.lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (code, ctype, body) = if method == "GET" {
        handle_request(&state, target)
    } else {
        (405, "text/plain", b"method not allowed".to_vec())
    };
    let status = match code {
        200 => "OK",
        404 => "Not Found",
        _ => "Method Not Allowed",
    };
    let head = format!(
        "HTTP/1.1 {code} {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

/// Serve until `stop` is set. Spawns the file-poll loop and one short-lived thread per connection.
pub fn serve(listener: TcpListener, state: Arc<Mutex<WatchState>>, stop: Arc<AtomicBool>) {
    let _ = listener.set_nonblocking(true);
    {
        let (st, stop) = (state.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                if let Ok(mut s) = st.lock() {
                    s.refresh();
                }
            }
        });
    }
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let st = state.clone();
                std::thread::spawn(move || serve_conn(stream, st));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

pub fn run_watch(edl: &Path, port: u16, kicad: Option<&Path>) -> Result<(), String> {
    if !edl.exists() {
        return Err(format!("{} does not exist", edl.display()));
    }
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
    let state = Arc::new(Mutex::new(WatchState::new(edl, kicad)));
    {
        let s = state.lock().unwrap();
        println!("EDL watch: {}", s.edl.display());
        println!("  preview : http://127.0.0.1:{port}");
        if let Some(k) = &s.kicad {
            println!("  kicad   : {} (two-way sync; EDL is source of truth at startup)", k.display());
        }
        println!("  status  : {}", s.events.last().cloned().unwrap_or_default());
        println!("Ctrl+C to stop.");
    }
    serve(listener, state, Arc::new(AtomicBool::new(false)));
    Ok(())
}

const INDEX_HTML: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>EDL Live</title>
<style>
 html,body{margin:0;height:100%;font-family:system-ui,sans-serif;background:#0f172a;color:#e2e8f0}
 header{display:flex;gap:14px;align-items:center;padding:8px 14px;background:#111827;border-bottom:1px solid #1f2937}
 h1{font-size:15px;margin:0;font-weight:600}
 #badge{padding:3px 10px;border-radius:999px;font-size:12px;font-weight:700}
 .good{background:#14532d;color:#bbf7d0}.bad{background:#7f1d1d;color:#fecaca}
 #file{color:#94a3b8;font-size:12px;font-family:monospace}
 main{display:flex;height:calc(100% - 42px)}
 #view{flex:1;overflow:hidden;position:relative;cursor:grab;background:#f8fafc}
 #stage{transform-origin:0 0;position:absolute;left:0;top:0}
 aside{width:340px;background:#0b1220;border-left:1px solid #1f2937;overflow:auto;font-size:12px}
 aside h2{font-size:12px;margin:10px 12px 4px;color:#94a3b8;text-transform:uppercase;letter-spacing:.06em}
 #events div{padding:3px 12px;font-family:monospace;border-bottom:1px solid #111827}
 #errs .e{margin:6px 12px;padding:8px;background:#2a1215;border:1px solid #7f1d1d;border-radius:6px}
 #errs code{color:#fca5a5}
</style></head><body>
<header><h1>EDL Live</h1><span id="badge" class="bad">connecting…</span><span id="file"></span></header>
<main><div id="view"><div id="stage"></div></div>
<aside><h2>Errors</h2><div id="errs"></div><h2>Activity</h2><div id="events"></div></aside></main>
<script>
let version=-1,scale=1,tx=0,ty=0,fitted=false;
const view=document.getElementById('view'),stage=document.getElementById('stage');
function apply(){stage.style.transform=`translate(${tx}px,${ty}px) scale(${scale})`}
view.addEventListener('wheel',e=>{e.preventDefault();const r=view.getBoundingClientRect();
 const mx=e.clientX-r.left,my=e.clientY-r.top,k=e.deltaY<0?1.12:1/1.12;
 tx=mx-(mx-tx)*k;ty=my-(my-ty)*k;scale*=k;apply()},{passive:false});
let drag=null;view.addEventListener('mousedown',e=>{drag=[e.clientX-tx,e.clientY-ty];view.style.cursor='grabbing'});
window.addEventListener('mouseup',()=>{drag=null;view.style.cursor='grab'});
window.addEventListener('mousemove',e=>{if(drag){tx=e.clientX-drag[0];ty=e.clientY-drag[1];apply()}});
function fit(){const s=stage.querySelector('svg');if(!s)return;const w=s.viewBox.baseVal.width||s.clientWidth,h=s.viewBox.baseVal.height||s.clientHeight;
 const r=view.getBoundingClientRect();scale=Math.min(r.width/w,r.height/h)*0.96;tx=(r.width-w*scale)/2;ty=(r.height-h*scale)/2;apply()}
view.addEventListener('dblclick',fit);
function esc(t){return String(t).replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]))}
async function poll(){
 try{const r=await fetch('/api/state?since='+version);const s=await r.json();
  if(s.changed){version=s.version;
   const b=document.getElementById('badge');
   if(s.ok){b.className='good';b.textContent=`ALL OK · ${s.summary.evidence_exact}/${s.summary.evidence_total} checks exact`}
   else{b.className='bad';b.textContent='ERRORS'}
   document.getElementById('file').textContent=s.file+(s.kicad?'  ⇄  '+s.kicad:'');
   if(s.svg){stage.innerHTML=s.svg;if(!fitted){fit();fitted=true}}
   document.getElementById('errs').innerHTML=s.errors.map(e=>`<div class="e"><code>${esc(e.code||'')}</code> ${esc(e.message||'')}${e.span?`<br><small>line ${e.span.start_line}</small>`:''}</div>`).join('')||'<div style="padding:4px 12px;color:#64748b">none</div>';
   document.getElementById('events').innerHTML=s.events.slice().reverse().map(x=>`<div>${esc(x)}</div>`).join('');
  }}catch(e){const b=document.getElementById('badge');b.className='bad';b.textContent='offline'}
 setTimeout(poll,500)}
poll();
</script></body></html>"##;
