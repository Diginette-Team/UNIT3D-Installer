//! TOML-driven configuration: the Rust replacement for the legacy
//! `src/Configs/{app,os}.php` PHP arrays.
//!
//! All sections implement [`Default`] and are tagged with `#[serde(default)]`,
//! so a partial user TOML file overlays on top of the built-in defaults
//! without needing manual coalescing logic.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub unit3d: Unit3dSection,
    #[serde(default)]
    pub app: AppSection,
    #[serde(default)]
    pub os: OsSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit3dSection {
    /// Minimum PHP version that the target box must run.
    #[serde(default = "default_min_php_version")]
    pub min_php_version: String,
    /// Git repository to clone.
    #[serde(default = "default_repository")]
    pub repository: String,
    /// Tag or branch to checkout.
    #[serde(default = "default_tag")]
    pub tag: String,
}

impl Default for Unit3dSection {
    fn default() -> Self {
        Self {
            min_php_version: default_min_php_version(),
            repository: default_repository(),
            tag: default_tag(),
        }
    }
}

fn default_min_php_version() -> String {
    "8.5".to_string()
}
fn default_repository() -> String {
    "https://github.com/HDInnovations/UNIT3D.git".to_string()
}
fn default_tag() -> String {
    "v9.2.0".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSection {
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default = "default_ssl")]
    pub ssl: bool,
    #[serde(default = "default_branch")]
    pub branch: String,

    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub owner_email: String,
    #[serde(default)]
    pub password: String,

    #[serde(default = "default_db_driver")]
    pub db_driver: DbDriver,
    #[serde(default = "default_db_name")]
    pub db: String,
    #[serde(default = "default_db_user")]
    pub dbuser: String,
    #[serde(default)]
    pub dbpass: String,
    #[serde(default)]
    pub dbrootpass: String,

    #[serde(default = "default_mail_driver")]
    pub mail_driver: String,
    #[serde(default)]
    pub mail_host: String,
    #[serde(default = "default_mail_port")]
    pub mail_port: String,
    #[serde(default)]
    pub mail_username: String,
    #[serde(default)]
    pub mail_password: String,
    #[serde(default)]
    pub mail_from_name: String,

    #[serde(default = "default_echo_port")]
    pub echo_port: u16,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    #[serde(default)]
    pub tmdb_key: String,
    #[serde(default)]
    pub meilisearch_key: String,
}

impl Default for AppSection {
    fn default() -> Self {
        Self {
            server_name: String::new(),
            ip: String::new(),
            hostname: String::new(),
            ssl: default_ssl(),
            branch: default_branch(),
            owner: String::new(),
            owner_email: String::new(),
            password: String::new(),
            db_driver: default_db_driver(),
            db: default_db_name(),
            dbuser: default_db_user(),
            dbpass: String::new(),
            dbrootpass: String::new(),
            mail_driver: default_mail_driver(),
            mail_host: String::new(),
            mail_port: default_mail_port(),
            mail_username: String::new(),
            mail_password: String::new(),
            mail_from_name: String::new(),
            echo_port: default_echo_port(),
            ssh_port: default_ssh_port(),
            tmdb_key: String::new(),
            meilisearch_key: String::new(),
        }
    }
}

fn default_ssl() -> bool {
    true
}
fn default_branch() -> String {
    "master".to_string()
}
fn default_db_driver() -> DbDriver {
    DbDriver::MariaDb
}
fn default_db_name() -> String {
    "unit3d".to_string()
}
fn default_db_user() -> String {
    "unit3d".to_string()
}
fn default_mail_driver() -> String {
    "smtp".to_string()
}
fn default_mail_port() -> String {
    "587".to_string()
}
fn default_echo_port() -> u16 {
    8443
}
fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum DbDriver {
    Mysql,
    MariaDb,
    Postgres,
}

