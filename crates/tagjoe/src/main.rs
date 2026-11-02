//! tagjoe command line.
#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tagjoe_core::config::Config;
use tagjoe_core::model::{Kind, Unit};
use tagjoe_core::naming::{Context, Template};
use tagjoe_core::propose::{self, Plan};
use tagjoe_core::scan;
use tagjoe_core::sources::http::Http;
use tagjoe_core::sources::identify::Identifier;
use tagjoe_core::schema::{self, Status};

/// println! that ignores a closed pipe (e.g. `tagjoe scan … | head`).
macro_rules! outln {
    ($($t:tt)*) => {{ let _ = writeln!(std::io::stdout().lock(), $($t)*); }};
}

#[derive(Parser)]
#[command(name = "tagjoe", version, about = "Check, propose and fix audio tags and file names (FileBot-style, for audio)")]
struct Cli {
    /// Configuration file (default: the user's config directory)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Force the kind of all scanned folders: album, compilation, audiobook
    #[arg(long, global = true)]
    kind: Option<String>,
    /// Machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,
    /// Do not use the network (tags and folder images only)
    #[arg(long, global = true)]
    offline: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read a folder and report every file and tag field (ok / missing / foreign)
    Scan { folder: PathBuf },
    /// Build the change plan (tags, cover, names) without writing anything
    Propose { folder: PathBuf },
    /// Show what "clean" would remove (foreign fields, secondary containers)
    Clean { folder: PathBuf },
    /// Identify a folder online and list the candidate releases
    Identify { folder: PathBuf },
    /// Render a name template for a file
    Preview {
        /// Template, e.g. "{albumartist} - {album} - {track} - {title}"
        #[arg(long)]
        format: String,
        file: PathBuf,
    },
    /// Print the effective configuration (TOML)
    Config,
}

fn load_config(path: Option<&PathBuf>) -> Config {
    match path {
        Some(p) => Config::load_from(p).unwrap_or_else(|| {
            eprintln!("warning: cannot read {}, using defaults", p.display());
            Config::default()
        }),
        None => Config::load(),
    }
}

fn apply_kind(units: &mut [Unit], kind: Option<&String>) -> Result<(), String> {
    if let Some(k) = kind {
        let kind = Kind::parse(k).ok_or_else(|| format!("unknown kind '{k}' (album, compilation, audiobook)"))?;
        for u in units {
            u.kind = kind;
        }
    }
    Ok(())
}

fn status_mark(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok     ",
        Status::Missing => "MISSING",
        Status::Differs => "DIFFERS",
        Status::Foreign => "FOREIGN",
    }
}

