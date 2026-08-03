use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use kimix_shell::agent::config::Config as AgentConfig;
use kimix_shell::util::kimix_home::kimix_home;

#[derive(Debug, clap::Args, Clone)]
pub struct TraceArgs {
    /// Session ID to export
    #[arg(required_unless_present = "archive")]
    pub session_id: Option<String>,
    /// Output path (default: $KIMIX_SHARE_DIR/trace-exports/<session-id>.tar.gz)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
    /// Archive large idle sessions into ~/.kimix/session-archive/ and remove
    /// them from the sessions dir (replaces archive-large-sessions.sh)
    #[arg(long)]
    pub archive: bool,
    /// With --archive: report candidates without archiving
    #[arg(long)]
    pub dry_run: bool,
    /// With --archive: minimum session size in MiB (default 30)
    #[arg(long, default_value_t = 30)]
    pub min_size_mb: u64,
    /// With --archive: minimum idle days since last activity (default 7)
    #[arg(long, default_value_t = 7)]
    pub idle_days: u64,
}

#[derive(serde::Serialize)]
struct TraceResult {
    session_id: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn run(args: TraceArgs, _agent_config: &AgentConfig) -> Result<()> {
    if args.archive {
        return run_archive(args.min_size_mb, args.idle_days, args.dry_run, args.json);
    }
    let session_id = args
        .session_id
        .as_deref()
        .context("missing session id (use `kimix trace --archive` for the archive sweep)")?;
    run_export(session_id, args.output.as_deref(), args.json).await
}

// ---------------------------------------------------------------------------
// Archive construction
// ---------------------------------------------------------------------------

pub fn build_session_tar(session_dir: &Path, session_id: &str) -> Result<Vec<u8>> {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    tracing::info!(
        session_id = %session_id,
        session_dir = %session_dir.display(),
        "trace_cmd: building session tar.gz archive"
    );

    let mut archive_data = Vec::new();
    let mut file_count: u32 = 0;
    {
        let encoder = GzEncoder::new(&mut archive_data, Compression::default());
        let mut archive = tar::Builder::new(encoder);

        file_count += add_directory_to_tar(&mut archive, session_dir, session_id)?;

        let metadata = ExportMetadata {
            session_id: session_id.to_owned(),
            kimix_version: env!("VERSION_WITH_COMMIT").to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            exported_at: chrono::Utc::now().to_rfc3339(),
        };
        let meta_bytes = serde_json::to_vec_pretty(&metadata)?;
        append_bytes(
            &mut archive,
            &format!("{session_id}/export_metadata.json"),
            &meta_bytes,
        );
        file_count += 1;

        archive
            .into_inner()
            .and_then(|encoder| encoder.finish())
            .context("Failed to finalize tar.gz archive")?;
    }

    tracing::info!(
        session_id = %session_id,
        file_count,
        archive_bytes = archive_data.len(),
        "trace_cmd: archive built"
    );

    Ok(archive_data)
}

#[derive(serde::Serialize)]
struct ExportMetadata {
    session_id: String,
    kimix_version: String,
    os: String,
    arch: String,
    exported_at: String,
}

fn append_bytes<W: std::io::Write>(archive: &mut tar::Builder<W>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    set_mtime(&mut header);
    if let Err(e) = archive.append_data(&mut header, path, data) {
        tracing::warn!(error = %e, "trace_cmd: failed to add file to archive");
        eprintln!("  Warning: failed to add {path}: {e}");
    }
}

fn set_mtime(header: &mut tar::Header) {
    header.set_mtime(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );
}

/// Returns the number of files added.
fn add_directory_to_tar<W: std::io::Write>(
    archive: &mut tar::Builder<W>,
    dir: &Path,
    prefix: &str,
) -> Result<u32> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("Failed to read {}", dir.display()))?;

    let mut count: u32 = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let archive_path = format!("{prefix}/{name_str}");

        if path.is_dir() {
            count += add_directory_to_tar(archive, &path, &archive_path)?;
        } else if path.is_file() {
            match std::fs::read(&path) {
                Ok(data) => {
                    append_bytes(archive, &archive_path, &data);
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "trace_cmd: failed to read file for archive"
                    );
                    eprintln!("  Warning: failed to read {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(count)
}

// ---------------------------------------------------------------------------
// Local export
// ---------------------------------------------------------------------------

pub(crate) fn find_session_dir(session_id: &str) -> Result<PathBuf> {
    kimix_shell::session::persistence::find_session_dir_by_id(session_id).with_context(|| {
        format!(
            "Session '{session_id}' not found under {}",
            crate::util::display_user_kimix_path("sessions")
        )
    })
}

pub fn trace_exports_dir() -> PathBuf {
    kimix_home().join("trace-exports")
}

/// Creates parent directory if needed.
pub fn save_local_bundle(
    archive: &[u8],
    session_id: &str,
    output: Option<&Path>,
) -> Result<PathBuf> {
    let output_path = match output {
        Some(p) => p.to_path_buf(),
        None => trace_exports_dir().join(format!("{session_id}.tar.gz")),
    };

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    std::fs::write(&output_path, archive)
        .with_context(|| format!("Failed to write {}", output_path.display()))?;

    tracing::info!(
        session_id = %session_id,
        path = %output_path.display(),
        size_bytes = archive.len(),
        "trace_cmd: local bundle saved"
    );

    Ok(output_path)
}

async fn run_export(session_id: &str, output: Option<&Path>, json: bool) -> Result<()> {
    let session_dir = find_session_dir(session_id)?;
    if !json {
        eprintln!("Found session at: {}", session_dir.display());
        eprintln!("Building session trace archive...");
    }

    let archive = build_session_tar(&session_dir, session_id)?;
    let output_path = save_local_bundle(&archive, session_id, output)?;

    if json {
        let result = TraceResult {
            session_id: session_id.to_owned(),
            status: "exported",
            local_path: Some(output_path.display().to_string()),
            error: None,
        };
        println!("{}", serde_json::to_string(&result)?);
    } else {
        let size_kb = archive.len() / 1024;
        eprintln!("Session trace exported ({size_kb} KB):");
        eprintln!("  {}", output_path.display());
        println!("{}", output_path.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Archive sweep (replaces the external archive-large-sessions.sh)
// ---------------------------------------------------------------------------

/// One candidate (or archived) large idle session.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveEntry {
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_path: Option<String>,
    size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_active: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveReport {
    dry_run: bool,
    scanned: usize,
    archived: usize,
    skipped_errors: usize,
    freed_bytes: u64,
    entries: Vec<ArchiveEntry>,
}

struct ArchiveCandidate {
    session_id: String,
    session_dir: PathBuf,
    size_bytes: u64,
    last_active: std::time::SystemTime,
}

fn run_archive(min_size_mb: u64, idle_days: u64, dry_run: bool, json: bool) -> Result<()> {
    let sessions_root = kimix_home().join("sessions");
    let archive_dir = kimix_home().join("session-archive");
    let candidates = collect_archive_candidates(&sessions_root, min_size_mb, idle_days);

    if !json {
        eprintln!("Archiving large idle sessions");
        eprintln!("  threshold: >= {min_size_mb} MiB and idle >= {idle_days} days");
        eprintln!("  scanning {}", sessions_root.display());
        if dry_run {
            eprintln!("  (dry run \u{2014} nothing will be archived)");
        }
    }

    let mut report = ArchiveReport {
        dry_run,
        scanned: candidates.len(),
        archived: 0,
        skipped_errors: 0,
        freed_bytes: 0,
        entries: Vec::new(),
    };

    for candidate in &candidates {
        let last_active = format_system_time(candidate.last_active);
        if dry_run {
            if !json {
                eprintln!(
                    "  [dry-run] {} ({} MiB, last active {last_active})",
                    candidate.session_id,
                    candidate.size_bytes >> 20
                );
            }
            report.entries.push(ArchiveEntry {
                session_id: candidate.session_id.clone(),
                archived: None,
                archive_path: None,
                size_bytes: candidate.size_bytes,
                last_active: Some(last_active),
                error: None,
            });
            continue;
        }
        match archive_candidate(candidate, &archive_dir) {
            Ok(path) => {
                report.archived += 1;
                report.freed_bytes += candidate.size_bytes;
                if !json {
                    eprintln!(
                        "  archived {} ({} MiB -> {})",
                        candidate.session_id,
                        candidate.size_bytes >> 20,
                        path.display()
                    );
                }
                report.entries.push(ArchiveEntry {
                    session_id: candidate.session_id.clone(),
                    archived: Some(true),
                    archive_path: Some(path.display().to_string()),
                    size_bytes: candidate.size_bytes,
                    last_active: Some(last_active),
                    error: None,
                });
            }
            Err(err) => {
                report.skipped_errors += 1;
                if !json {
                    eprintln!("  FAILED {}: {err:#}", candidate.session_id);
                }
                report.entries.push(ArchiveEntry {
                    session_id: candidate.session_id.clone(),
                    archived: Some(false),
                    archive_path: None,
                    size_bytes: candidate.size_bytes,
                    last_active: Some(last_active),
                    error: Some(format!("{err:#}")),
                });
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        eprintln!(
            "  done: {} archived ({} freed), {} failed, {} scanned",
            report.archived,
            kimix_tools::util::truncate::format_bytes(report.freed_bytes as usize),
            report.skipped_errors,
            report.scanned
        );
        println!("{}", archive_dir.display());
    }
    Ok(())
}

/// Scan `<sessions-root>/<encoded-cwd>/<session-id>/` for sessions larger
/// than `min_size_mb` MiB whose newest file is older than `idle_days` days.
fn collect_archive_candidates(
    sessions_root: &Path,
    min_size_mb: u64,
    idle_days: u64,
) -> Vec<ArchiveCandidate> {
    let mut candidates = Vec::new();
    let Ok(cwd_entries) = std::fs::read_dir(sessions_root) else {
        return candidates;
    };
    let min_bytes = min_size_mb.saturating_mul(1 << 20);
    let idle_secs = idle_days.saturating_mul(86400);
    let now = std::time::SystemTime::now();

    for cwd_entry in cwd_entries.flatten() {
        let cwd_dir = cwd_entry.path();
        if !cwd_dir.is_dir() {
            continue;
        }
        let Ok(sid_entries) = std::fs::read_dir(&cwd_dir) else {
            continue;
        };
        for sid_entry in sid_entries.flatten() {
            let session_dir = sid_entry.path();
            if !session_dir.is_dir() {
                continue;
            }
            let session_id = sid_entry.file_name().to_string_lossy().into_owned();
            if !is_uuidv7_shape(&session_id) {
                continue;
            }
            let stats = dir_stats(&session_dir);
            let idle = now
                .duration_since(stats.latest_mtime)
                .map(|d| d.as_secs())
                .unwrap_or(u64::MAX);
            if stats.size_bytes >= min_bytes && idle >= idle_secs {
                candidates.push(ArchiveCandidate {
                    session_id,
                    session_dir,
                    size_bytes: stats.size_bytes,
                    last_active: stats.latest_mtime,
                });
            }
        }
    }
    // Oldest candidates first: archive the most at-risk sessions first.
    candidates.sort_by_key(|c| c.last_active);
    candidates
}

struct DirStats {
    size_bytes: u64,
    latest_mtime: std::time::SystemTime,
}

/// Recursively sum file sizes and track the newest file mtime under `dir`.
fn dir_stats(dir: &Path) -> DirStats {
    let mut stats = DirStats {
        size_bytes: 0,
        latest_mtime: std::time::UNIX_EPOCH,
    };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                stats.size_bytes += meta.len();
                if let Ok(mtime) = meta.modified() {
                    stats.latest_mtime = stats.latest_mtime.max(mtime);
                }
            }
        }
    }
    stats
}

/// Build the tar.gz, write it to the archive dir, record the manifest line,
/// and only then remove the original session directory.
fn archive_candidate(candidate: &ArchiveCandidate, archive_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(archive_dir)
        .with_context(|| format!("Failed to create {}", archive_dir.display()))?;
    let date = chrono::Utc::now().format("%Y%m%d");
    let tar_path = archive_dir.join(format!("{}-{date}.tar.gz", candidate.session_id));

    let data = build_session_tar(&candidate.session_dir, &candidate.session_id)?;
    if data.is_empty() {
        anyhow::bail!(
            "archive produced no bytes; leaving session {} in place",
            candidate.session_id
        );
    }
    std::fs::write(&tar_path, &data)
        .with_context(|| format!("Failed to write {}", tar_path.display()))?;
    append_manifest(archive_dir, candidate, &tar_path)?;
    std::fs::remove_dir_all(&candidate.session_dir)
        .with_context(|| format!("Failed to remove {}", candidate.session_dir.display()))?;
    Ok(tar_path)
}

/// Append one JSONL line to `<archive-dir>/manifest.jsonl` (same schema as
/// the archived large-sessions script, so existing manifests stay readable).
fn append_manifest(
    archive_dir: &Path,
    candidate: &ArchiveCandidate,
    tar_path: &Path,
) -> Result<()> {
    use std::io::Write;
    let manifest = archive_dir.join("manifest.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest)
        .with_context(|| format!("Failed to open {}", manifest.display()))?;
    let entry = serde_json::json!({
        "sid": candidate.session_id,
        "archived_at": chrono::Utc::now().to_rfc3339(),
        "size_kb": candidate.size_bytes / 1024,
        "last_active": format_system_time(candidate.last_active),
        "original_dir": candidate.session_dir.display().to_string(),
        "archive": tar_path.display().to_string(),
    });
    writeln!(file, "{entry}")?;
    Ok(())
}

/// UUIDv7 shape: 36 chars, hex with dashes, version digit `7` at index 14.
fn is_uuidv7_shape(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    if bytes[14] != b'7' {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            if *b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

/// `%Y-%m-%d` of a `SystemTime` (UTC), or "unknown" for pre-epoch times.
fn format_system_time(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}
