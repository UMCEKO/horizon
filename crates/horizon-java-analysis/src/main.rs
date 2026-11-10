use std::io::{Read, Write};
use std::process::ExitCode;

use std::path::PathBuf;

use horizon_java_analysis::class::{self, Parse};
use horizon_java_analysis::{jars, libraries, mixins};
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
        Some(cmd @ ("dump-libraries" | "dump-jars")) => {
            match dump_install(cmd, &args.collect::<Vec<_>>()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("horizon-java-analysis: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("mixins") => match write_mixins(&args.collect::<Vec<_>>()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("horizon-java-analysis: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "usage: horizon-java-analysis dump-classes JAR... | dump-libraries --install DIR [--cache C] | dump-jars --install DIR | mixins --install DIR --out OUT"
            );
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

/// `--install DIR` and `--cache C` of a dump command.
fn install_args(args: &[String]) -> Result<(PathBuf, Option<PathBuf>), String> {
    let (mut install, mut cache) = (None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--install" => install = Some(PathBuf::from(value)),
            "--cache" => cache = Some(PathBuf::from(value)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok((install.ok_or("--install DIR is required")?, cache))
}

/// The library jars, their class index and the SPI names; or each mod archive's facts, as one JSON document.
fn dump_install(cmd: &str, args: &[String]) -> Result<(), String> {
    let (install, cache) = install_args(args)?;
    let library_jars = libraries::library_jars(&install.join("libraries"))?;
    let doc = if cmd == "dump-libraries" {
        let index = libraries::load_library_index(&library_jars, cache.as_deref())?;
        let rows: serde_json::Map<String, serde_json::Value> = index
            .iter()
            .map(|(k, c)| (k.clone(), c.to_json()))
            .collect();
        json!({
            "jars": library_jars.iter().map(|j| j.to_string_lossy()).collect::<Vec<_>>(),
            "index": rows,
            "spi": libraries::spi_names(&library_jars)?,
        })
    } else {
        let spis = libraries::spi_names(&library_jars)?;
        let mut rows = Vec::new();
        for jar in libraries::glob(&install, "mods/*.jar") {
            for mut archive in jars::walk(&jar)? {
                let facts = jars::jar_facts(&mut archive, &spis)
                    .map_err(|e| format!("{}: {e}", archive.label))?;
                rows.push(json!({"mod": archive.mod_id, "label": archive.label, "facts": facts}));
            }
        }
        json!(rows)
    };
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    writeln!(out, "{doc}").map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())
}

/// `mixins --install DIR --out OUT`: writes `mixins.json` and `mixins.md`.
fn write_mixins(args: &[String]) -> Result<(), String> {
    let (mut install, mut out) = (None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--install" => install = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let install = install.ok_or("--install DIR is required")?;
    let out = out.ok_or("--out OUT is required")?;
    let mut warnings = Vec::new();
    let (entries, classes) = mixins::scan_install(&install, &mut warnings)?;
    mixins::write_mixins(&out, &entries, classes, &warnings)
}
