//! Rust-native GitHub mirror and read-only metadata backup, invoking the
//! official gh/git clients without shells or tokens in process arguments.
//! Remote deletion and organization write APIs are intentionally absent.
use crate::{archive, journal};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::{self, File}, io::{Read, Write}, path::Path, process::{Command, Stdio}};
use tempfile::TempDir;

pub type GithubResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn err(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
fn safe_slug(slug: &str) -> GithubResult<(&str, &str)> {
    let parts: Vec<_> = slug.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|part| {
        part.is_empty() || part.len() > 100 || *part == "." || *part == ".." ||
        !part.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    }) {
        return Err(err("Invalid GitHub owner/repository slug"));
    }
    Ok((parts[0], parts[1]))
}
fn private_dir(path: &Path) -> GithubResult<()> {
    let new_directory = fs::symlink_metadata(path).is_err();
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(err("Unsafe symlinked backup directory"));
    }
    fs::create_dir_all(path)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if new_directory { fs::set_permissions(path, fs::Permissions::from_mode(0o700))?; }
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err(err("Backup directories must have 0700 permissions"));
        }
    }
    Ok(())
}
fn run_capture(program: &str, args: &[&str], cwd: Option<&Path>) -> GithubResult<Vec<u8>> {
    if !["gh", "git"].contains(&program) { return Err(err("Unauthorized helper executable")); }
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd { command.current_dir(cwd); }
    let output = command.output()?;
    if !output.status.success() {
        // gh can include credentials or private information in stderr; never
        // include it in user-visible errors.
        return Err(err("Authenticated GitHub/Git command failed"));
    }
    if output.stdout.len() > 32 * 1024 * 1024 {
        return Err(err("GitHub API JSON exceeded response limit"));
    }
    Ok(output.stdout)
}
fn api(endpoint: &str) -> GithubResult<Value> {
    if !endpoint.starts_with('/') || endpoint.contains('\n') ||
       endpoint.contains('\r') || endpoint.contains('\\') ||
       endpoint.len() > 4096 {
        return Err(err("Invalid GitHub API path"));
    }
    Ok(serde_json::from_slice(&run_capture("gh", &["api", endpoint], None)?)?)
}
fn pages(endpoint: &str, collection: Option<&str>, max: usize) -> GithubResult<Vec<Value>> {
    let mut all = Vec::new();
    for page in 1..=500 {
        let separator = if endpoint.contains('?') { '&' } else { '?' };
        let url = format!("{endpoint}{separator}per_page=100&page={page}");
        let response = api(&url)?;
        let values = match collection {
            Some(name) => response[name].as_array().ok_or_else(|| err("GitHub API collection missing"))?,
            None => response.as_array().ok_or_else(|| err("GitHub API array missing"))?,
        };
        if all.len().saturating_add(values.len()) > max {
            return Err(err("GitHub resource enumeration exceeded safety limit"));
        }
        all.extend(values.iter().cloned());
        if values.len() < 100 { break; }
    }
    Ok(all)
}
fn json_file(path: &Path, data: &Value) -> GithubResult<()> {
    let parent = path.parent().ok_or_else(|| err("No metadata parent directory"))?;
    private_dir(parent)?;
    let mut file = File::create(path)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer_pretty(&mut file, data)?;
    writeln!(file)?;
    file.sync_all()?;
    Ok(())
}
/// Reusing an immutable encrypted archive is safe only after authenticating
/// its local SHA-256 sidecar. Never silently skip an existing corrupt shard.
fn verify_existing_archive(path: &Path) -> GithubResult<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || !path.to_string_lossy().ends_with(".tar.gz.age") {
        return Err(err("Unsafe encrypted Git snapshot"));
    }
    let sidecar = std::path::PathBuf::from(format!("{}.sha256", path.display()));
    if !fs::symlink_metadata(&sidecar)?.file_type().is_file() {
        return Err(err("Missing or symlinked encrypted archive checksum"));
    }
    let reference = fs::read_to_string(&sidecar)?;
    let expected = reference.split_whitespace().next()
        .ok_or_else(|| err("Missing encrypted archive SHA-256"))?;
    if expected.len() != 64 || !expected.bytes().all(|b|b.is_ascii_hexdigit()) {
        return Err(err("Malformed encrypted archive checksum"));
    }
    let mut stream = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = stream.read(&mut bytes)?;
        if count == 0 { break; }
        hash.update(&bytes[..count]);
    }
    let actual = format!("{:x}", hash.finalize());
    if actual != expected.to_ascii_lowercase() {
        return Err(err("Encrypted Git snapshot SHA-256 mismatch"));
    }
    Ok(())
}