fn cmd_scan(folder: &Path, cfg: &Config, kind: Option<&String>, json: bool) -> Result<(), String> {
    let mut units = scan::scan_path(folder).map_err(|e| e.to_string())?;
    apply_kind(&mut units, kind)?;
    if json {
        let reports: Vec<_> = units
            .iter()
            .map(|u| {
                let files: Vec<_> = u.files.iter().map(|f| schema::check_file(f, u.kind, cfg)).collect();
                serde_json::json!({ "unit": u.path, "kind": u.kind, "files": files, "cover_files": u.cover_files, "warnings": u.warnings })
            })
            .collect();
        outln!("{}", serde_json::to_string_pretty(&reports).unwrap());
        return Ok(());
    }
    for u in &units {
        let total_secs = u.total_secs();
        outln!("== {}  [{}]  {} files, {:.0} min{}", u.path.display(), u.kind.label(), u.files.len(), total_secs / 60.0, if u.disc_folders { ", disc folders" } else { "" });
        if !u.cover_files.is_empty() {
            outln!("   cover files: {}", u.cover_files.iter().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).collect::<Vec<_>>().join(", "));
        }
        for w in &u.warnings {
            outln!("   warning: {w}");
        }
        let mut complete = 0;
        for f in &u.files {
            let r = schema::check_file(f, u.kind, cfg);
            if r.complete {
                complete += 1;
            }
            let missing: Vec<&str> = r.fields.iter().filter(|c| c.status == Status::Missing).map(|c| c.field.key()).collect();
            outln!(
                "   {:<60} {} {:>4}s {}{}{}{}",
                truncate(&f.file_name, 60),
                f.format,
                f.duration_secs as u64,
                if r.has_cover { "cover " } else { "NO-COVER " },
                if missing.is_empty() { String::new() } else { format!("missing[{}] ", missing.join(",")) },
                if r.foreign.is_empty() { String::new() } else { format!("foreign[{}] ", r.foreign.iter().map(|x| x.key.as_str()).collect::<Vec<_>>().join(",")) },
                if r.containers.is_empty() { String::new() } else { format!("containers[{}]", r.containers.join(",")) },
            );
        }
        outln!("   complete: {complete} of {}", u.files.len());
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

fn http(cfg: &Config, offline: bool) -> Http {
    let mut h = Http::new(tagjoe_core::USER_AGENT, Some(tagjoe_core::config::cache_dir().join("http")), cfg.sources.cache_days);
    h.offline = offline;
    h
}

fn progress(step: &str, done: usize, total: usize) {
    eprint!("\r{step}: {done}/{total}   ");
    if done == total {
        eprintln!();
    }
}

fn build_plans(folder: &Path, cfg: &Config, kind: Option<&String>, offline: bool) -> Result<Vec<Plan>, String> {
    let mut units = scan::scan_path(folder).map_err(|e| e.to_string())?;
    apply_kind(&mut units, kind)?;
    let http = http(cfg, offline);
    let ident = Identifier { http: &http, cfg };
    let mut plans = Vec::new();
    for u in &mut units {
        let base = propose::release_from_unit(u);
        let mut best: Option<(propose::ReleaseInfo, Vec<Option<usize>>)> = None;
        if !offline && u.kind != Kind::Audiobook {
            match ident.identify(u, 3, &progress) {
                Ok(cands) => {
                    for n in cands.first().map(|c| c.notes.clone()).unwrap_or_default() {
                        eprintln!("note: {n}");
                    }
                    if let Some(c) = cands.into_iter().next() {
                        if c.release.score >= 50 {
                            let mut rel = c.release;
                            if let Err(e) = ident.fetch_cover(&mut rel) {
                                eprintln!("note: cover: {e}");
                            }
                            if rel.cover.is_none() {
                                rel.cover = base.cover.clone();
                            }
                            u.kind = tagjoe_core::sources::identify::kind_from_release(&rel, u.kind);
                            best = Some((rel, c.matched));
                        } else {
                            eprintln!("note: best candidate scored only {} – using tags", c.release.score);
                        }
                    }
                }
                Err(e) => eprintln!("note: identification failed: {e}"),
            }
        }
        let (rel, matched) = best.unwrap_or_else(|| (base, propose::identity_match(u.files.len())));
        plans.push(propose::build_plan(u, &rel, &matched, cfg));
    }
    Ok(plans)
}

fn cmd_identify(folder: &Path, cfg: &Config, kind: Option<&String>, json: bool) -> Result<(), String> {
    let mut units = scan::scan_path(folder).map_err(|e| e.to_string())?;
    apply_kind(&mut units, kind)?;
    let http = http(cfg, false);
    let ident = Identifier { http: &http, cfg };
    for u in &units {
        outln!("== {}  [{}]  {} files", u.path.display(), u.kind.label(), u.files.len());
        let cands = ident.identify(u, 5, &progress).map_err(|e| e.to_string())?;
        if json {
            let v: Vec<_> = cands.iter().map(|c| serde_json::json!({"score": c.release.score, "summary": c.release.summary, "album": c.release.album, "matched": c.matched, "by_fingerprint": c.by_fingerprint, "by_heuristic": c.by_heuristic, "notes": c.notes})).collect();
            outln!("{}", serde_json::to_string_pretty(&v).unwrap());
            continue;
        }
        for n in cands.first().map(|c| c.notes.clone()).unwrap_or_default() {
            outln!("   note: {n}");
        }
        for c in &cands {
            let m = c.matched.iter().filter(|x| x.is_some()).count();
            outln!("   {:>3}%  {}  [{}]  matched {m}/{} (fingerprint {}, heuristic {})", c.release.score, c.release.summary, c.release.album.get(&tagjoe_core::Field::MbReleaseId).cloned().unwrap_or_default(), u.files.len(), c.by_fingerprint, c.by_heuristic);
        }
        if cands.is_empty() {
            outln!("   no candidates");
        }
    }
    Ok(())
}

fn print_plan(p: &Plan, clean_only: bool) {
    outln!("== {}  [{}]  source: {}", p.unit.display(), p.kind.label(), p.source);
    if let Some(nf) = &p.new_folder {
        outln!("   folder -> {}", nf.display());
    }
    for n in &p.notes {
        outln!("   note: {n}");
    }
    for f in &p.files {
        outln!("   {}", f.file_name);
        if !clean_only {
            if let Some(n) = &f.new_name {
                outln!("      name    -> {n}");
            }
            for c in f.changes.iter().filter(|c| c.status != Status::Ok) {
                outln!("      {} {:<22} {:?} -> {:?}", status_mark(c.status), c.field.key(), c.old, c.new);
            }
            if let Some(c) = &f.cover {
                if c.apply {
                    outln!("      cover   -> {}x{} ({} KB){}", c.width, c.height, c.bytes / 1024, if c.replaces_existing { ", replaces existing" } else { "" });
                }
            }
        }
        for r in &f.removals {
            outln!("      REMOVE  {:<22} {:?}", r.key, truncate(&r.value, 60));
        }
        for c in &f.remove_containers {
            outln!("      REMOVE container {c}");
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cfg = load_config(cli.config.as_ref());
    let r = match &cli.cmd {
        Cmd::Scan { folder } => cmd_scan(folder, &cfg, cli.kind.as_ref(), cli.json),
        Cmd::Propose { folder } | Cmd::Clean { folder } => build_plans(folder, &cfg, cli.kind.as_ref(), cli.offline || matches!(cli.cmd, Cmd::Clean { .. })).map(|plans| {
            if cli.json {
                outln!("{}", serde_json::to_string_pretty(&plans).unwrap());
            } else {
                for p in &plans {
                    print_plan(p, matches!(cli.cmd, Cmd::Clean { .. }));
                }
            }
        }),
        Cmd::Identify { folder } => cmd_identify(folder, &cfg, cli.kind.as_ref(), cli.json),
        Cmd::Preview { format, file } => (|| {
            let info = scan::read_file(file).map_err(|e| e.to_string())?;
            let t = Template::parse(format).map_err(|e| e.to_string())?;
            let unit = Unit { path: file.parent().map(PathBuf::from).unwrap_or_default(), kind: Kind::parse(cli.kind.as_deref().unwrap_or("album")).unwrap_or(Kind::Album), files: vec![info.clone()], cover_files: vec![], disc_folders: false, warnings: vec![] };
            let rel = propose::release_from_unit(&unit);
            let mut merged = rel.album.clone();
            merged.extend(rel.tracks[0].fields.clone());
            let track: u32 = merged.get(&tagjoe_core::Field::TrackNumber).and_then(|s| s.parse().ok()).unwrap_or(1);
            let total: u32 = merged.get(&tagjoe_core::Field::TrackTotal).and_then(|s| s.parse().ok()).unwrap_or(1);
            let ctx: Context = propose::context_for(unit.kind, &merged, &info.ext, (track, total, 1, 1), &cfg);
            let stem = tagjoe_core::naming::render_file_stem(&t, &ctx, &info.ext, cfg.templates.max_name_bytes).map_err(|e| e.to_string())?;
            outln!("{stem}.{}", info.ext);
            Ok(())
        })(),
        Cmd::Config => {
            outln!("{}", cfg.to_toml());
            Ok(())
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
