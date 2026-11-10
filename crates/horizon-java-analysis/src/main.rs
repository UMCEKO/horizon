use std::io::{Read, Write};
use std::process::ExitCode;

use horizon_java_analysis::class::{self, Parse};
use serde_json::json;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("dump-classes") => match dump_classes(&args.collect::<Vec<_>>()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("horizon-java-analysis: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: horizon-java-analysis dump-classes JAR...");
            ExitCode::from(2)
        }
    }
}

/// One JSON line per class file of each jar, in the zip's entry order.
fn dump_classes(jars: &[String]) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for jar in jars {
        let file = std::fs::File::open(jar).map_err(|e| format!("{jar}: {e}"))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{jar}: {e}"))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| format!("{jar}: {e}"))?;
            let name = entry.name().map_or_else(
                |_| String::from_utf8_lossy(entry.name_raw()).into_owned(),
                |n| n.into_owned(),
            );
            if !name.ends_with(".class")
                || name.starts_with("META-INF/")
                || name == "module-info.class"
            {
                continue;
            }
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| format!("{jar}!{name}: {e}"))?;
            let line = match class::parse_class(&data, Parse::all()) {
                Ok(info) => {
                    let mut v = class::dump(&info);
                    v["entry"] = json!(name);
                    v
                }
                Err(_) => json!({"entry": name, "error": true}),
            };
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
        }
    }
    out.flush().map_err(|e| e.to_string())
}
