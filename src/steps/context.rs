//! Cross-cutting state threaded through every step: the loaded [`Config`],
//! the I/O [`Prompter`] + [`Style`], and the [`Exec`] implementation
//! (real or dry-run).

use crate::cli::Args;
use crate::config::Config;
use crate::io::{Prompter, Style};
use crate::process::{DryExec, Exec, RealExec};
use anyhow::{Context as _, Result};
use std::path::PathBuf;
use std::sync::Arc;

use super::Step;

pub struct Context {
    pub config: Config,
    pub prompter: Prompter,
    pub style: Style,
    pub exec: Arc<dyn Exec>,
    pub dry_run: bool,
    pub non_interactive: bool,
    /// Path the config was loaded from (kept for diagnostics + future
    /// `--write-back` support).
    #[allow(dead_code)]
    pub config_path: Option<PathBuf>,
}

impl Context {
    /// Build the initial context from CLI args.
    pub fn build(args: &Args) -> Result<Self> {
        let config =
            Config::load(args.config.as_deref()).context("failed to load configuration")?;
        let exec: Arc<dyn Exec> = if args.dry_run {
            Arc::new(DryExec)
        } else {
            Arc::new(RealExec)
        };
        Ok(Self {
            config,
            prompter: Prompter::new(args.non_interactive),
            style: Style,
            exec,
            dry_run: args.dry_run,
            non_interactive: args.non_interactive,
            config_path: args.config.clone(),
        })
    }

    /// Helper: run a shell command via the configured executor.
    pub fn run(&self, cmd: &str) -> Result<()> {
        self.exec.run(cmd).map(|_| ())
    }

    /// Helper: run multiple shell commands in order.
    pub fn run_all(&self, cmds: impl IntoIterator<Item = String>) -> Result<()> {
        for cmd in cmds {
            self.exec.run(&cmd)?;
        }
        Ok(())
    }