fn checked_clone(slug: &str, into: &Path) -> GithubResult<()> {
    safe_slug(slug)?;
    if into.exists() { return Err(err("Git mirror staging directory already exists")); }
    let target = into.to_str().ok_or_else(|| err("Non-UTF8 mirror staging path"))?;
    run_capture("gh", &["repo", "clone", slug, target, "--", "--mirror"], None)?;
    run_capture("git", &["-C", target, "fsck", "--full"], None)?;
    // Git LFS content is not included by a plain --mirror clone. Fetch all
    // objects while online, or fail rather than producing an incomplete mirror.
    let status = Command::new("git").args(["-C", target, "lfs", "fetch", "--all"])
        .stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    if !status.success() { return Err(err("Unable to fetch all Git LFS objects")); }
    Ok(())
}
fn fingerprint(mirror: &Path) -> GithubResult<String> {
    let target = mirror.to_str().ok_or_else(|| err("Git path not UTF8"))?;
    let refs = run_capture("git", &["-C", target, "for-each-ref",
        "--format=%(refname):%(objectname)"], None)?;
    let head = run_capture("git", &["-C", target, "symbolic-ref", "HEAD"], None)
        .unwrap_or_default();
    let mut digest = Sha256::new();
    digest.update(&refs);
    digest.update(&head);
    Ok(format!("{:x}", digest.finalize()))
}
fn metadata_for(slug: &str, root: &Path, max_runs: usize) -> GithubResult<Value> {
    safe_slug(slug)?;
    let base = root.join("repositories").join(slug);
    private_dir(&base)?;
    let repo = api(&format!("/repos/{slug}"))?;
    json_file(&base.join("repository.json"), &repo)?;
    let resources: &[(&str, &str)] = &[
        ("branches","branches"),("tags","tags"),
        ("releases","releases"),("issues","issues"),
        ("pulls","pulls"),("actions/workflows","workflows"),
        ("actions/secrets","secrets_METADATA_ONLY"),
        ("actions/variables","variables_METADATA_ONLY"),
    ];
    let mut status = Vec::new();
    for (endpoint, name) in resources {
        // Only read-only metadata endpoints. A 403/404 is recorded as an
        // explicit omission, not falsified as an empty successful collection.
        match pages(&format!("/repos/{slug}/{endpoint}"),
                    if *endpoint=="actions/workflows" {Some("workflows")}
                    else if *endpoint=="actions/secrets" {Some("secrets")}
                    else if *endpoint=="actions/variables" {Some("variables")}
                    else {None}, 100_000) {
            Ok(items) => {
                json_file(&base.join(format!("{name}.json")), &json!(items))?;
                status.push(json!({"resource":name,"ok":true,"count":items.len()}));
            }
            Err(_) => status.push(json!({"resource":name,"ok":false,"reason":"API read failed"})),
        }
    }
    let candidates = pages(&format!("/repos/{slug}/actions/runs"), Some("workflow_runs"), 100_000)?;
    json_file(&base.join("ci-runs-candidates.json"), &json!(candidates))?;
    let ci_dir = base.join("ci").join("runs");
    private_dir(&ci_dir)?;
    let mut archived = 0usize;
    let mut rejected = Vec::new();
    let target = if max_runs == 0 { candidates.len() } else { max_runs.min(candidates.len()) };
    for run in &candidates {
        if archived >= target { break; }
        if run["status"] != "completed" || run["conclusion"] != "success" {
            continue;
        }
        let Some(id) = run["id"].as_u64() else { continue };
        let where_to = ci_dir.join(id.to_string());
        private_dir(&where_to)?;
        json_file(&where_to.join("run.json"), run)?;
        // CI download is conservative: never count a run as complete when its
        // declared artifacts cannot be downloaded. An older run backfills it.
        let artifacts = api(&format!(
            "/repos/{slug}/actions/runs/{id}/artifacts?per_page=100"))?;
        let artifact_count = artifacts["total_count"].as_u64()
            .ok_or_else(|| err("Actions artifact inventory missing total_count"))?;
        if artifact_count > 0 {
            let directory = where_to.join("artifacts");
            private_dir(&directory)?;
            let Some(dest) = directory.to_str() else { return Err(err("Invalid CI artifact staging path")); };
            if run_capture("gh", &["run", "download", &id.to_string(),
                             "-R", slug, "--dir", dest], None).is_err() {
                rejected.push(id);
                continue;
            }
        }
        // Store run metadata for all successful candidates. Logs are
        // retrieved by a dedicated read-only GitHub API endpoint.
        // Archive logs directly to disk; never collect arbitrarily large
        // Actions log ZIPs in Rust process memory.
        let path = where_to.join("logs.zip");
        let sink = File::create(&path)?;
        let status = Command::new("gh")
            .args(["api", &format!("/repos/{slug}/actions/runs/{id}/logs")])
            .stdout(Stdio::from(sink)).stderr(Stdio::null()).status();
        if !matches!(status, Ok(s) if s.success()) ||
           fs::metadata(&path)?.len() == 0 {
            let _ = fs::remove_file(path);
            rejected.push(id);
            continue;
        }
        archived += 1;
    }
    if archived < target {
        return Err(err("CI backup shortfall: insufficient verified successful runs"));
    }
    let summary = json!({
        "backend":"github", "repository":slug,
        "ci_target":target, "ci_complete":archived, "ci_rejected":rejected,
        "metadata":status
    });
    json_file(&base.join("backup_status.json"), &summary)?;
    Ok(summary)
}

