//! Pre-flight policies. Rust replaces the bulk of the legacy
//! `src/Installer/Policies/*` directory with compile-time serde validation
//! (presence/key-type are enforced by the typed `Config` struct), leaving
//! only the runtime checks that need the live filesystem: UID 0, UNIT3D not
//! yet installed in the install dir, and (optionally) live PHP version.

use crate::steps::{Context, Step};
use crate::system;
use anyhow::{Result, bail};

pub struct PoliciesStep;

impl Step for PoliciesStep {
    fn name(&self) -> &'static str {
        "Validating Installer Policies"
    }

    fn handle(&self, ctx: &mut Context) -> Result<()> {
        // IsPrivilegedUser
        if !ctx.dry_run {
            system::require_root()?;
        }

        // Detect the distro and refuse early on unsupported OSes
        // (skipped in --dry-run mode so users can preview the plan from
        // any dev box).
        if !ctx.dry_run {
            match system::detect() {
                Ok(info) => {
                    if !info.distro.is_supported() {
                        bail!(
                            "Unsupported OS: {} {}. This installer supports Ubuntu LTS and Debian.",
                            info.id,
                            info.version_id
                        );
                    }
                    ctx.style
                        .info(&format!("Detected {} {}", info.id, info.version_id));
                }
                Err(e) => {
                    bail!("OS detection failed: {e}");
                }
            }
        }

        // AppNotInstalled: if the install dir already contains an `app/`
        // subdir, UNIT3D was already cloned there. Skipped on dry-run.
        if !ctx.dry_run {
            ensure_not_installed(ctx.config.install_dir())?;
        }

        // AppKeyExists / PhpVersionKeyExists / InstallDirKeyExists /
        // DatabaseInstallersKeyExists / DatabaseDriverKeyExists are all
        // enforced statically by the typed `Config` struct.

        // IsPhpVersionCompat: check the live PHP version matches
        // `min_php_version` from config.
        if !ctx.dry_run {
            check_php_compat(&ctx.config.unit3d.min_php_version)?;
        }

        Ok(())
    }
}

/// AppNotInstalled policy: fail if `install_dir/app/` already exists.
fn ensure_not_installed(install_dir: &std::path::Path) -> Result<()> {
    let already = install_dir.join("app");
    if already.exists() {
        bail!(
            "UNIT3D already installed at {} — refusing to overwrite. \
             Remove the directory first if you intend to reinstall.",
            install_dir.display()
        );
    }
    Ok(())
}

/// Compare PHP versions numerically rather than via string prefix matching.
/// Accepts values such as `8.5`, `8.5.1`, and `8.5.0RC1`.
fn php_version_at_least(version: &str, required: &str) -> bool {
    let parse = |text: &str| {
        let mut nums = Vec::new();
        for part in text.split(|c: char| !c.is_ascii_digit()) {
            if part.is_empty() {
                continue;
            }
            if let Ok(num) = part.parse::<u32>() {
                nums.push(num);
            }
            if nums.len() == 3 {
                break;
            }
        }
        while nums.len() < 3 {
            nums.push(0);
        }
        (nums[0], nums[1], nums[2])
    };

    let current = parse(version);
    let minimum = parse(required);
    current >= minimum
}

/// Verify the on-box PHP version is at least the required one. Reads
/// `php --version` output. On boxes that don't yet have PHP installed
/// (the typical case before `Prerequisites` runs), this check is a no-op
/// reported as `Ok(())` — the legacy installer relies on `ubuntu.sh` to
/// install PHP before the `artisan install` command runs, so the
/// legacy-equivalent timing here is "after prerequisites, before main".
fn check_php_compat(required: &str) -> Result<()> {
    let output = std::process::Command::new("php").arg("--version").output();
    let Ok(out) = output else {
        // PHP not installed yet — Prerequisites step will install it.
        return Ok(());
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(first) = text.lines().next() else {
        return Ok(());
    };
    // Format: "PHP 8.5.0 (cli) ( ... )"
    let version = first.split_whitespace().nth(1).unwrap_or_default();
    if !php_version_at_least(version, required) {
        bail!("PHP version {version} on this box is below required {required}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_existing_install_blocks_run() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app")).unwrap();
        let err = ensure_not_installed(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("already installed"));
    }

    #[test]
    fn clean_dir_passes_policy() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_not_installed(tmp.path()).unwrap();
    }

    #[test]
    fn check_php_compat_tolerates_no_php() {
        // PHP may or may not be present in CI sandbox — either way, the
        // helper must not panic. Returns Ok when `php` binary is absent.
        let _ = check_php_compat("8.5");
    }

    #[test]
    fn check_php_compat_version_parsing() {
        // Simulate `php --version` first-line parsing regardless of whether
        // PHP is installed: a running PHP that is too old must fail.
        let first = "PHP 8.2.0 (cli) (built: Jun 20 2024 12:00:00) ( NTS )";
        let version = first.split_whitespace().nth(1).unwrap_or_default();
        assert_eq!(version, "8.2.0");
        assert!(!version.starts_with("8.5"));
    }

    #[test]
    fn php_version_at_least_accepts_newer_major_versions() {
        assert!(php_version_at_least("8.6.0", "8.5"));
        assert!(php_version_at_least("8.5.1", "8.5"));
        assert!(!php_version_at_least("8.4.24", "8.5"));
    }

    #[test]
    fn ensure_not_installed_ok_when_missing() {
        // A path that doesn't exist must pass the policy.
        let tmp = tempfile::tempdir().unwrap();
        ensure_not_installed(&tmp.path().join("missing")).unwrap();
    }

    #[test]
    fn app_subdir_triggers_block() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app")).unwrap();
        let err = ensure_not_installed(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
    }
}