impl DbDriver {
    /// The Laravel `.env` `DB_CONNECTION` value.
    pub fn as_db_connection(&self) -> &'static str {
        match self {
            DbDriver::Mysql => "mysql",
            DbDriver::MariaDb => "mariadb",
            DbDriver::Postgres => "pgsql",
        }
    }

    /// The apt package name providing the server.
    #[allow(dead_code)]
    pub fn package(&self) -> &'static str {
        match self {
            DbDriver::Mysql => "mysql-server",
            DbDriver::MariaDb => "mariadb-server",
            DbDriver::Postgres => "postgresql",
        }
    }

    /// The admin CLI binary used to issue `CREATE DATABASE` / `CREATE USER`.
    #[allow(dead_code)]
    pub fn admin_binary(&self) -> &'static str {
        match self {
            DbDriver::Mysql => "mysql",
            DbDriver::MariaDb => "mariadb",
            DbDriver::Postgres => "psql",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsSection {
    #[serde(default)]
    pub ubuntu: UbuntuOs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UbuntuOs {
    #[serde(default = "default_pkg_manager")]
    pub pkg_manager: String,
    #[serde(default = "default_web_user")]
    pub web_user: String,
    #[serde(default = "default_install_dir")]
    pub install_dir: PathBuf,
    #[serde(default = "default_nginx_sites")]
    pub nginx_sites_available_path: PathBuf,
    #[serde(default)]
    pub software: SoftwareSection,
}

impl Default for UbuntuOs {
    fn default() -> Self {
        Self {
            pkg_manager: default_pkg_manager(),
            web_user: default_web_user(),
            install_dir: default_install_dir(),
            nginx_sites_available_path: default_nginx_sites(),
            software: SoftwareSection::default(),
        }
    }
}

fn default_pkg_manager() -> String {
    "apt-get".to_string()
}
fn default_web_user() -> String {
    "www-data".to_string()
}
fn default_install_dir() -> PathBuf {
    PathBuf::from("/var/www/html")
}
fn default_nginx_sites() -> PathBuf {
    PathBuf::from("/etc/nginx/sites-available")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareSection {
    #[serde(default = "default_software")]
    pub packages: BTreeMap<String, String>,
    #[serde(default = "default_php_extensions")]
    pub php_extensions: Vec<String>,
}

impl Default for SoftwareSection {
    fn default() -> Self {
        Self {
            packages: default_software(),
            php_extensions: default_php_extensions(),
        }
    }
}

fn default_software() -> BTreeMap<String, String> {
    default_software_for_db_driver(default_db_driver())
}

fn default_software_for_db_driver(db_driver: DbDriver) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let db_pkg = db_driver.package();
    let items = [
        ("build-essential", "Basic C/C++ Development Environment"),
        ("nginx", "Web Server"),
        (
            db_pkg,
            match db_driver {
                DbDriver::Mysql => "Database Server (MySQL)",
                DbDriver::MariaDb => "Database Server (MariaDB)",
                DbDriver::Postgres => "Database Server (PostgreSQL)",
            },
        ),
        ("supervisor", "A Process Control System"),
        ("nodejs", "JavaScript Run-time Environment (Includes npm)"),
        ("git", "Version Control"),
        ("tmux", "Screen Multiplexer"),
        ("vim", "Text Editor"),
        ("wget", "Transfer Data From A Server"),
        ("zip", "Compress Files"),
        ("unzip", "Decompress Files"),
        ("htop", "Monitor Server Resources"),
        ("redis-server", "Advanced Key-Value Store"),
        ("cron", "Process Scheduling Daemon"),
        ("acl", "Access Control Lists"),
        ("net-tools", "Network diagnostics"),
        ("gnupg", "GnuPG"),
        ("lsb-release", "LSB version info"),
        ("apt-transport-https", "HTTPS apt transport"),
        ("ca-certificates", "SSL certificates"),
        ("software-properties-common", "PPA management"),
        ("certbot", "Let's Encrypt SSL bot"),
        ("python3-certbot-nginx", "Certbot nginx plugin"),
    ];
    for (k, v) in items {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

fn software_packages_for_driver(
    packages: &BTreeMap<String, String>,
    db_driver: DbDriver,
) -> BTreeMap<String, String> {
    let db_pkg = db_driver.package();
    let mut filtered = BTreeMap::new();
    for (pkg, desc) in packages {
        if matches!(
            pkg.as_str(),
            "mysql-server" | "mariadb-server" | "postgresql"
        ) {
            if pkg == db_pkg {
                filtered.insert(pkg.clone(), desc.clone());
            }
        } else {
            filtered.insert(pkg.clone(), desc.clone());
        }
    }
    filtered
}

fn default_php_extensions() -> Vec<String> {
    [
        "php8.5-fpm",
        "php8.5-cli",
        "php8.5-mysql",
        "php8.5-pgsql",
        "php8.5-sqlite3",
        "php8.5-redis",
        "php8.5-memcached",
        "php8.5-curl",
        "php8.5-gd",
        "php8.5-imagick",
        "php8.5-mbstring",
        "php8.5-xml",
        "php8.5-zip",
        "php8.5-bcmath",
        "php8.5-intl",
        "php8.5-soap",
        // `php8.5-opcache` package that may not exist in all repos.
        "php8.5-readline",
        "php8.5-common",
        "php8.5-igbinary",
        "php8.5-msgpack",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed reading config file {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("failed parsing TOML config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error(
        "config file {0} is empty — refusing to run with all-default settings.\n\
         Either fill in the file (copy unit3d-installer.example.toml), or omit\n\
         `--config` entirely to answer the questions interactively."
    )]
    Empty(PathBuf),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

impl Config {
    /// Load configuration from an optional TOML file. Missing sections and
    /// fields transparently fall back to the baked-in defaults via
    /// `#[serde(default)]`.
    pub fn load(maybe_path: Option<&Path>) -> Result<Self, ConfigError> {
        if let Some(path) = maybe_path {
            let text = std::fs::read_to_string(path)
                .map_err(|e| ConfigError::Read(path.to_path_buf(), e))?;
            if is_effectively_empty(&text) {
                return Err(ConfigError::Empty(path.to_path_buf()));
            }
            let mut cfg: Config = toml::from_str(&text)?;
            if !text.contains("[os.ubuntu.software]") {
                let filtered = software_packages_for_driver(
                    &cfg.os.ubuntu.software.packages,
                    cfg.app.db_driver,
                );
                cfg.os.ubuntu.software = SoftwareSection {
                    packages: filtered,
                    php_extensions: cfg.os.ubuntu.software.php_extensions,
                };
            }
            cfg.validate()?;
            return Ok(cfg);
        }
        Ok(Config::default())
    }

    /// Structural validation that catches obviously-broken configuration
    /// before any destructive step runs. FQDN and port checks apply to both
    /// config-file and interactive paths. Every string that later reaches a
    /// shell command line is constrained to a safe character set here, which
    /// is what makes the command-injection class of bugs unreachable.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // hostname must look like a FQDN unless explicitly "localhost".
        if !self.app.hostname.is_empty()
            && self.app.hostname != "localhost"
            && !is_valid_fqdn(&self.app.hostname)
        {
            return Err(ConfigError::Invalid(format!(
                "app.hostname '{}' is not a valid fully-qualified domain name",
                self.app.hostname
            )));
        }
        if self.app.echo_port == 0 {
            return Err(ConfigError::Invalid(
                "app.echo_port must be a non-zero TCP port (1-65535)".to_string(),
            ));
        }
        if self.app.ssh_port == 0 {
            return Err(ConfigError::Invalid(
                "app.ssh_port must be a non-zero TCP port (1-65535)".to_string(),
            ));
        }
        // Database name / user flow into `mysql -e` and `psql` strings.
        if !is_safe_token(&self.app.db) {
            return Err(ConfigError::Invalid(format!(
                "app.db '{}' must be an identifier using only [A-Za-z0-9_.-]",
                self.app.db
            )));
        }
        if !is_safe_token(&self.app.dbuser) {
            return Err(ConfigError::Invalid(format!(
                "app.dbuser '{}' must be an identifier using only [A-Za-z0-9_.-]",
                self.app.dbuser
            )));
        }
        // owner is optional until filled by interactive prompts.
        if !self.app.owner.is_empty() && !is_safe_user(&self.app.owner) {
            return Err(ConfigError::Invalid(format!(
                "app.owner '{}' must be a username using only [A-Za-z0-9_-]",
                self.app.owner
            )));
        }
        // web_user flows into `sudo -u` and chown.
        if !is_safe_user(self.web_user()) {
            return Err(ConfigError::Invalid(format!(
                "os.ubuntu.web_user '{}' must be a username using only [A-Za-z0-9_-]",
                self.web_user()
            )));
        }
        // install_dir flows into `rm -rf`, `cd`, chmod/chown, cron and sed.
        if !is_safe_install_dir(self.install_dir()) {
            return Err(ConfigError::Invalid(format!(
                "os.ubuntu.install_dir '{}' must be an absolute path with no '..', \
                 quotes, spaces, or shell metacharacters",
                self.install_dir().display()
            )));
        }
        // Owner email reaches the certbot command line.
        if !self.app.owner_email.is_empty() && !is_valid_email(&self.app.owner_email) {
            return Err(ConfigError::Invalid(format!(
                "app.owner_email '{}' is not a valid email address",
                self.app.owner_email
            )));
        }
        // Git tag/branch and repository reach `git clone -b {tag} {url}`.
        if !is_safe_ref(&self.unit3d.tag) {
            return Err(ConfigError::Invalid(format!(
                "unit3d.tag '{}' contains characters not allowed in a git ref",
                self.unit3d.tag
            )));
        }
        if !is_safe_repository(&self.unit3d.repository) {
            return Err(ConfigError::Invalid(
                "unit3d.repository must be an https:// or git@ URL".to_string(),
            ));
        }
        // mail_port is written into .env; a numeric range keeps it a value.
        if !self.app.mail_port.is_empty() {
            let port_ok = self
                .app
                .mail_port
                .parse::<u16>()
                .map(|p| p != 0)
                .unwrap_or(false);
            if !port_ok {
                return Err(ConfigError::Invalid(format!(
                    "app.mail_port '{}' must be a numeric TCP port (1-65535)",
                    self.app.mail_port
                )));
            }
        }
        Ok(())
    }

    /// Resolve the install dir, falling back to the OS section default.
    pub fn install_dir(&self) -> &Path {
        &self.os.ubuntu.install_dir
    }

    pub fn web_user(&self) -> &str {
        &self.os.ubuntu.web_user
    }
}

/// Conservative FQDN check: at least two dot-separated labels of only
/// `[A-Za-z0-9-]` characters, no shell metacharacters, no control chars.
/// Allows a trailing dot.
pub(crate) fn is_valid_fqdn(s: &str) -> bool {
    let s = s.trim_end_matches('.');
    if s.is_empty() || s.len() > 253 || s.contains(' ') || s.contains('/') {
        return false;
    }
    if s.chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '.'))
    {
        return false;
    }
    if s.split('.').count() < 2 {
        return false;
    }
    s.split('.').all(is_valid_dns_label)
}

/// A single DNS label: 1-63 chars, alphanumeric, optional interior dashes,
/// never a bare dash or a dash at either end.
fn is_valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// A DNS label / shell-safe token: only `[A-Za-z0-9_.-]`.
pub(crate) fn is_safe_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// A Unix username / group: `[A-Za-z0-9_-]`, no leading dash.
pub(crate) fn is_safe_user(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// A git tag / branch: `[A-Za-z0-9._/-]`, no spaces or shell metacharacters.
fn is_safe_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/'))
}

/// An install dir must be an absolute path with no `..` segments and no
/// characters that would break out of a shell string or confuse tooling.
fn is_safe_install_dir(p: &Path) -> bool {
    let s = p.to_string_lossy();
    if !s.starts_with('/') {
        return false;
    }
    if s.contains('\'')
        || s.contains('"')
        || s.contains('\\')
        || s.contains('$')
        || s.contains(';')
        || s.contains('`')
        || s.contains(' ')
        || s.contains('\n')
    {
        return false;
    }
    p.components().all(|c| c != std::path::Component::ParentDir)
}

/// A basic RFC-ish email shape: exactly one `@`, a dot in the domain, and
/// no whitespace/shell metacharacters. Used before the address reaches the
/// certbot command line.
fn is_valid_email(s: &str) -> bool {
    if s.is_empty()
        || s.len() > 254
        || s.contains(' ')
        || s.contains(';')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('`')
        || s.contains('$')
        || s.contains('\n')
    {
        return false;
    }
    match s.rsplit_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && !domain.is_empty()
                && domain.contains('.')
                && domain
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        }
        None => false,
    }
}

/// A git repository URL: https(s) git URLs only, no shell metacharacters.
fn is_safe_repository(s: &str) -> bool {
    (s.starts_with("https://") || s.starts_with("git@"))
        && s.len() <= 512
        && !s.chars().any(|c| {
            matches!(
                c,
                '\'' | '"' | '`' | '$' | ';' | '\\' | '\n' | ' ' | '(' | ')'
            )
        })
}

/// True when a config file contains only comments and whitespace — i.e. no
/// actual TOML keys. Used to refuse running with silently all-default values.
fn is_effectively_empty(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .all(|line| line.is_empty() || line.starts_with('#'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_effectively_empty() {
        assert!(is_effectively_empty(""));
        assert!(is_effectively_empty("   \n\n\t\n"));
        assert!(is_effectively_empty("# just a comment\n\n  # another\n"));
    }

    #[test]
    fn any_key_means_not_empty() {
        assert!(!is_effectively_empty("ssl = true"));
        assert!(!is_effectively_empty(
            "[app]\nhostname = \"tracker.example.com\""
        ));
        assert!(!is_effectively_empty("# comment first\n[app]\n"));
    }

    #[test]
    fn load_without_config_returns_defaults() {
        let cfg = Config::load(None).unwrap();
        assert_eq!(cfg.app.db_driver, DbDriver::MariaDb);
        assert_eq!(cfg.unit3d.min_php_version, "8.5");
    }

    #[test]
    fn load_empty_config_refuses() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "# nothing configured\n\n").unwrap();
        let err = Config::load(Some(tmp.path())).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn load_blank_config_refuses() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "\n\n   \n").unwrap();
        assert!(Config::load(Some(tmp.path())).is_err());
    }

    #[test]
    fn load_real_config_overlays_defaults() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            "[app]\nhostname = \"tracker.example.com\"\nssl = false\n",
        )
        .unwrap();
        let cfg = Config::load(Some(tmp.path())).unwrap();
        assert_eq!(cfg.app.hostname, "tracker.example.com");
        assert!(!cfg.app.ssl);
        // Unset fields still fall back to defaults.
        assert_eq!(cfg.app.db_driver, DbDriver::MariaDb);
    }

    #[test]
    fn db_driver_mappings() {
        assert_eq!(DbDriver::Mysql.as_db_connection(), "mysql");
        assert_eq!(DbDriver::MariaDb.as_db_connection(), "mariadb");
        assert_eq!(DbDriver::Postgres.as_db_connection(), "pgsql");

        assert_eq!(DbDriver::Mysql.package(), "mysql-server");
        assert_eq!(DbDriver::MariaDb.package(), "mariadb-server");
        assert_eq!(DbDriver::Postgres.package(), "postgresql");

        assert_eq!(DbDriver::Mysql.admin_binary(), "mysql");
        assert_eq!(DbDriver::MariaDb.admin_binary(), "mariadb");
        assert_eq!(DbDriver::Postgres.admin_binary(), "psql");
    }

    #[test]
    fn serde_db_driver_roundtrip_pascal_case() {
        // Confirm the PascalCase rename works from TOML input.
        let cfg: Config = toml::from_str(
            r#"
            [app]
            db_driver = "Postgres"
        "#,
        )
        .unwrap();
        assert_eq!(cfg.app.db_driver, DbDriver::Postgres);
    }

    #[test]
    fn default_software_has_core_packages() {
        let sw = SoftwareSection::default();
        for key in [
            "nginx",
            "mariadb-server",
            "redis-server",
            "supervisor",
            "certbot",
            "git",
            "unzip",
        ] {
            assert!(sw.packages.contains_key(key), "missing package {key}");
        }
        assert!(!sw.packages.contains_key("mysql-server"));
        assert!(!sw.packages.contains_key("postgresql"));
        // Every package has a non-empty description.
        for (pkg, desc) in &sw.packages {
            assert!(!desc.is_empty(), "package {pkg} has empty description");
        }
    }

    #[test]
    fn default_php_extensions_for_85() {
        let exts = SoftwareSection::default().php_extensions;
        assert!(exts.contains(&"php8.5-fpm".to_string()));
        assert!(exts.contains(&"php8.5-mysql".to_string()));
        assert!(exts.contains(&"php8.5-pgsql".to_string()));
        // Some repositories provide opcache as a separate package, others
        // bundle it in `php8.5-common`. Accept either signal as valid.
        assert!(
            exts.contains(&"php8.5-common".to_string())
                || exts.iter().any(|e| e.contains("opcache"))
        );
        // No duplicates.
        let mut sorted = exts.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), exts.len());
    }

    #[test]
    fn unit3d_defaults_pin_tag() {
        let cfg = Config::default();
        assert_eq!(
            cfg.unit3d.repository,
            "https://github.com/HDInnovations/UNIT3D.git"
        );
        assert_eq!(cfg.unit3d.tag, "v9.2.0");
        assert_eq!(cfg.unit3d.min_php_version, "8.5");
        assert_eq!(cfg.app.echo_port, 8443);
        assert_eq!(cfg.app.ssh_port, 22);
        assert!(cfg.app.ssl);
        assert_eq!(cfg.os.ubuntu.web_user, "www-data");
        assert_eq!(cfg.os.ubuntu.install_dir, PathBuf::from("/var/www/html"));
    }

    #[test]
    fn config_path_helpers() {
        let cfg = Config::default();
        assert_eq!(cfg.install_dir(), Path::new("/var/www/html"));
        assert_eq!(cfg.web_user(), "www-data");
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "[app\nhostname = ").unwrap();
        let err = Config::load(Some(tmp.path())).unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
    }

    #[test]
    fn missing_file_returns_read_error() {
        let err = Config::load(Some(Path::new("/no/such/file.toml"))).unwrap_err();
        assert!(matches!(err, ConfigError::Read(..)));
    }

    #[test]
    fn load_full_config_overrides_all_sections() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            r#"
