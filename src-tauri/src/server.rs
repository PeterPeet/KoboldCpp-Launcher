//! Runs the KoboldCpp process: start with a `.kcpps`, capture its output, watch until it serves,
//! stop it. Uses plain threads so it works without an async runtime (and in tests).

use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_LOG_LINES: usize = 4000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ServerStatus {
    Stopped,
    /// Process running, HTTP not answering yet (loading or downloading models).
    Starting { port: u16 },
    Running { port: u16 },
    /// Stopped by itself or failed to start.
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerEvent {
    Status { status: ServerStatus },
    Log { line: String },
}

pub type Notify = Arc<dyn Fn(ServerEvent) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("KoboldCpp is already running")]
    AlreadyRunning,
    #[error("port {0} is already in use by another program")]
    PortBusy(u16),
    #[error("could not start KoboldCpp: {0}")]
    Spawn(std::io::Error),
}

struct Inner {
    child: Option<Child>,
    status: ServerStatus,
    logs: VecDeque<String>,
    /// Bumped on every start so threads of an old run stop touching state.
    run: u64,
    stop_requested: bool,
}

#[derive(Clone)]
pub struct ServerManager {
    inner: Arc<Mutex<Inner>>,
}

impl Default for ServerManager {
    fn default() -> Self {
        Self { inner: Arc::new(Mutex::new(Inner { child: None, status: ServerStatus::Stopped, logs: VecDeque::new(), run: 0, stop_requested: false })) }
    }
}

/// `true` if nothing listens on `port` yet (KoboldCpp binds all interfaces by default).
pub fn port_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok() && TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Minimal blocking GET to the local server; returns the body on HTTP 200.
pub fn local_get(port: u16, path: &str, timeout: Duration) -> Option<String> {
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), timeout).ok()?;
    s.set_read_timeout(Some(timeout)).ok()?;
    s.set_write_timeout(Some(timeout)).ok()?;
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n").ok()?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).ok()?;
    let (head, body) = resp.split_once("\r\n\r\n")?;
    head.lines().next().filter(|l| l.contains(" 200"))?;
    Some(body.to_string())
}

