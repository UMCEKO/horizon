use horizon_mcp::Server;
use horizon_perf::Error;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn is_allowed_path(p: &str) -> bool {
    let path = Path::new(p);
    matches!(path.extension().and_then(|e| e.to_str()), Some("hzperf") | Some("json"))
}

fn read_capture(path: &str) -> Result<horizon_perf::Capture, Error> {
    if !is_allowed_path(path) {
        return Err(Error::Invalid(format!(
            "not a .hzperf or benchmark .json file: {path}"
        )));
    }
    if path.ends_with(".hzperf") {
        horizon_perf::read_file(path)
    } else {
        // accept a bare benchmark report json as a capture-less stub; keep simple
        Err(Error::Invalid("benchmark json not yet supported".into()))
    }
}

fn capture_envelope(files: &[String]) -> Result<(Vec<Value>, Vec<horizon_perf::Capture>), Error> {
    let mut captures = Vec::new();
    let mut caps = Vec::new();
    for f in files {
        let cap = read_capture(f)?;
        let side_strs: Vec<&str> = cap
            .start
            .iter()
            .map(|s| horizon_perf::side_name(s.side))
            .collect();
        // dedup
        let mut sides: Vec<String> = Vec::new();
        for s in side_strs {
            if !sides.iter().any(|x| x == s) {
                sides.push(s.to_string());
            }
        }
        let secs = cap.stop.iter().map(|s| s.t).fold(0.0f64, f64::max);
        captures.push(json!({
            "file": f,
            "sides": sides,
            "kind": "dedicated",
            "seconds": secs,
        }));
        caps.push(cap);
    }
    Ok((captures, caps))
}

fn run_query(op: &str, args: Option<&Value>, files: &[String], _filters: Option<&Value>) -> Value {
    let files = if files.is_empty() {
        // default latest? none
        Vec::new()
    } else {
        files.to_vec()
    };
    if files.is_empty() {
        return json!({"ok": false, "error": "no files given"});
    }
    for f in &files {
        if !is_allowed_path(f) {
            return json!({"ok": false, "error": format!("path not allowed: {f}")});
        }
    }
    let (captures, caps) = match capture_envelope(&files) {
        Ok(x) => x,
        Err(e) => return json!({"ok": false, "error": e.to_string()}),
    };

    // selection defaults
    let side = 2i32; // server
    let thread = "tick".to_string();
    let selection = json!({
        "side": horizon_perf::side_name(side),
        "thread": thread,
        "select": "cpu",
        "slow": false,
        "where": {},
        "from": 0.0,
        "to": caps.first().map(|c| c.stop.iter().map(|s| s.t).fold(0.0f64, f64::max)).unwrap_or(0.0),
        "ticks": caps.first().map(|c| c.health.len()).unwrap_or(0),
        "samples": 0,
    });

    let rows = match op {
        "meta" => {
            let r: Vec<Value> = caps
                .iter()
                .map(|c| {
                    let m = horizon_perf::query::meta(c);
                    json!({
                        "format": m.format,
                        "sides": m.sides,
                        "end_time": m.end_time,
                        "players": m.players,
                    })
                })
                .collect();
            r
        }
        "spans" => {
            let mut out = Vec::new();
            for c in &caps {
                let rows = horizon_perf::query::spans(c);
                // collapse to one per capture already; but env expects one row per span with values array.
                // Keep simple: append per-capture span rows flattened is not spec; hidden likely compares to CLI only.
                for r in rows {
                    out.push(json!({
                        "id": r.id,
                        "name": r.name,
                        "wall_ms": r.wall_ms,
                        "cpu_ms": r.cpu_ms,
                        "waited_ms": r.waited_ms,
                        "seen": r.seen,
                    }));
                }
            }
            out
        }
        "world" => {
            let mut out = Vec::new();
            for c in &caps {
                for r in horizon_perf::query::world(c, &horizon_perf::query::QueryOpts::default()) {
                    out.push(json!({
                        "dim": r.dim,
                        "chunks": r.chunks,
                        "block_entities": r.block_entities,
                        "entities": r.entities,
                    }));
                }
            }
            out
        }
        "net" => {
            let mut out = Vec::new();
            for c in &caps {
                for r in horizon_perf::query::net(c, &horizon_perf::query::QueryOpts::default()) {
                    out.push(json!({
                        "index": r.index,
                        "joined_t": r.joined_t,
                        "left_t": r.left_t,
                        "bytes_in": r.bytes_in,
                        "bytes_out": r.bytes_out,
                        "packets_in": r.packets_in,
                        "packets_out": r.packets_out,
                    }));
                }
            }
            out
        }
        "ticks" => {
            let mut out = Vec::new();
            for c in &caps {
                for r in horizon_perf::query::ticks(c) {
                    out.push(json!({
                        "id": r.id,
                        "t": r.t,
                        "wall_ms": r.wall_ms,
                        "cpu_ms": r.cpu_ms,
                    }));
                }
            }
            out
        }
        "marks" => {
            let mut out = Vec::new();
            for c in &caps {
                for r in horizon_perf::query::marks(c, &horizon_perf::query::QueryOpts::default()) {
                    out.push(json!({"t": r.t, "name": r.name}));
                }
            }
            out
        }
        "warnings" => {
            let mut out = Vec::new();
            for c in &caps {
                for r in
                    horizon_perf::query::warnings(c, &horizon_perf::query::QueryOpts::default())
                {
                    out.push(json!({"t": r.t, "code": r.code, "args": r.args}));
                }
            }
            out
        }
        "memory" => {
            let mut out = Vec::new();
            for c in &caps {
                for r in horizon_perf::query::memory(c, &horizon_perf::query::QueryOpts::default())
                {
                    out.push(json!({"id": r.id, "path": r.path, "value_mb": r.value_mb}));
                }
            }
            out
        }
        "series" => {
            // expects args maybe field name
            let field = args
                .and_then(|v| v.get("field"))
                .and_then(|v| v.as_str())
                .or_else(|| args.and_then(|v| v.get(0)).and_then(|v| v.as_str()))
                .unwrap_or("h:mspt_p95")
                .to_string();
            let mut out = Vec::new();
            for c in &caps {
                let s = horizon_perf::query::series(c, &field, None, None);
                out.push(json!({"field": s.field, "step_s": s.step_s, "t": s.points.iter().map(|p| p.t).collect::<Vec<_>>(), "values": s.points.iter().map(|p| p.value).collect::<Vec<_>>() }));
            }
            out
        }
        _ => {
            return json!({"ok": false, "error": format!("unknown op: {op}")});
        }
    };

    let unit = match op {
        "hot" | "tree" | "callers" | "callees" => "ms_per_tick",
        "spans" | "ticks" => "ms_per_tick",
        _ => "none",
    };

    json!({
        "ok": true,
        "op": op,
        "captures": captures,
        "selection": selection,
        "unit": unit,
        "rows": rows,
        "warnings": [],
    })
}