pub fn backup_with_progress<F: FnMut(usize, usize, &str)>(
    output: &Path, recipient: &str, resume: bool, max_runs: usize,
    mut on_progress: F,
) -> GithubResult<Value> {
    if !recipient.starts_with("age1") || recipient.len() < 24 {
        return Err(err("A valid public age recipient is required"));
    }
    private_dir(output)?;
    // Rust's own schema-1 checkpoint remains fully readable by the Python
    // journal auditor. The native config is explicit, so do not accidentally
    // adopt an unrelated prior Python job with different backup switches.
    let config = json!({
        "recipient":recipient, "source":"github-native", "max_runs_per_repo":max_runs
    });
    let mut session = journal::Journal::start(&output.join(".state"), &config,
                                             !resume, resume)?;
    let user = api("/user")?;
    let login = user["login"].as_str().ok_or_else(|| err("GitHub authenticated login missing"))?;
    if login.len() > 100 { return Err(err("Unsafe GitHub login")); }
    let repositories = pages("/user/repos?affiliation=owner,collaborator,organization_member",
                             None, 5000)?;
    if repositories.is_empty() { return Err(err("No GitHub repositories available")); }
    let mut outcomes = Vec::new();
    let cycle = session.cycle()?.to_owned();
    let total = repositories.len();
    on_progress(0, total, "Repository GitHub enumerati");
    for (index, repo) in repositories.into_iter().enumerate() {
        let slug = repo["full_name"].as_str().ok_or_else(|| err("GitHub repository full_name missing"))?;
        safe_slug(slug)?;
        let step = format!("repo:{slug}:native");
        if session.done(&step) {
            outcomes.push(json!({"repository":slug,"resumed":true}));
            on_progress(index + 1, total, "Checkpoint repository riutilizzato");
            continue;
        }
        let stage = TempDir::new_in(output)?;
        let mirror = stage.path().join("git").join(format!("{}.git", slug.split('/').last().unwrap()));
        private_dir(mirror.parent().ok_or_else(|| err("Missing Git mirror parent"))?)?;
        checked_clone(slug, &mirror)?;
        let fp = fingerprint(&mirror)?;
        let permanent = output.join("repositories").join(slug).join("native-snapshots");
        private_dir(&permanent)?;
        let archive_name = format!("{fp}.tar.gz.age");
        let dest = permanent.join(archive_name);
        if !dest.exists() {
            archive::create(&stage.path().join("git"), &dest, recipient,
                &format!("multi-backup/repositories/{slug}/git"))?;
        }
        verify_existing_archive(&dest)?;
        let meta_stage = TempDir::new_in(output)?;
        let info = metadata_for(slug, meta_stage.path(), max_runs)?;
        let info_folder = meta_stage.path().join("repositories").join(slug);
        let meta_dest = output.join("metadata").join("cycles").join(&cycle)
            .join("repositories").join(slug).join("native-metadata.tar.gz.age");
        private_dir(meta_dest.parent().ok_or_else(|| err("Missing metadata destination parent"))?)?;
        if !meta_dest.exists() {
            archive::create(&info_folder, &meta_dest, recipient,
                &format!("multi-backup/repositories/{slug}"))?;
        }
        verify_existing_archive(&meta_dest)?;
        session.record(&step, &meta_dest)?;
        outcomes.push(json!({"repository":slug, "fingerprint":fp, "metadata":info}));
        on_progress(index + 1, total, "Mirror e metadati cifrati");
    }
    let result = json!({
        "schema":1, "backend":"github", "cycle":cycle, "account":login,
        "repositories":outcomes,
        // Optional read-denied resources are archived as omissions, not
        // fabricated as complete metadata. The native GUI surfaces them.
        "complete":outcomes.iter().all(|repo: &Value| {
            repo["resumed"] == true ||
            repo["metadata"]["metadata"].as_array().is_some_and(|items|
                items.iter().all(|item| item["ok"] == true))
        }),
        "remote_cleanup":"disabled"
    });
    let summary_dir = output.join("metadata").join("cycles").join(&cycle);
    private_dir(&summary_dir)?;
    json_file(&summary_dir.join("native-github-summary.json"), &result)?;
    session.record("native-summary",&summary_dir.join("native-github-summary.json"))?;
    session.finish()?;
    Ok(result)
}
pub fn backup(output: &Path, recipient: &str, resume: bool, max_runs: usize)
    -> GithubResult<Value> {
    backup_with_progress(output, recipient, resume, max_runs, |_,_,_|{})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rejects_unsafe_repo_slugs() {
        for value in ["../root","foo/../bar","foo","a/b/c","owner/\\bad","a/.","a/.."] {
            assert!(safe_slug(value).is_err(), "{value}");
        }
        assert_eq!(safe_slug("team/repo.name").unwrap(), ("team","repo.name"));
    }
    #[test] fn corrupt_or_unverified_existing_archive_never_reused() {
        let dir=tempfile::tempdir().unwrap();
        let item=dir.path().join("one.tar.gz.age");
        fs::write(&item, b"encrypted").unwrap();
        assert!(verify_existing_archive(&item).is_err());
        let checksum=std::path::PathBuf::from(format!("{}.sha256",item.display()));
        fs::write(&checksum, format!("{:x}  one.tar.gz.age\n",
            Sha256::digest(b"encrypted"))).unwrap();
        assert!(verify_existing_archive(&item).is_ok());
        fs::write(&item, b"modified!").unwrap();
        assert!(verify_existing_archive(&item).is_err());
    }
    #[test] fn disallows_shell_program_execution() {
        assert!(run_capture("sh", &["-c","echo unsafe"], None).is_err());
        assert!(run_capture("bash", &["-c","echo unsafe"], None).is_err());
    }
}