impl ServerManager {
    pub fn status(&self) -> ServerStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn logs(&self, last: usize) -> Vec<String> {
        let g = self.inner.lock().unwrap();
        g.logs.iter().skip(g.logs.len().saturating_sub(last)).cloned().collect()
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status(), ServerStatus::Starting { .. } | ServerStatus::Running { .. })
    }

    fn set_status(inner: &Arc<Mutex<Inner>>, run: u64, status: ServerStatus, notify: &Notify) {
        {
            let mut g = inner.lock().unwrap();
            if g.run != run || g.status == status {
                return;
            }
            g.status = status.clone();
        }
        notify(ServerEvent::Status { status });
    }

    /// Start `binary --config <config> --skiplauncher` in `cwd`.
    pub fn start(&self, binary: &Path, config: &Path, cwd: &Path, port: u16, notify: Notify) -> Result<(), ServerError> {
        if self.is_active() {
            return Err(ServerError::AlreadyRunning);
        }
        if !port_free(port) {
            return Err(ServerError::PortBusy(port));
        }
        let mut cmd = Command::new(binary);
        cmd.arg("--config").arg(config).arg("--skiplauncher").current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // Unbuffered Python output so the log updates live.
        cmd.env("PYTHONUNBUFFERED", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(ServerError::Spawn)?;
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
        // Everything KoboldCpp prints also goes to <config>.log (e.g. runs/last-run.log).
        let log_file = std::fs::File::create(config.with_extension("log")).ok().map(|f| Arc::new(Mutex::new(f)));
        let run = {
            let mut g = self.inner.lock().unwrap();
            g.run += 1;
            g.child = Some(child);
            g.logs.clear();
            g.stop_requested = false;
            g.status = ServerStatus::Starting { port };
            g.run
        };
        notify(ServerEvent::Status { status: ServerStatus::Starting { port } });

        for pipe in [stdout.map(|p| Box::new(p) as Box<dyn Read + Send>), stderr.map(|p| Box::new(p) as Box<dyn Read + Send>)].into_iter().flatten() {
            let inner = self.inner.clone();
            let notify = notify.clone();
            let log_file = log_file.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(pipe);
                let mut buf = Vec::new();
                // Split on \n and \r so progress lines (downloads) show up as they change.
                loop {
                    buf.clear();
                    let mut byte = [0u8; 1];
                    let mut got_any = false;
                    while reader.read(&mut byte).map(|n| n == 1).unwrap_or(false) {
                        got_any = true;
                        if byte[0] == b'\n' || byte[0] == b'\r' {
                            break;
                        }
                        buf.push(byte[0]);
                    }
                    if !got_any {
                        break;
                    }
                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(f) = &log_file
                        && let Ok(mut f) = f.lock()
                    {
                        let _ = writeln!(f, "{line}");
                    }
                    {
                        let mut g = inner.lock().unwrap();
                        if g.run != run {
                            return;
                        }
                        g.logs.push_back(line.clone());
                        while g.logs.len() > MAX_LOG_LINES {
                            g.logs.pop_front();
                        }
                    }
                    notify(ServerEvent::Log { line });
                }
            });
        }

        // Watcher: readiness and exit.
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            let mut ready = false;
            let mut last_probe = Instant::now() - Duration::from_secs(5);
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let exited = {
                    let mut g = inner.lock().unwrap();
                    if g.run != run {
                        return;
                    }
                    match g.child.as_mut().map(|c| c.try_wait()) {
                        Some(Ok(Some(code))) => Some((code, g.stop_requested)),
                        Some(Ok(None)) => None,
                        _ => Some((Default::default(), g.stop_requested)),
                    }
                };
                if let Some((code, requested)) = exited {
                    // Let the reader threads catch the last lines (often the error message).
                    std::thread::sleep(Duration::from_millis(300));
                    let status = if requested {
                        ServerStatus::Stopped
                    } else {
                        let tail: Vec<String> = {
                            let g = inner.lock().unwrap();
                            g.logs.iter().rev().take(4).rev().cloned().collect()
                        };
                        let why = if ready { "KoboldCpp stopped unexpectedly" } else { "KoboldCpp could not start" };
                        ServerStatus::Failed { message: format!("{why} ({code}). {}", tail.join(" / ")) }
                    };
                    inner.lock().unwrap().child = None;
                    Self::set_status(&inner, run, status, &notify);
                    return;
                }
                if !ready && last_probe.elapsed() >= Duration::from_secs(1) {
                    last_probe = Instant::now();
                    if local_get(port, "/api/extra/version", Duration::from_millis(800)).is_some() {
                        ready = true;
                        Self::set_status(&inner, run, ServerStatus::Running { port }, &notify);
                    }
                }
            }
        });
        Ok(())
    }

    /// Ask KoboldCpp to exit (SIGTERM on Unix), force it after `grace`.
    pub fn stop(&self, grace: Duration) {
        let pid = {
            let mut g = self.inner.lock().unwrap();
            g.stop_requested = true;
            g.child.as_ref().map(Child::id)
        };
        let Some(pid) = pid else { return };
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
        #[cfg(not(unix))]
        let _ = pid;
        let deadline = Instant::now() + grace;
        loop {
            let done = {
                let mut g = self.inner.lock().unwrap();
                match g.child.as_mut() {
                    None => true,
                    Some(c) => {
                        if cfg!(not(unix)) || Instant::now() >= deadline {
                            let _ = c.kill();
                            let _ = c.wait();
                            true
                        } else {
                            matches!(c.try_wait(), Ok(Some(_)))
                        }
                    }
                }
            };
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // The watcher thread reports Stopped; make sure state is right even if it already quit.
        let mut g = self.inner.lock().unwrap();
        if let Some(mut c) = g.child.take() {
            let _ = c.wait();
        }
        if matches!(g.status, ServerStatus::Starting { .. } | ServerStatus::Running { .. }) {
            g.status = ServerStatus::Stopped;
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    /// A stand-in for KoboldCpp: prints a line, then serves /api/extra/version with Python.
    fn fake_binary(dir: &Path, port: u16) -> std::path::PathBuf {
        std::fs::create_dir_all(dir.join("www/api/extra")).unwrap();
        std::fs::write(dir.join("www/api/extra/version"), r#"{"result":"KoboldCpp","version":"9.9"}"#).unwrap();
        let script = dir.join("koboldcpp");
        std::fs::write(&script, format!("#!/bin/sh\necho \"fake koboldcpp $@\"\ncd '{}' && exec python3 -m http.server {port} --bind 127.0.0.1\n", dir.join("www").display())).unwrap();
        crate::binary::make_executable(&script).unwrap();
        script
    }

    fn wait_for(m: &ServerManager, want: impl Fn(&ServerStatus) -> bool) -> ServerStatus {
        let end = Instant::now() + Duration::from_secs(20);
        while Instant::now() < end {
            let s = m.status();
            if want(&s) {
                return s;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        m.status()
    }

    #[test]
    fn starts_reports_ready_and_stops() {
        let dir = tempfile::tempdir().unwrap();
        let port = free_port();
        let bin = fake_binary(dir.path(), port);
        let cfg = dir.path().join("run.kcpps");
        std::fs::write(&cfg, "{}").unwrap();
        let events = Arc::new(AtomicUsize::new(0));
        let ev = events.clone();
        let m = ServerManager::default();
        m.start(&bin, &cfg, dir.path(), port, Arc::new(move |_| { ev.fetch_add(1, Ordering::SeqCst); })).unwrap();
        assert!(matches!(m.start(&bin, &cfg, dir.path(), port, Arc::new(|_| {})), Err(ServerError::AlreadyRunning)));
        assert_eq!(wait_for(&m, |s| matches!(s, ServerStatus::Running { .. })), ServerStatus::Running { port });
        assert!(m.logs(10).iter().any(|l| l.contains("fake koboldcpp --config") && l.contains("--skiplauncher")));
        assert!(std::fs::read_to_string(dir.path().join("run.log")).unwrap().contains("fake koboldcpp"));
        assert!(local_get(port, "/api/extra/version", Duration::from_secs(1)).unwrap().contains("9.9"));
        m.stop(Duration::from_secs(5));
        assert_eq!(m.status(), ServerStatus::Stopped);
        assert!(events.load(Ordering::SeqCst) >= 3);
        assert!(port_free(port));
    }

    #[test]
    fn reports_failure_when_process_exits() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bad");
        std::fs::write(&bin, "#!/bin/sh\necho 'Error: model file not found'\nexit 3\n").unwrap();
        crate::binary::make_executable(&bin).unwrap();
        let m = ServerManager::default();
        m.start(&bin, &dir.path().join("x.kcpps"), dir.path(), free_port(), Arc::new(|_| {})).unwrap();
        match wait_for(&m, |s| matches!(s, ServerStatus::Failed { .. })) {
            ServerStatus::Failed { message } => assert!(message.contains("model file not found"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn busy_port_is_refused() {
        let l = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let m = ServerManager::default();
        let r = m.start(Path::new("/bin/true"), Path::new("x"), Path::new("/"), port, Arc::new(|_| {}));
        assert!(matches!(r, Err(ServerError::PortBusy(p)) if p == port));
    }
}