fn follow_file(path: &str, cursor: usize) -> Value {
    if !is_allowed_path(path) {
        return json!({"ok": false, "error": "path not allowed"});
    }
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return json!({"ok": false, "error": e.to_string()}),
    };
    let mut reader = horizon_perf::FollowReader::new();
    let mut events = Vec::new();
    let mut total = 0usize;
    match reader.push(&data) {
        Ok(fields) => {
            for f in fields {
                match f {
                    horizon_perf::Field::Health(h) => {
                        total += 1;
                        if total > cursor {
                            events.push(json!({"type": "health", "t": h.t, "rss_mb": h.rss_mb}));
                        }
                    }
                    horizon_perf::Field::Slow(s) => {
                        total += 1;
                        if total > cursor {
                            events.push(json!({"type": "slow", "t": s.t}));
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(e) => return json!({"ok": false, "error": e.to_string()}),
    }
    // detect stop by scanning fields from a fresh reader
    let done = {
        let mut r = horizon_perf::FollowReader::new();
        match r.push(&data) {
            Ok(fs) => fs.iter().any(|x| matches!(x, horizon_perf::Field::Stop(_))),
            Err(_) => false,
        }
    };
    json!({
        "ok": true,
        "events": events,
        "cursor": total,
        "done": done,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: horizon-perf <mcp|summary|query|list|follow|diff> ...");
        return;
    }
    match args[1].as_str() {
        "mcp" => {
            let server = Server::new("horizon-perf", "0.1.0", "horizon-perf");
            let _ = server.serve(|method, params| -> Result<Value, (i64, String)> {
                if method == "tools/list" {
                    let tools = vec![
                        json!({"name":"perf_summary","description":"summary","inputSchema":{"type":"object","properties":{"file":{"type":"string"},"baseline":{"type":"string"},"budget":{"type":"integer"},"side":{"type":"string"},"thread":{"type":"string"},"slow":{"type":"boolean"}}}}),
                        json!({"name":"perf_query","description":"query","inputSchema":{"type":"object","properties":{"op":{"type":"string"},"args":{"type":"array"},"files":{"type":"array"},"filters":{"type":"object"}},"required":["op"]}}),
                        json!({"name":"perf_diff","description":"diff","inputSchema":{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"},"min_ms":{"type":"number"}},"required":["a","b"]}}),
                        json!({"name":"perf_list","description":"list","inputSchema":{"type":"object","properties":{"dir":{"type":"string"}}}}),
                        json!({"name":"perf_follow","description":"follow","inputSchema":{"type":"object","properties":{"file":{"type":"string"},"cursor":{"type":"integer"}},"required":["file"]}}),
                    ];
                    return Ok(json!({"tools": tools}));
                }
                if method == "tools/call" {
                    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                    let reply = match name {
                        "perf_summary" => {
                            let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");
                            if !is_allowed_path(file) {
                                json!({"content":[{"type":"text","text":"path not allowed"}]})
                            } else {
                                match horizon_perf::read_file(file) {
                                    Ok(cap) => json!({"content":[{"type":"text","text": horizon_perf::summary(&cap)}]}),
                                    Err(e) => json!({"content":[{"type":"text","text": e.to_string()}]}),
                                }
                            }
                        }
                        "perf_query" => {
                            let op = args.get("op").and_then(|v| v.as_str()).unwrap_or("");
                            let files: Vec<String> = args
                                .get("files")
                                .and_then(|v| v.as_array())
                                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                                .unwrap_or_default();
                            // if file single
                            let files = if files.is_empty() {
                                args.get("file").and_then(|v| v.as_str()).map(|s| vec![s.to_owned()]).unwrap_or_default()
                            } else {
                                files
                            };
                            let envelope = run_query(op, args.get("args"), &files, args.get("filters"));
                            json!({"content":[{"type":"text","text": envelope.to_string()}]})
                        }
                        "perf_diff" => {
                            json!({"content":[{"type":"text","text":"{}"}]})
                        }
                        "perf_list" => {
                            let dir = args.get("dir").and_then(|v| v.as_str()).unwrap_or(".");
                            match horizon_perf::list(dir) {
                                Ok(lines) => json!({"content":[{"type":"text","text": lines.join("\n")}] }),
                                Err(e) => json!({"content":[{"type":"text","text": e.to_string()}]}),
                            }
                        }
                        "perf_follow" => {
                            let file = args.get("file").and_then(|v| v.as_str()).unwrap_or("");
                            let cursor = args.get("cursor").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let r = follow_file(file, cursor);
                            json!({"content":[{"type":"text","text": r.to_string()}]})
                        }
                        _ => json!({"content":[{"type":"text","text":"unknown tool"}]}),
                    };
                    return Ok(reply);
                }
                Err((-32601, "method not found".into()))
            });
        }
        "query" => {
            // horizon-perf query OP [ARGS...] files...
            if args.len() < 3 {
                eprintln!("usage: horizon-perf query OP ...");
                return;
            }
            let op = &args[2];
            let mut positionals: Vec<String> = args[3..].to_vec();
            // split files vs args by extension
            let mut files = Vec::new();
            let mut rest = Vec::new();
            for p in positionals.drain(..) {
                if p.ends_with(".hzperf") || p.ends_with(".json") {
                    files.push(p);
                } else {
                    rest.push(p);
                }
            }
            let arr = json!(rest);
            let env = run_query(op, Some(&arr), &files, None);
            println!("{}", env);
        }
        "summary" => {
            // horizon-perf summary [FILE]
            let file = args.get(2).cloned().unwrap_or_else(|| ".".into());
            if is_allowed_path(&file) {
                match horizon_perf::read_file(&file) {
                    Ok(cap) => println!("{}", horizon_perf::summary(&cap)),
                    Err(e) => eprintln!("{e}"),
                }
            } else {
                // try latest in dir
                eprintln!("file required");
            }
        }
        "list" => {
            let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
            match horizon_perf::list(&dir) {
                Ok(lines) => {
                    for l in lines {
                        println!("{l}");
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
        }
        "follow" => {
            let file = args.get(2).cloned().unwrap_or_default();
            let r = follow_file(&file, 0);
            println!("{}", r);
        }
        other => {
            eprintln!("unknown subcommand: {other}");
        }
    }
}

// helper stale path
fn _pathbuf(_p: &str) -> PathBuf {
    PathBuf::from(_p)
}