    /// Helper: write a file, but in `--dry-run` mode just print the
    /// intended contents to stdout instead of touching the filesystem.
    /// Written atomically (temp + rename) and refusing to follow symlinks,
    /// so a pre-planted symlink at a system path can't be turned into a
    /// root write primitive.
    pub fn write_file(&self, path: &std::path::Path, contents: &str) -> Result<()> {
        if self.dry_run {
            println!("# >>> write {}", path.display());
            println!("{contents}");
            println!("# <<< end {}", path.display());
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic(path, contents)
    }

    /// Write a file containing secrets (`.env`, `/root/.my.cnf`,
    /// credentials ledger) with mode 0600, atomically (temp file + rename)
    /// so no world-readable intermediate state ever exists on disk.
    pub fn write_secret_file(&self, path: &std::path::Path, contents: &str) -> Result<()> {
        if self.dry_run {
            println!("# >>> write (0600) {}", path.display());
            println!("{contents}");
            println!("# <<< end {}", path.display());
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic_0600(path, contents)
    }
}

/// Write a temp file with the given mode from the start (never a
/// world-readable window), refusing to follow a pre-existing symlink, then
/// atomically rename it over `path`.
#[cfg(unix)]
fn write_atomic_with_mode(path: &std::path::Path, contents: &str, mode: u32) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let tmp = temp_file_for(path);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)?;
    f.write_all(contents.as_bytes())?;
    f.flush()?;
    // fsync the data so a crash right after rename can't leave an empty
    // file where the real one used to be.
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_atomic_with_mode(path: &std::path::Path, contents: &str, _mode: u32) -> Result<()> {
    let tmp = temp_file_for(path);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Atomic write for ordinary (non-secret) config files: 0644.
fn write_atomic(path: &std::path::Path, contents: &str) -> Result<()> {
    write_atomic_with_mode(path, contents, 0o644)
}

/// Atomic write for secret files: 0600.
fn write_atomic_0600(path: &std::path::Path, contents: &str) -> Result<()> {
    write_atomic_with_mode(path, contents, 0o600)
}

/// Derive a unique sibling temp path for atomic writes. Includes the PID and
/// a process-wide counter so two concurrent writers (or a leftover temp file
/// from a crashed run) never collide with `create_new`.
fn temp_file_for(path: &std::path::Path) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unit3d-tmp".to_string());
    let dir = path.parent().unwrap_or(std::path::Path::new("/tmp"));
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{name}.{pid}.{n}.tmp"))
}

/// Catalog of every install step, in execution order. This is the Rust
/// equivalent of `InstallCommand::$steps`.
pub struct Steps;

impl Steps {
    pub fn ordered() -> Vec<Box<dyn Step>> {
        vec![
            Box::new(super::server::ServerSetupStep),
            Box::new(super::prerequisites::PrerequisitesStep),
            Box::new(super::policies::PoliciesStep),
            Box::new(super::redis::RedisSetupStep),
            Box::new(super::database::DatabaseStep),
            Box::new(super::php::PhpSetupStep),
            Box::new(super::nginx::NginxSetupStep),
            Box::new(super::unit3d::Unit3dSetupStep),
            Box::new(super::meilisearch::MeilisearchSetupStep),
            Box::new(super::credentials::CredentialsStep),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Args;
    use clap::Parser;
    use tempfile::tempdir;

    #[test]
    fn build_selects_dry_exec_when_dry_run() {
        let args = Args::parse_from(["unit3d-installer", "--dry-run", "--non-interactive"]);
        let ctx = Context::build(&args).unwrap();
        assert!(ctx.dry_run);
        assert!(ctx.non_interactive);
        // DryExec must succeed without touching the system.
        ctx.run("echo hello").unwrap();
        ctx.run_all(["echo a".to_string(), "echo b".to_string()])
            .unwrap();
    }

    #[test]
    fn build_uses_real_exec_by_default() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let ctx = Context::build(&args).unwrap();
        assert!(!ctx.dry_run);
        // RealExec runs `true` fine.
        ctx.run("true").unwrap();
    }

    #[test]
    fn write_file_dry_run_does_not_touch_disk() {
        let args = Args::parse_from(["unit3d-installer", "--dry-run"]);
        let ctx = Context::build(&args).unwrap();
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("nested/deep/out.txt");
        ctx.write_file(&target, "contents").unwrap();
        // Parent directories must NOT have been created in dry-run mode.
        assert!(!tmp.path().join("nested").exists());
    }

    #[test]
    fn write_file_creates_parent_dirs() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let ctx = Context::build(&args).unwrap();
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("a/b/c.txt");
        ctx.write_file(&target, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn write_file_overwrites_existing() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let ctx = Context::build(&args).unwrap();
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("f.txt");
        std::fs::write(&target, "old").unwrap();
        ctx.write_file(&target, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn run_all_short_circuits_on_failure() {
        struct Boom;
        impl Exec for Boom {
            fn run(&self, cmd: &str) -> Result<std::process::Output> {
                if cmd == "fail" {
                    anyhow::bail!("boom");
                }
                Ok(std::process::Output {
                    status: std::os::unix::process::ExitStatusExt::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
        }
        let ctx = Context {
            config: Config::default(),
            prompter: Prompter::new(true),
            style: Style,
            exec: Arc::new(Boom),
            dry_run: false,
            non_interactive: true,
            config_path: None,
        };
        let res = ctx.run_all(["ok".to_string(), "fail".to_string(), "never".to_string()]);
        assert!(res.is_err());
    }

    #[test]
    fn config_path_is_forwarded() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("cfg.toml");
        std::fs::write(&cfg_path, "[app]\nhostname = \"x.com\"\n").unwrap();
        let args = Args::parse_from([
            "unit3d-installer",
            "--config",
            cfg_path.to_str().unwrap(),
            "--dry-run",
        ]);
        let ctx = Context::build(&args).unwrap();
        assert_eq!(ctx.config_path.as_deref(), Some(cfg_path.as_path()));
        assert_eq!(ctx.config.app.hostname, "x.com");
    }

    #[test]
    fn write_secret_file_sets_0600_and_is_atomic() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let ctx = Context::build(&args).unwrap();
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("secret.txt");
        ctx.write_secret_file(&target, "s3cret").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "s3cret");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "secret file must be 0600");
        }
        // No leftover temp files next to the target.
        let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(leftovers, vec!["secret.txt"]);
    }

    #[test]
    fn write_secret_file_overwrites_with_new_perms() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let ctx = Context::build(&args).unwrap();
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("s.txt");
        std::fs::write(&target, "old").unwrap();
        ctx.write_secret_file(&target, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn write_secret_file_dry_run_touches_nothing() {
        let args = Args::parse_from(["unit3d-installer", "--dry-run"]);
        let ctx = Context::build(&args).unwrap();
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("nested/secret.txt");
        ctx.write_secret_file(&target, "x").unwrap();
        assert!(!target.exists());
        assert!(!tmp.path().join("nested").exists());
    }

    #[test]
    fn write_file_does_not_follow_symlink() {
        #[cfg(unix)]
        {
            let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
            let ctx = Context::build(&args).unwrap();
            let tmp = tempdir().unwrap();
            let victim = tmp.path().join("victim.txt");
            std::fs::write(&victim, "do-not-touch").unwrap();
            let target = tmp.path().join("config.txt");
            // Attacker plants a symlink at the config path pointing at the
            // victim.
            std::os::unix::fs::symlink(&victim, &target).unwrap();
            // write_file must replace the symlink, not follow it.
            ctx.write_file(&target, "new").unwrap();
            assert_eq!(std::fs::read_to_string(&victim).unwrap(), "do-not-touch");
            assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
            // The target is now a regular file, not a symlink.
            assert!(!target.is_symlink());
        }
    }

    #[test]
    fn write_secret_file_does_not_follow_symlink() {
        #[cfg(unix)]
        {
            let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
            let ctx = Context::build(&args).unwrap();
            let tmp = tempdir().unwrap();
            let victim = tmp.path().join("victim.txt");
            std::fs::write(&victim, "do-not-touch").unwrap();
            let target = tmp.path().join("secret.conf");
            std::os::unix::fs::symlink(&victim, &target).unwrap();
            ctx.write_secret_file(&target, "s3cret").unwrap();
            assert_eq!(std::fs::read_to_string(&victim).unwrap(), "do-not-touch");
            assert_eq!(std::fs::read_to_string(&target).unwrap(), "s3cret");
        }
    }

    #[test]
    fn temp_file_names_are_unique_per_call() {
        let a = temp_file_for(std::path::Path::new("/var/www/html/.env"));
        let b = temp_file_for(std::path::Path::new("/var/www/html/.env"));
        assert_ne!(a, b);
        assert!(a.starts_with("/var/www/html"));
        // Filename must not collide with the real .env.
        assert!(!a.ends_with(".env"));
    }

    #[test]
    fn write_file_default_perms_0644() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
            let ctx = Context::build(&args).unwrap();
            let tmp = tempdir().unwrap();
            let target = tmp.path().join("nginx-site.txt");
            ctx.write_file(&target, "server { }").unwrap();
            let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
        }
    }
}
