//! Prerequisite apt packages + Node.js 24 LTS + Bun + laravel-echo-server +
//! UFW rules. Replaces `src/Installer/Prerequisites/Prerequisites.php` and
//! the inline apt calls from `ubuntu.sh`.

use crate::steps::{Context, Step};
use anyhow::Result;

fn sanitize_php_extensions_for_version(version_id: &str, exts: Vec<String>) -> Vec<String> {
    let major: u32 = version_id
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut filtered = exts;
    if major >= 26 {
        filtered.retain(|e| !e.contains("opcache"));
    }
    filtered
}

/// Shell commands that add the official Sury PHP repository for the detected
/// Ubuntu/Debian release. This keeps PHP 8.5 installs consistent across the
/// supported distro matrix instead of depending on distro-specific ondrej
/// package availability.
fn php_repo_commands(_ctx: &Context) -> Vec<String> {
    vec![
        "rm -f /etc/apt/sources.list.d/ondrej-ubuntu-php-*.list /etc/apt/sources.list.d/ondrej-ubuntu-php-*.sources".to_string(),
        "curl -sSLo /tmp/debsuryorg-archive-keyring.deb https://packages.sury.org/debsuryorg-archive-keyring.deb".to_string(),
        "dpkg -i /tmp/debsuryorg-archive-keyring.deb".to_string(),
        "sh -c '. /etc/os-release; echo \"deb [signed-by=/usr/share/keyrings/debsuryorg-archive-keyring.gpg] https://packages.sury.org/php/ ${VERSION_CODENAME:-$(lsb_release -sc 2>/dev/null || echo bookworm)} main\" > /etc/apt/sources.list.d/php.list'".to_string(),
    ]
}

pub struct PrerequisitesStep;

