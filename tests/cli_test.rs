//! End-to-end CLI smoke tests. These run the compiled binary in `--dry-run
//! --non-interactive` mode against the shipped example config, asserting the
//! full pipeline executes without touching the system.

use assert_cmd::Command;
use predicates::prelude::*;

const EXAMPLE_CONFIG: &str = "unit3d-installer.example.toml";

/// Strip ANSI SGR escape sequences so assertions work regardless of the
/// owo-colors TTY detection behavior.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn dry_run_stdout() -> String {
    let mut cmd = Command::cargo_bin("unit3d-installer").unwrap();
    let out = cmd
        .args(["--non-interactive", "--dry-run", "--config", EXAMPLE_CONFIG])
        .output()
        .unwrap();
    assert!(out.status.success(), "dry-run must succeed");
    strip_ansi(&String::from_utf8_lossy(&out.stdout))
}

#[test]
fn dry_run_completes_all_steps() {
    let stdout = dry_run_stdout();
    assert!(stdout.contains("UNIT3D Installation Complete!"));
}

#[test]
fn dry_run_emits_step_headers_and_commands() {
    let stdout = dry_run_stdout();

    // Every step header appears.
    for header in [
        "Validating Installer Policies",
        "Redis Setup & Configurations",
        "Prerequisites",
        "Database",
        "PHP",
        "Nginx Setup & Configurations",
        "UNIT3D",
        "Meilisearch",
        "Credentials",
    ] {
        assert!(stdout.contains(header), "missing step header: {header}");
    }

    // Dry-run prints each command prefixed with `$ `.
    assert!(stdout.contains("$ apt-get install -y"));
    assert!(stdout.contains("$ systemctl restart redis-server"));
    assert!(stdout.contains("$ certbot"));
    assert!(stdout.contains("php artisan key:generate"));
}

#[test]
fn missing_config_is_an_error() {
    let mut cmd = Command::cargo_bin("unit3d-installer").unwrap();
    cmd.args([
        "--non-interactive",
        "--dry-run",
        "--config",
        "/nonexistent/config.toml",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("failed to load configuration"));
}

#[test]
fn empty_config_file_is_refused() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "# nothing here\n\n").unwrap();

    let mut cmd = Command::cargo_bin("unit3d-installer").unwrap();
    cmd.args(["--non-interactive", "--dry-run", "--config"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("is empty").and(predicate::str::contains("refusing to run")),
        );
}

#[test]
fn help_flag_prints_usage() {
    let mut cmd = Command::cargo_bin("unit3d-installer").unwrap();
    cmd.arg("--help").assert().success().stdout(
        predicate::str::contains("--config")
            .and(predicate::str::contains("--dry-run"))
            .and(predicate::str::contains("--non-interactive")),
    );
}

#[test]
fn version_flag_prints_version() {
    let mut cmd = Command::cargo_bin("unit3d-installer").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn yes_to_all_alias_works_in_dry_run() {
    let mut cmd = Command::cargo_bin("unit3d-installer").unwrap();
    let out = cmd
        .args(["--yes-to-all", "--dry-run", "--config", EXAMPLE_CONFIG])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = strip_ansi(&String::from_utf8_lossy(&out.stdout));
    assert!(stdout.contains("UNIT3D Installation Complete!"));
}

#[test]
fn dry_run_uses_node_24_and_echo_port() {
    let stdout = dry_run_stdout();
    assert!(stdout.contains("setup_24.x"));
    assert!(stdout.contains("ufw allow 22"));
    assert!(stdout.contains("ufw allow 8443"));
    assert!(stdout.contains("php artisan scout:sync-index-settings"));
    assert!(stdout.contains("sudo -u www-data bash"));
}

#[test]
fn invalid_flag_is_rejected() {
    let mut cmd = Command::cargo_bin("unit3d-installer").unwrap();
    cmd.arg("--definitely-not-a-flag")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn dry_run_prints_credentials_block() {
    let stdout = dry_run_stdout();
    assert!(stdout.contains("UNIT3D Installation Credentials"));
    assert!(stdout.contains("URL:"));
    assert!(stdout.contains("OWNER LOGIN:"));
    assert!(stdout.contains("DATABASE:"));
    assert!(stdout.contains("MEILISEARCH:"));
    assert!(stdout.contains("KEEP THIS FILE SECURE"));
}

#[test]
fn dry_run_writes_no_files_to_cwd() {
    // The dry-run must not create anything under the current directory.
    let before: Vec<_> = std::fs::read_dir(".")
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let _ = dry_run_stdout();
    let after: Vec<_> = std::fs::read_dir(".")
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(before.len(), after.len());
}