[app]
hostname = "tracker.example.com"
ssl = false
owner = "admin"
db_driver = "Mysql"
db = "tracker_db"
dbuser = "tracker"
echo_port = 6001

[unit3d]
tag = "v9.1.0"

[os.ubuntu]
install_dir = "/srv/unit3d"
web_user = "ubuntu"
"#,
        )
        .unwrap();
        let cfg = Config::load(Some(tmp.path())).unwrap();
        assert_eq!(cfg.app.hostname, "tracker.example.com");
        assert!(!cfg.app.ssl);
        assert_eq!(cfg.app.owner, "admin");
        assert_eq!(cfg.app.db_driver, DbDriver::Mysql);
        assert_eq!(cfg.app.db, "tracker_db");
        assert_eq!(cfg.app.dbuser, "tracker");
        assert_eq!(cfg.app.echo_port, 6001);
        assert_eq!(cfg.unit3d.tag, "v9.1.0");
        assert_eq!(cfg.os.ubuntu.install_dir, PathBuf::from("/srv/unit3d"));
        assert_eq!(cfg.os.ubuntu.web_user, "ubuntu");
        // Unspecified fields fall back to defaults.
        assert_eq!(cfg.app.mail_driver, "smtp");
        assert_eq!(cfg.app.mail_port, "587");
        assert_eq!(cfg.app.branch, "master");
    }

    #[test]
    fn invalid_db_driver_value_is_rejected() {
        let cfg: Result<Config, _> = toml::from_str("[app]\ndb_driver = \"oracle\"\n");
        assert!(cfg.is_err());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // serde defaults ignore unknown keys, keeping forward compatibility.
        let cfg: Config = toml::from_str("[app]\nfuture_field = 42\n").unwrap();
        assert_eq!(cfg.app.db_driver, DbDriver::MariaDb);
    }

    #[test]
    fn os_section_partial_falls_back() {
        let cfg: Config = toml::from_str("[os.ubuntu]\nweb_user = \"www-data\"\n").unwrap();
        assert_eq!(cfg.os.ubuntu.web_user, "www-data");
        // pkg_manager default preserved.
        assert_eq!(cfg.os.ubuntu.pkg_manager, "apt-get");
    }

    #[test]
    fn echo_port_bounds_are_u16() {
        let ok: Config = toml::from_str("[app]\necho_port = 65535\n").unwrap();
        assert_eq!(ok.app.echo_port, 65535);
        let err: Result<Config, _> = toml::from_str("[app]\necho_port = 70000\n");
        assert!(err.is_err());
    }

    #[test]
    fn example_config_file_parses() {
        // The shipped example must remain valid TOML and parse cleanly.
        let text = include_str!("../../unit3d-installer.example.toml");
        let cfg: Config = toml::from_str(text).unwrap();
        assert!(!cfg.app.hostname.is_empty());
    }

    #[test]
    fn roundtrip_serialize_contains_all_sections() {
        let cfg = Config::default();
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("[unit3d]"));
        assert!(s.contains("[app]"));
        assert!(s.contains("[os.ubuntu]"));
    }

    #[test]
    fn software_packages_match_selected_db_driver() {
        let sw = SoftwareSection::default();
        assert!(sw.packages.contains_key("mariadb-server"));
        assert!(!sw.packages.contains_key("mysql-server"));
        assert!(!sw.packages.contains_key("postgresql"));

        let mysql_sw = default_software_for_db_driver(DbDriver::Mysql);
        assert!(mysql_sw.contains_key("mysql-server"));
        assert!(!mysql_sw.contains_key("mariadb-server"));
        assert!(!mysql_sw.contains_key("postgresql"));

        let postgres_sw = default_software_for_db_driver(DbDriver::Postgres);
        assert!(postgres_sw.contains_key("postgresql"));
        assert!(!postgres_sw.contains_key("mariadb-server"));
        assert!(!postgres_sw.contains_key("mysql-server"));
    }

    #[test]
    fn no_cruft_in_package_descriptions() {
        for (pkg, desc) in &SoftwareSection::default().packages {
            assert!(!desc.contains('{') && !desc.contains('}'), "{pkg}");
        }
    }

    #[test]
    fn validate_accepts_valid_fqdn() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_bare_hostname() {
        let mut cfg = Config::default();
        cfg.app.hostname = "myserver".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("not a valid fully-qualified"));
    }

    #[test]
    fn validate_accepts_localhost() {
        let mut cfg = Config::default();
        cfg.app.hostname = "localhost".to_string();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_echo_port() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.app.echo_port = 0;
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("echo_port"));
    }

    #[test]
    fn toml_roundtrip_sets_ssh_port() {
        let cfg: Config = toml::from_str("[app]\nssh_port = 2222\n").unwrap();
        assert_eq!(cfg.app.ssh_port, 2222);
        // Default is 22 when unset.
        let cfg: Config = toml::from_str("[app]\nhostname = \"tracker.example.com\"\n").unwrap();
        assert_eq!(cfg.app.ssh_port, 22);
    }

    #[test]
    fn validate_rejects_zero_ssh_port() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.app.ssh_port = 0;
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("ssh_port"));
    }

    #[test]
    fn fqdn_checks() {
        assert!(is_valid_fqdn("tracker.example.com"));
        assert!(is_valid_fqdn("a.b"));
        assert!(is_valid_fqdn("sub.domain.co.uk"));
        assert!(!is_valid_fqdn("no-dot"));
        assert!(!is_valid_fqdn("has space.com"));
        assert!(!is_valid_fqdn("has/slash.com"));
        assert!(!is_valid_fqdn(".leading-dot"));
        assert!(!is_valid_fqdn(""));
    }

    #[test]
    fn load_rejects_invalid_hostname_config() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "[app]\nhostname = \"badhostname\"\n").unwrap();
        let err = Config::load(Some(tmp.path())).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[test]
    fn fqdn_rejects_shell_metacharacters() {
        // These pass the old space/slash-only check but must never reach a
        // shell command line (nginx certbot, nginx site, php pool filename).
        for bad in [
            "tracker.example.com;touch /tmp/pwn",
            "tracker.example.com$(id)",
            "tracker.example.com`id`",
            "tracker.example.com'",
            "tracker.example.com\"",
            "tracker\n.example.com",
            "tracker.example.com\t",
            "tracker..example.com",
            "-.example.com",
        ] {
            assert!(!is_valid_fqdn(bad), "{bad} must be rejected");
        }
    }

    #[test]
    fn fqdn_accepts_legit_domains() {
        for ok in [
            "tracker.example.com",
            "www.example.co.uk",
            "a.b",
            "my-tracker.example.com",
            "sub1.sub2.example.net.",
            "localhost.localdomain",
        ] {
            assert!(is_valid_fqdn(ok), "{ok} must be accepted");
        }
    }

    #[test]
    fn safe_token_accepts_rejects() {
        assert!(is_safe_token("unit3d"));
        assert!(is_safe_token("my_db"));
        assert!(is_safe_token("db-2"));
        assert!(!is_safe_token(""));
        assert!(!is_safe_token("unit3d;drop"));
        assert!(!is_safe_token("unit3d'"));
        assert!(!is_safe_token("has space"));
        assert!(!is_safe_token("x".repeat(65).as_str()));
    }

    #[test]
    fn safe_user_accepts_rejects() {
        assert!(is_safe_user("www-data"));
        assert!(is_safe_user("ubuntu"));
        assert!(is_safe_user("unit3d"));
        assert!(!is_safe_user(""));
        assert!(!is_safe_user("-root"));
        assert!(!is_safe_user("user;x"));
        assert!(!is_safe_user("has space"));
        assert!(!is_safe_user("user'"));
    }

    #[test]
    fn safe_ref_accepts_rejects() {
        assert!(is_safe_ref("v9.2.0"));
        assert!(is_safe_ref("master"));
        assert!(is_safe_ref("release/v9.1.0"));
        assert!(!is_safe_ref(""));
        assert!(!is_safe_ref("v9.2.0;x"));
        assert!(!is_safe_ref("v9.2.0 `rm`"));
        assert!(!is_safe_ref("has space"));
    }

    #[test]
    fn safe_install_dir_accepts_rejects() {
        assert!(is_safe_install_dir(Path::new("/var/www/html")));
        assert!(is_safe_install_dir(Path::new("/srv/unit3d")));
        assert!(!is_safe_install_dir(Path::new("/var/www/../etc")));
        assert!(!is_safe_install_dir(Path::new("/var/www/..")));
        assert!(!is_safe_install_dir(Path::new("relative/path")));
        assert!(!is_safe_install_dir(Path::new("/etc;rm -rf /")));
        assert!(!is_safe_install_dir(Path::new("/var/www 'x'")));
        assert!(!is_safe_install_dir(Path::new("/var/www$(id)")));
        assert!(!is_safe_install_dir(Path::new("/var/www `id`")));
        assert!(!is_safe_install_dir(Path::new("/var/www/with space")));
    }

    #[test]
    fn valid_email_checks() {
        assert!(is_valid_email("admin@example.com"));
        assert!(is_valid_email("a.b+c@sub.example.co.uk"));
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("no-at-sign"));
        assert!(!is_valid_email("user@nodot"));
        assert!(!is_valid_email("admin@example.com;rm -rf /"));
        assert!(!is_valid_email("admin@example.com'"));
        assert!(!is_valid_email("ad min@example.com"));
        assert!(!is_valid_email("@example.com"));
    }

    #[test]
    fn safe_repository_checks() {
        assert!(is_safe_repository(
            "https://github.com/HDInnovations/UNIT3D.git"
        ));
        assert!(is_safe_repository("git@github.com:user/repo.git"));
        assert!(!is_safe_repository(""));
        assert!(!is_safe_repository("https://github.com/x;touch /tmp/y"));
        assert!(!is_safe_repository("https://github.com/x `id`"));
        assert!(!is_safe_repository("file:///etc/passwd"));
    }

    #[test]
    fn validate_rejects_bad_db_user() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.app.dbuser = "unit3d;DROP TABLE x;".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("dbuser"));
    }

    #[test]
    fn validate_rejects_bad_db_name() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.app.db = "unit3d' --".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("app.db"));
    }

    #[test]
    fn validate_rejects_bad_web_user() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.os.ubuntu.web_user = "www-data;id".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("web_user"));
    }

    #[test]
    fn validate_rejects_bad_install_dir() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.os.ubuntu.install_dir = PathBuf::from("/var/www/../../etc");
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("install_dir"));
    }

    #[test]
    fn validate_rejects_bad_email() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.app.owner_email = "admin@example.com;touch /tmp/pwn".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("owner_email"));
    }

    #[test]
    fn validate_rejects_bad_tag() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.unit3d.tag = "v9.2.0 `rm -rf /`".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("unit3d.tag"));
    }

    #[test]
    fn validate_rejects_bad_repository() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.unit3d.repository = "https://github.com/x;touch /tmp/y".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("repository"));
    }

    #[test]
    fn validate_rejects_bad_mail_port() {
        let mut cfg = Config::default();
        cfg.app.hostname = "tracker.example.com".to_string();
        cfg.app.mail_port = "587\nMAIL_PASSWORD=hax".to_string();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("mail_port"));
    }

    #[test]
    fn validate_accepts_full_valid_config() {
        let cfg = Config::default();
        // Defaults are all safe.
        assert!(cfg.validate().is_ok());
    }
}