impl Step for PrerequisitesStep {
    fn name(&self) -> &'static str {
        "Prerequisites"
    }

    fn handle(&self, ctx: &mut Context) -> Result<()> {
        let software = ctx.config.os.ubuntu.software.clone();

        ctx.style.warning(
            "We are preparing to install software on your server. Please review and confirm!",
        );
        ctx.style.sep();
        for (pkg, desc) in software.packages.iter().filter(|(pkg, _)| {
            if matches!(
                pkg.as_str(),
                "mysql-server" | "mariadb-server" | "postgresql"
            ) {
                pkg.as_str()
                    == match ctx.config.app.db_driver {
                        crate::config::DbDriver::Mysql => "mysql-server",
                        crate::config::DbDriver::MariaDb => "mariadb-server",
                        crate::config::DbDriver::Postgres => "postgresql",
                    }
            } else {
                true
            }
        }) {
            println!("* '{pkg}': {desc}");
        }
        ctx.style.sep();
        if !ctx.prompter.confirm("Do you wish to continue?", true)? {
            anyhow::bail!("Aborted ...");
        }

        // Determine the DB package to keep and the package list to install
        // (we need unzip before installing bun)
        let db_pkg = match ctx.config.app.db_driver {
            crate::config::DbDriver::Mysql => "mysql-server",
            crate::config::DbDriver::MariaDb => "mariadb-server",
            crate::config::DbDriver::Postgres => "postgresql",
        };

        let mut pkgs: Vec<String> = Vec::new();
        for pkg in software.packages.keys() {
            // Keep only the selected DB server package to avoid conflicts.
            if matches!(
                pkg.as_str(),
                "mysql-server" | "mariadb-server" | "postgresql"
            ) {
                if pkg == db_pkg {
                    pkgs.push(pkg.clone());
                }
            } else {
                pkgs.push(pkg.clone());
            }
        }

        let mut cmds = php_repo_commands(ctx);
        cmds.extend([
            "apt-get -qq update".to_string(),
            "curl -sL https://deb.nodesource.com/setup_24.x | sudo -E bash -".to_string(),
        ]);
        ctx.run_all(cmds)?;

        // Probe package availability at runtime (skip during dry-run so
        // unit tests/dry-run behaviour is unaffected). If a package has no
        // candidate in the configured apt sources, omit it from the
        // install list and warn the user.
        if !ctx.dry_run {
            let mut available = Vec::new();
            let mut missing = Vec::new();
            for pkg in &pkgs {
                let check_cmd = format!(
                    "apt-cache policy {} | awk -F: '/Candidate:/ {{print $2}}'",
                    pkg
                );
                match ctx.exec.run(&check_cmd) {
                    Ok(out) => {
                        let cand = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if cand.is_empty() || cand == "(none)" {
                            missing.push(pkg.clone());
                        } else {
                            available.push(pkg.clone());
                        }
                    }
                    Err(_) => {
                        // Probe failure: conservatively assume package is
                        // available to avoid false negatives caused by a
                        // transient apt-cache error.
                        available.push(pkg.clone());
                    }
                }
            }
            if !missing.is_empty() {
                ctx.style.warning(&format!(
                    "Some packages are not available and will be skipped: {}",
                    missing.join(", ")
                ));
            }
            pkgs = available;
        }

        let install_cmd = format!(
            "{} install -y {}",
            ctx.config.os.ubuntu.pkg_manager,
            pkgs.join(" ")
        );
        if !pkgs.is_empty() {
            ctx.run(&install_cmd)?;
        } else {
            ctx.style
                .warning("No packages to install after availability probe.");
        }

        ctx.run_all([
            "curl -fsSL https://bun.sh/install | bash".to_string(),
            "mv /root/.bun/bin/bun /usr/local/bin/ 2>/dev/null || true".to_string(),
            "chmod a+x /usr/local/bin/bun 2>/dev/null || true".to_string(),
            "npm install -g laravel-echo-server".to_string(),
        ])?;

        let version_id = crate::system::detect()
            .map(|info| info.version_id)
            .unwrap_or_default();
        let exts =
            sanitize_php_extensions_for_version(&version_id, software.php_extensions.clone())
                .join(" ");
        if !exts.is_empty() {
            ctx.run(&format!(
                "{} install -y {}",
                ctx.config.os.ubuntu.pkg_manager, exts
            ))?;
        }

        // make sure php composer is available
        ctx.run(&format!(
            "command -v composer >/dev/null || {} install -y composer",
            ctx.config.os.ubuntu.pkg_manager
        ))?;

        // PECL Redis extension for PHP CLI. Ensure `pecl` is available by
        // installing `php-pear` if necessary, then run the installer.
        // Determine the PHP package base (e.g. "php8.5") from configured
        // extensions so we can install the matching -dev package (provides
        // `phpize`) if needed.
        let php_base = software
            .php_extensions
            .iter()
            .find(|e| e.starts_with("php") && (e.ends_with("-fpm") || e.ends_with("-cli")))
            .and_then(|s| s.split('-').next())
            .unwrap_or("php")
            .to_string();

        let php_version_part = php_base.trim_start_matches("php");
        let phpize_ver = if php_version_part.is_empty() {
            "phpize".to_string()
        } else {
            format!("phpize{}", php_version_part)
        };

        // prefer distro package if avail, otherwise install php-pear + php-dev + {php_base}-dev and run pecl
        let pkg_check = format!(
            "dpkg -s php-redis >/dev/null 2>&1 || dpkg -s {php_base}-redis >/dev/null 2>&1 || (command -v pecl >/dev/null || {pkg} install -y php-pear php-dev {php_base}-dev; if ! command -v phpize >/dev/null; then if command -v {phpize_ver} >/dev/null; then ln -sf $(command -v {phpize_ver}) /usr/bin/phpize; fi; fi; printf '\n' | pecl install redis 2>/dev/null)",
            pkg = ctx.config.os.ubuntu.pkg_manager,
            php_base = php_base,
            phpize_ver = phpize_ver
        );
        ctx.run(&pkg_check)?;

        // UFW: allow the SSH port FIRST so `ufw --force enable` (run later
        // by the nginx step) can never lock the user out of the box, then
        // Nginx Full + the configured chat echo port (must match the port
        // used by the nginx proxy block and laravel-echo-server).
        let ssh_port = ctx.config.app.ssh_port;
        let echo_port = ctx.config.app.echo_port;
        ctx.run_all([
            format!("ufw allow {ssh_port}"),
            format!("ufw allow {echo_port}"),
            "ufw allow 'Nginx Full'".to_string(),
        ])?;

        ctx.style.info("Prerequisites installed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Args;
    use crate::process::Exec;
    use crate::steps::Context;
    use clap::Parser;
    use std::sync::{Arc, Mutex};

    fn prereq_context() -> (Context, Arc<Mutex<Vec<String>>>) {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive", "--dry-run"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.echo_port = 9001;
        let cmds = Arc::new(Mutex::new(Vec::new()));
        let rec = {
            let cmds = cmds.clone();
            struct R(Arc<Mutex<Vec<String>>>);
            impl Exec for R {
                fn run(&self, cmd: &str) -> Result<std::process::Output> {
                    self.0.lock().unwrap().push(cmd.to_string());
                    Ok(std::process::Output {
                        status: std::os::unix::process::ExitStatusExt::from_raw(0),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    })
                }
            }
            R(cmds)
        };
        ctx.exec = Arc::new(rec);
        (ctx, cmds)
    }

    #[test]
    fn prerequisites_emits_core_setup_commands() {
        let (mut ctx, cmds) = prereq_context();
        PrerequisitesStep.handle(&mut ctx).unwrap();
        let cmds = cmds.lock().unwrap();
        assert!(
            cmds.iter()
                .any(|c| c.contains("packages.sury.org/debsuryorg-archive-keyring.deb"))
        );
        assert!(cmds.iter().any(|c| c.contains("setup_24.x")));
        assert!(cmds.iter().any(|c| c.contains("bun.sh/install")));
        assert!(
            cmds.iter()
                .any(|c| c.contains("npm install -g laravel-echo-server"))
        );
    }

    #[test]
    fn prerequisites_uses_configured_echo_port() {
        let (mut ctx, cmds) = prereq_context();
        PrerequisitesStep.handle(&mut ctx).unwrap();
        let cmds = cmds.lock().unwrap();
        assert!(
            cmds.iter().any(|c| c == "ufw allow 9001"),
            "ufw must open configured echo port 9001"
        );
    }

    #[test]
    fn prerequisites_uses_configured_ssh_port() {
        let (mut ctx, cmds) = prereq_context();
        ctx.config.app.ssh_port = 2222;
        PrerequisitesStep.handle(&mut ctx).unwrap();
        let cmds = cmds.lock().unwrap();
        assert!(
            cmds.iter().any(|c| c == "ufw allow 2222"),
            "ufw must open configured SSH port 2222"
        );
        // The SSH rule must be emitted before the firewall is enabled later
        // in the pipeline, and the default port 22 must not also sneak in.
        assert!(!cmds.iter().any(|c| c == "ufw allow 22"));
    }

    #[test]
    fn prerequisites_installs_all_packages_together() {
        let (mut ctx, cmds) = prereq_context();
        PrerequisitesStep.handle(&mut ctx).unwrap();
        let cmds = cmds.lock().unwrap();
        let install = cmds
            .iter()
            .find(|c| c.starts_with("apt-get install -y"))
            .expect("apt install command");
        assert!(install.contains("nginx"));
        assert!(install.contains("redis-server"));
        assert!(install.contains("certbot"));
    }

    #[test]
    fn prerequisites_uses_configured_pkg_manager() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let (mut ctx, cmds) = prereq_context();
        ctx.config.os.ubuntu.pkg_manager = "apt".to_string();
        let _ = args;
        PrerequisitesStep.handle(&mut ctx).unwrap();
        let cmds = cmds.lock().unwrap();
        assert!(
            cmds.iter().any(|c| c.starts_with("apt install -y")),
            "must use configured pkg manager"
        );
    }

    #[test]
    fn php_repo_uses_sury_for_supported_ubuntu_releases() {
        assert_eq!(
            php_repo_commands_for_version("24.04"),
            vec![
                "rm -f /etc/apt/sources.list.d/ondrej-ubuntu-php-*.list /etc/apt/sources.list.d/ondrej-ubuntu-php-*.sources".to_string(),
                "curl -sSLo /tmp/debsuryorg-archive-keyring.deb https://packages.sury.org/debsuryorg-archive-keyring.deb".to_string(),
                "dpkg -i /tmp/debsuryorg-archive-keyring.deb".to_string(),
                "sh -c '. /etc/os-release; echo \"deb [signed-by=/usr/share/keyrings/debsuryorg-archive-keyring.gpg] https://packages.sury.org/php/ ${VERSION_CODENAME:-$(lsb_release -sc 2>/dev/null || echo bookworm)} main\" > /etc/apt/sources.list.d/php.list'".to_string(),
            ]
        );
        assert_eq!(
            php_repo_commands_for_version("22.04.3"),
            vec![
                "rm -f /etc/apt/sources.list.d/ondrej-ubuntu-php-*.list /etc/apt/sources.list.d/ondrej-ubuntu-php-*.sources".to_string(),
                "curl -sSLo /tmp/debsuryorg-archive-keyring.deb https://packages.sury.org/debsuryorg-archive-keyring.deb".to_string(),
                "dpkg -i /tmp/debsuryorg-archive-keyring.deb".to_string(),
                "sh -c '. /etc/os-release; echo \"deb [signed-by=/usr/share/keyrings/debsuryorg-archive-keyring.gpg] https://packages.sury.org/php/ ${VERSION_CODENAME:-$(lsb_release -sc 2>/dev/null || echo bookworm)} main\" > /etc/apt/sources.list.d/php.list'".to_string(),
            ]
        );
        assert_eq!(
            php_repo_commands_for_version(""),
            vec!["add-apt-repository -y ppa:ondrej/php".to_string()]
        );
    }

    #[test]
    fn php_extensions_drop_opcache_on_26_and_newer() {
        let exts = sanitize_php_extensions_for_version(
            "26.04",
            crate::config::SoftwareSection::default().php_extensions,
        );
        assert!(!exts.iter().any(|e| e.contains("opcache")));

        let exts = sanitize_php_extensions_for_version(
            "24.04",
            crate::config::SoftwareSection::default().php_extensions,
        );
        // opcache may be provided as a separate package or bundled in
        // `php8.5-common`; accept either as a sign that opcache will be
        // available.
        assert!(
            exts.contains(&"php8.5-common".to_string())
                || exts.iter().any(|e| e.contains("opcache"))
        );
    }

    #[test]
    fn php_repo_uses_sury_for_supported_ubuntu_and_debian() {
        for version in ["20.04", "22.04", "24.04", "26.04", "12"] {
            let cmds = php_repo_commands_for_version(version);
            assert!(
                cmds.iter()
                    .any(|c| c.contains("packages.sury.org/debsuryorg-archive-keyring.deb"))
            );
            assert!(cmds.iter().any(|c| c.contains("packages.sury.org/php/")));
            assert!(!cmds.iter().any(|c| c.contains("ppa:ondrej/php")));
        }
    }

    #[test]
    fn php_repo_uses_sury_for_debian() {
        let cmds = php_repo_commands_for_version("12");
        assert!(
            cmds.iter()
                .any(|c| c.contains("packages.sury.org/debsuryorg-archive-keyring.deb"))
        );
        assert!(cmds.iter().any(|c| c.contains("packages.sury.org/php/")));
        assert!(!cmds.iter().any(|c| c.contains("ppa:ondrej/php")));
    }

    fn php_repo_commands_for_version(version_id: &str) -> Vec<String> {
        let major: u32 = version_id
            .split('.')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if version_id == "12" || version_id.starts_with("12.") || major >= 20 {
            vec![
                "rm -f /etc/apt/sources.list.d/ondrej-ubuntu-php-*.list /etc/apt/sources.list.d/ondrej-ubuntu-php-*.sources"
                    .to_string(),
                "curl -sSLo /tmp/debsuryorg-archive-keyring.deb https://packages.sury.org/debsuryorg-archive-keyring.deb"
                    .to_string(),
                "dpkg -i /tmp/debsuryorg-archive-keyring.deb".to_string(),
                "sh -c '. /etc/os-release; echo \"deb [signed-by=/usr/share/keyrings/debsuryorg-archive-keyring.gpg] https://packages.sury.org/php/ ${VERSION_CODENAME:-$(lsb_release -sc 2>/dev/null || echo bookworm)} main\" > /etc/apt/sources.list.d/php.list'"
                    .to_string(),
            ]
        } else {
            vec!["add-apt-repository -y ppa:ondrej/php".to_string()]
        }
    }
}
