use karia::{Definition, Diagnostic, Index};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    uri: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    name: String,
}
#[derive(Serialize)]
struct Response<T: Serialize> {
    id: u64,
    result: T,
}
#[derive(Serialize)]
struct ErrorResponse {
    id: u64,
    error: String,
}
#[derive(Serialize)]
struct CheckOutput {
    definitions: Vec<Definition>,
    diagnostics: Vec<Diagnostic>,
}

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("serve") => serve(),
        Some("check") => {
            let root = args.next().unwrap_or_else(|| ".".into());
            let mut format = "json".to_owned();
            while let Some(option) = args.next() {
                if option == "--format" {
                    format = args.next().unwrap_or_default();
                } else {
                    eprintln!("unknown option: {option}");
                    std::process::exit(2);
                }
            }
            if format != "json" {
                eprintln!("unsupported format: {format}");
                std::process::exit(2);
            }
            let out = check(Path::new(&root));
            let failed = !out.diagnostics.is_empty();
            println!("{}", serde_json::to_string(&out).unwrap());
            if failed {
                std::process::exit(1);
            }
        }
        Some("inspect") => {
            let root = args.next().unwrap_or_else(|| ".".into());
            let mut token = None;
            while let Some(a) = args.next() {
                if a == "--token" {
                    token = args.next();
                    if token.is_none() {
                        eprintln!("--token requires a name");
                        std::process::exit(2);
                    }
                } else {
                    eprintln!("unknown option: {a}");
                    std::process::exit(2);
                }
            }
            let mut idx = Index::new();
            if !scan(Path::new(&root), &mut idx) {
                std::process::exit(1);
            }
            let result = token
                .map(|n| idx.inspect(&n))
                .unwrap_or_else(|| idx.variables());
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        _ => {
            eprintln!("usage: karia <serve|check|inspect> <root>");
            std::process::exit(2);
        }
    }
}

fn serve() {
    let stdin = io::stdin();
    let mut out = io::BufWriter::new(io::stdout());
    let mut idx = Index::new();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(x) => x,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(x) => x,
            Err(e) => {
                let _ = writeln!(
                    out,
                    "{{\"id\":0,\"error\":{}}}",
                    serde_json::to_string(&e.to_string()).unwrap()
                );
                let _ = out.flush();
                continue;
            }
        };
        match req.method.as_str() {
            "update" => {
                idx.update(&req.uri, &req.text);
                write_json(
                    &mut out,
                    Response {
                        id: req.id,
                        result: serde_json::Value::Null,
                    },
                );
            }
            "remove" => {
                idx.remove(&req.uri);
                write_json(
                    &mut out,
                    Response {
                        id: req.id,
                        result: serde_json::Value::Null,
                    },
                );
            }
            "variables" => write_json(
                &mut out,
                Response {
                    id: req.id,
                    result: idx.variables(),
                },
            ),
            "classes" => write_json(
                &mut out,
                Response {
                    id: req.id,
                    result: idx.classes(&req.uri),
                },
            ),
            "inspect" => write_json(
                &mut out,
                Response {
                    id: req.id,
                    result: idx.inspect(&req.name),
                },
            ),
            "diagnostics" => write_json(
                &mut out,
                Response {
                    id: req.id,
                    result: idx.diagnostics(&req.uri),
                },
            ),
            _ => write_json(
                &mut out,
                ErrorResponse {
                    id: req.id,
                    error: format!("Unknown method: {}", req.method),
                },
            ),
        }
        let _ = out.flush();
    }
}
fn write_json<T: Serialize>(out: &mut impl Write, value: T) {
    let _ = serde_json::to_writer(&mut *out, &value);
    let _ = writeln!(out);
}

fn check(root: &Path) -> CheckOutput {
    let mut idx = Index::new();
    if !scan(root, &mut idx) {
        std::process::exit(1);
    }
    let mut diagnostics = Vec::new();
    let paths = match files(root) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    for path in paths {
        let uri = path.to_string_lossy().into_owned();
        diagnostics.extend(idx.diagnostics(&uri));
    }
    CheckOutput {
        definitions: idx.variables(),
        diagnostics,
    }
}
fn scan(root: &Path, idx: &mut Index) -> bool {
    if !root.exists() || !root.is_dir() {
        eprintln!("root is not a readable directory: {}", root.display());
        return false;
    }
    let paths = match files(root) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("{e}");
            return false;
        }
    };
    for path in paths {
        match fs::read_to_string(&path) {
            Ok(text) => idx.update(&path.to_string_lossy(), &text),
            Err(e) => {
                eprintln!("could not read {}: {e}", path.display());
                return false;
            }
        }
    }
    true
}
fn files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    visit(root, &mut out, &mut visited)?;
    out.sort();
    Ok(out)
}
fn visit(
    path: &Path,
    out: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    let canonical =
        fs::canonicalize(path).map_err(|e| format!("could not access {}: {e}", path.display()))?;
    if !visited.insert(canonical) {
        return Ok(());
    }
    let rd = fs::read_dir(path)
        .map_err(|e| format!("could not read directory {}: {e}", path.display()))?;
    for entry in rd {
        let e = entry
            .map_err(|e| format!("could not read directory entry in {}: {e}", path.display()))?;
        let p = e.path();
        if fs::symlink_metadata(&p)
            .map_err(|e| format!("could not inspect {}: {e}", p.display()))?
            .file_type()
            .is_symlink()
        {
            continue;
        }
        if p.is_dir() {
            let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
            if matches!(n, "node_modules" | "dist" | "target" | ".git") {
                continue;
            }
            visit(&p, out, visited)?;
        } else if p.extension().and_then(|x| x.to_str()) == Some("css") {
            out.push(p);
        }
    }
    Ok(())
}
