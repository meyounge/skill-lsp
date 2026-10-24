use skill_lsp::{
    fnd::{self, Database, Entry},
    server::Backend,
};
use std::{env, fs, path::PathBuf, sync::Arc, time::Instant};
use tower::Service;
use tower_lsp::{LspService, Server};
// tower-lsp 0.20 stops its service on exit but keeps waiting for input EOF.
// Wake the owner when exit arrives, even if the editor keeps its pipe open.
struct ExitService {
    inner: LspService<Backend>,
    exit: Arc<tokio::sync::Notify>,
}
impl Service<tower_lsp::jsonrpc::Request> for ExitService {
    type Response = <LspService<Backend> as Service<tower_lsp::jsonrpc::Request>>::Response;
    type Error = <LspService<Backend> as Service<tower_lsp::jsonrpc::Request>>::Error;
    type Future = <LspService<Backend> as Service<tower_lsp::jsonrpc::Request>>::Future;
    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
    fn call(&mut self, req: tower_lsp::jsonrpc::Request) -> Self::Future {
        let exit = req.method() == "exit";
        let future = self.inner.call(req);
        if exit {
            self.exit.notify_one();
        }
        future
    }
}
const EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded.json"));
struct Options {
    fnd: Option<PathBuf>,
    database: Option<PathBuf>,
    index: Option<(PathBuf, PathBuf)>,
    tcp: bool,
    host: String,
    port: u16,
    max_completions: usize,
}
fn usage() {
    eprintln!(
        "skill-lsp {}\n\
Usage: skill-lsp [--fnd-dir DIR | --database FILE] [--tcp --host HOST --port PORT]\n\
       skill-lsp index DIR OUTPUT.json\n\
  --fnd-dir DIR       Parse local .fnd files once at startup\n\
  --database FILE     Load a previously compiled JSON API database\n\
  --max-completions N Optional response limit (default unlimited, matching Python)\n\
  --tcp              Listen over TCP (default stdio)\n\
  --host HOST        TCP bind address (default 127.0.0.1)\n\
  --port PORT        TCP port (default 2087)\n\
Environment: SKILL_FND_DIR for runtime docs; SKILL_EMBED_FND_DIR at build time.\n\
With no override, use embedded docs, ./SKILL, or SKILL beside the executable.",
        env!("CARGO_PKG_VERSION")
    );
}
fn options() -> Result<Option<Options>, String> {
    let mut args = env::args().skip(1);
    let mut o = Options {
        fnd: env::var_os("SKILL_FND_DIR").map(PathBuf::from),
        database: None,
        index: None,
        tcp: false,
        host: "127.0.0.1".into(),
        port: 2087,
        max_completions: usize::MAX,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                usage();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("skill-lsp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--fnd-dir" => o.fnd = Some(args.next().ok_or("--fnd-dir needs DIR")?.into()),
            "--database" => o.database = Some(args.next().ok_or("--database needs FILE")?.into()),
            "--tcp" => o.tcp = true,
            "--host" => o.host = args.next().ok_or("--host needs HOST")?,
            "--port" => {
                o.port = args
                    .next()
                    .ok_or("--port needs PORT")?
                    .parse()
                    .map_err(|_| "invalid port")?
            }
            "--max-completions" => {
                o.max_completions = args
                    .next()
                    .ok_or("--max-completions needs N")?
                    .parse()
                    .map_err(|_| "invalid completion limit")?;
                if o.max_completions == 0 {
                    return Err("completion limit must be positive".into());
                }
            }
            "index" => {
                let root = args.next().ok_or("index needs DIR OUTPUT.json")?;
                let out = args.next().ok_or("index needs DIR OUTPUT.json")?;
                o.index = Some((root.into(), out.into()));
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    Ok(Some(o))
}
fn load(o: &Options) -> Result<Vec<Entry>, String> {
    if let Some(path) = &o.database {
        let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // Accept our compact array and the Python parser's {functionName: entry} export.
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let entries: Vec<Entry> = if value.is_object() {
            serde_json::from_value::<std::collections::BTreeMap<String, Entry>>(value)
                .map_err(|e| e.to_string())?
                .into_values()
                .collect()
        } else {
            serde_json::from_value(value).map_err(|e| e.to_string())?
        };
        if entries
            .iter()
            .any(|e| !(1..=10).contains(&e.keyword_category))
        {
            return Err("database categories must be in 1..10".into());
        }
        return Ok(entries);
    }
    if let Some(root) = &o.fnd {
        return fnd::load_dir(root);
    }
    let embedded: Vec<Entry> = serde_json::from_slice(EMBEDDED).map_err(|e| e.to_string())?;
    if !embedded.is_empty() {
        return Ok(embedded);
    }
    // Preserve the Python server's Lite-XL default without hardcoding a user name.
    if let Some(home) = env::var_os("HOME") {
        let legacy = PathBuf::from(home).join(".config/lite-xl/plugins/skill_lsp/SKILL");
        if legacy.is_dir() {
            return fnd::load_dir(&legacy);
        }
    }
    let beside = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("SKILL")));
    for root in std::iter::once(PathBuf::from("SKILL")).chain(beside) {
        if root.is_dir() {
            return fnd::load_dir(&root);
        }
    }
    eprintln!("[skill-lsp] No API docs configured; syntax and user symbols remain available.");
    Ok(vec![])
}
fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime");
    let result = runtime.block_on(run());
    // Tokio's stdin reader is a blocking task. An editor may leave its pipe open
    // after `exit`; waiting for that reader during runtime drop would hang forever.
    runtime.shutdown_timeout(std::time::Duration::ZERO);
    if let Err(e) = result {
        eprintln!("[skill-lsp] {e}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    let Some(o) = options()? else {
        return Ok(());
    };
    if let Some((root, out)) = &o.index {
        let start = Instant::now();
        let entries = fnd::load_dir(root)?;
        fs::write(
            out,
            serde_json::to_vec(&entries).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        eprintln!(
            "Indexed {} functions in {:?}: {}",
            entries.len(),
            start.elapsed(),
            out.display()
        );
        return Ok(());
    }
    let start = Instant::now();
    let db = Arc::new(Database::new(load(&o)?));
    eprintln!(
        "[skill-lsp] {} API functions indexed in {:?}",
        db.entries.len(),
        start.elapsed()
    );
    let make_service = || {
        let db = db.clone();
        let (inner, socket) =
            LspService::new(move |client| Backend::new(client, db.clone(), o.max_completions));
        let exit = Arc::new(tokio::sync::Notify::new());
        (
            ExitService {
                inner,
                exit: exit.clone(),
            },
            socket,
            exit,
        )
    };
    if o.tcp {
        let listener = tokio::net::TcpListener::bind((o.host.as_str(), o.port))
            .await
            .map_err(|e| e.to_string())?;
        eprintln!("[skill-lsp] Listening on {}:{}", o.host, o.port);
        loop {
            let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let (read, write) = tokio::io::split(stream);
            let (service, socket, exit) = make_service();
            tokio::select! {_=Server::new(read,write,socket).concurrency_level(1).serve(service)=>{},_=exit.notified()=>{return Ok(());}}
        }
    } else {
        let (service, socket, exit) = make_service();
        tokio::select! {_=Server::new(tokio::io::stdin(),tokio::io::stdout(),socket).concurrency_level(1).serve(service)=>{},_=exit.notified()=>{}}
    }
    Ok(())
}
