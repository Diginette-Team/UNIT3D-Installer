//! MySQL/MariaDB driver. Provisioning logic is shared between them: install
//! the package, start the service, write `/root/.my.cnf`, create the
//! database + user, harden the root account, drop the test database. The
//! only differences are the binary names and the init command — both
//! handled by the [`Flavor`] argument.
//!
//! Replaces `src/Installer/Database/{MySqlSetup,MariaDbSetup}.php`.

use crate::config::DbDriver;
use crate::resources::my_cnf::MyCnfTemplate;
use crate::steps::Context;
use crate::system::memory;
use anyhow::Result;
use askama::Template;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub enum Flavor {
    Mysql,
    MariaDb,
}

impl Flavor {
    fn binary(&self) -> &'static str {
        match self {
            Flavor::Mysql => "mysql",
            Flavor::MariaDb => "mariadb",
        }
    }
    fn server_pkg(&self) -> &'static str {
        match self {
            Flavor::Mysql => "mysql-server",
            Flavor::MariaDb => "mariadb-server",
        }
    }
    fn init_bin(&self) -> &'static str {
        match self {
            Flavor::Mysql => "mysqld",
            Flavor::MariaDb => "mariadbd",
        }
    }
    fn admin_bin(&self) -> &'static str {
        match self {
            Flavor::Mysql => "mysqladmin",
            Flavor::MariaDb => "mariadb-admin",
        }
    }
    fn service_name(&self) -> &'static str {
        match self {
            Flavor::Mysql => "mysql",
            Flavor::MariaDb => "mariadb",
        }
    }
}

pub fn configure(ctx: &mut Context) -> Result<()> {
    let flavor = match ctx.config.app.db_driver {
        DbDriver::Mysql => Flavor::Mysql,
        DbDriver::MariaDb => Flavor::MariaDb,
        _ => unreachable!("postgres should not reach this driver"),
    };
    ctx.style
        .info(&format!("Installing {} server", flavor.server_pkg()));

    // Install the server package.
    ctx.run(&format!(
        "{} install -y {}",
        ctx.config.os.ubuntu.pkg_manager,
        flavor.server_pkg()
    ))?;

    // Pick a tuning profile based on physical RAM (mirrors the PHP `memory()`
    // switch).
    let mycnf = pick_mycnf(memory());
    let cnf_src = format!("/etc/mysql/conf.d/{mycnf}");

    if ctx.dry_run || !Path::new(&cnf_src).exists() {
        // Use the legacy bundled tuning file by writing a sensible default
        // directly when the legacy stub file is not present on this box.
        let body = default_tuning_for(mycnf);
        ctx.write_file(std::path::Path::new(&cnf_src), body)?;
    }

    // On a fresh Ubuntu data dir is empty — initialize it.
    if ctx.dry_run || !Path::new("/var/lib/mysql").exists() || is_dir_empty("/var/lib/mysql")? {
        ctx.run_all([
            // stop any running service instances before initialization
            "systemctl stop mariadb || true".to_string(),
            "systemctl stop mysql || true".to_string(),
            // ensure directory exists and is owned by mysql
            "mkdir -p /var/lib/mysql".to_string(),
            // make sure `mysql` user exists; if not, try to create it
            "id -u mysql >/dev/null 2>&1 || (groupadd -r mysql >/dev/null 2>&1 || true) && (useradd -r -g mysql -s /usr/sbin/nologin mysql >/dev/null 2>&1 || true)".to_string(),
            // recursively set ownership and backup stale lock files that can
            // block init if leftover by previous runs
            "chown -R mysql:mysql /var/lib/mysql".to_string(),
            "[ -e /var/lib/mysql/aria_log_control ] && mv /var/lib/mysql/aria_log_control /var/lib/mysql/aria_log_control.bak || true".to_string(),
            "[ -e /var/lib/mysql/ibdata1 ] && mv /var/lib/mysql/ibdata1 /var/lib/mysql/ibdata1.bak || true".to_string(),
            match flavor {
                    Flavor::MariaDb => format!(
                        "( {bin} --initialize-insecure --user=mysql ) || ( mariadb-install-db --user=mysql --datadir=/var/lib/mysql ) || runuser -u mysql -- mariadb-install-db --user=mysql --datadir=/var/lib/mysql || runuser -u mysql -- mysql_install_db --user=mysql --datadir=/var/lib/mysql",
                        bin = flavor.init_bin()
                    ),
                    Flavor::Mysql => format!(
                        "( {bin} --initialize-insecure --user=mysql ) || runuser -u mysql -- {bin} --initialize-insecure",
                        bin = flavor.init_bin()
                    ),
                },
        ])?;
    }

    // `/root/.my.cnf` lets subsequent `mysql -e ...` calls authenticate
    // without prompting. The password must match what `mysqladmin` actually
    // sets below, which is the `shell_quote`-scrubbed value — otherwise auth
    // fails silently later.
    let tpl = MyCnfTemplate {
        password: &shell_quote(&ctx.config.app.dbrootpass),
    };
    let rendered = tpl.render()?;
    ctx.write_secret_file(std::path::Path::new("/root/.my.cnf"), &rendered)?;
    ctx.run("chmod 600 /root/.my.cnf")?;

    // Start the service and set the root password.
    ctx.run_all([
        "mkdir -p /var/run/mysqld".to_string(),
        "chown mysql:mysql /var/run/mysqld".to_string(),
        // NOTE: never `chmod -R` here. Recursing would also hit the live
        // `mysqld.sock` (present on re-installs) — a unix socket needs write
        // permission to connect(), so a 755 socket locks out every non-root
        // client (www-data/PHP-FPM) with SQLSTATE[HY000] [2002] Permission
        // denied. The directory itself is already 755 from mkdir.
        format!("update-rc.d {} defaults", flavor.service_name()),
        format!("service {} start", flavor.service_name()),
        format!(
            "{} -u root password {}",
            flavor.admin_bin(),
            shell_quote(&ctx.config.app.dbrootpass)
        ),
    ])?;

    // db / dbuser are shell-quoted (and validated in the config path) so a
    // hostile value can never break out of the surrounding `bash -c` string
    // or the SQL statement.
    let db = shell_quote(&ctx.config.app.db);
    let dbuser = shell_quote(&ctx.config.app.dbuser);
    let dbpass = shell_quote(&ctx.config.app.dbpass);
    let root_pass = shell_quote(&ctx.config.app.dbrootpass);
    let bin = flavor.binary();

    let mut critical: Vec<String> = vec![
        format!("{bin} -e \"DROP USER IF EXISTS '{dbuser}'@'localhost'\""),
        format!("{bin} -e \"DROP DATABASE IF EXISTS {db}\""),
        format!("{bin} -e \"CREATE DATABASE {db}\""),
        format!("{bin} -e \"CREATE USER '{dbuser}'@'localhost' IDENTIFIED BY '{dbpass}'\""),
        format!("{bin} -e \"GRANT ALL PRIVILEGES ON {db} . * TO '{dbuser}'@'localhost'\""),
    ];

    if matches!(flavor, Flavor::Mysql) {
        critical.push(format!(
            "{bin} -e \"ALTER USER 'root'@'localhost' IDENTIFIED WITH mysql_native_password BY '{root_pass}'\"",
            bin = bin,
            root_pass = root_pass
        ));
    }

    critical.extend_from_slice(&[
        format!("{bin} -e \"DELETE FROM mysql.user WHERE User=''\""),
        format!(
            "{bin} -e \"DELETE FROM mysql.user WHERE User='root' AND Host NOT IN ('localhost', '127.0.0.1', '::1')\""
        ),
        format!("{bin} -e \"FLUSH PRIVILEGES\""),
    ]);

    ctx.run_all(critical)?;

    // Non-critical: drop the test database.
    ctx.run_all([
        format!("{bin} -e \"DROP DATABASE IF EXISTS test\""),
        format!("{bin} -e \"DELETE FROM mysql.db WHERE Db='test' OR Db='test\\\\_%'\""),
    ])
    .ok();
    let _ = ctx.run(&format!("{bin} -e \"FLUSH PRIVILEGES\""));

    ctx.style.info("Database configured successfully");
    Ok(())
}

fn pick_mycnf(mem_kb: u64) -> &'static str {
    if (1_200_000..3_900_000).contains(&mem_kb) {
        "my-medium.cnf"
    } else if mem_kb >= 3_900_000 {
        "my-large.cnf"
    } else {
        "my-small.cnf"
    }
}

fn default_tuning_for(name: &str) -> &'static str {
    match name {
        "my-large.cnf" => DEFAULT_LARGE,
        "my-medium.cnf" => DEFAULT_MEDIUM,
        _ => DEFAULT_SMALL,
    }
}

const DEFAULT_SMALL: &str = "[mysqld]\nkey_buffer_size = 16K\nmax_connections = 30\nmax_user_connections = 20\nwait_timeout = 10\ninnodb_file_per_table\n";
const DEFAULT_MEDIUM: &str = "[mysqld]\nkey_buffer_size = 16M\nmax_allowed_packet = 16M\nmax_connections = 70\nmax_user_connections = 30\nwait_timeout = 10\ninnodb_file_per_table\n";
const DEFAULT_LARGE: &str = "[mysqld]\nkey_buffer_size = 256M\nmax_allowed_packet = 32M\ntable_open_cache = 256\nthread_cache_size = 8\nmax_connections = 200\nmax_user_connections = 50\nwait_timeout = 10\ninnodb_file_per_table\n";

fn is_dir_empty(p: &str) -> Result<bool> {
    Ok(Path::new(p).exists() && std::fs::read_dir(p)?.next().is_none())
}

/// Single-quote a value for use inside an SQL identifier/clause. This is a
/// blunt protector against characters the shell would otherwise interpret;
/// the installer warns explicitly in interactive prompts that special
/// characters aren't supported yet.
///
/// Hardened for the Rust port: beyond stripping `'`, we also strip every
/// character that could break out of the surrounding `bash -c` double-quoted
/// string or inject a second SQL statement — `"`, `\`, backticks, `$`,
/// `;`, newlines, and CR. The result is a plain token safe to embed in both
/// a shell string and an SQL literal.
pub fn shell_quote(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\' | '`' | '$' | ';' | '\n' | '\r' | '\0'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_small() {
        assert_eq!(pick_mycnf(500_000), "my-small.cnf");
    }
    #[test]
    fn pick_medium() {
        assert_eq!(pick_mycnf(2_000_000), "my-medium.cnf");
    }
    #[test]
    fn pick_large() {
        assert_eq!(pick_mycnf(8_000_000), "my-large.cnf");
    }

    #[test]
    fn shell_quote_strips_single_quotes() {
        assert_eq!(shell_quote("a'b'c"), "abc");
    }

    #[test]
    fn default_tuning_for_small() {
        assert!(DEFAULT_SMALL.contains("innodb_file_per_table"));
    }

    #[test]
    fn pick_mycnf_boundaries() {
        // Below 1.2 GB → small.
        assert_eq!(pick_mycnf(0), "my-small.cnf");
        assert_eq!(pick_mycnf(1_199_999), "my-small.cnf");
        // [1.2 GB, 3.9 GB) → medium.
        assert_eq!(pick_mycnf(1_200_000), "my-medium.cnf");
        assert_eq!(pick_mycnf(2_000_000), "my-medium.cnf");
        assert_eq!(pick_mycnf(3_899_999), "my-medium.cnf");
        // ≥ 3.9 GB → large.
        assert_eq!(pick_mycnf(3_900_000), "my-large.cnf");
        assert_eq!(pick_mycnf(u64::MAX), "my-large.cnf");
    }

    #[test]
    fn flavor_attributes() {
        let m = Flavor::Mysql;
        assert_eq!(m.binary(), "mysql");
        assert_eq!(m.server_pkg(), "mysql-server");
        assert_eq!(m.init_bin(), "mysqld");
        assert_eq!(m.admin_bin(), "mysqladmin");
        assert_eq!(m.service_name(), "mysql");

        let md = Flavor::MariaDb;
        assert_eq!(md.binary(), "mariadb");
        assert_eq!(md.server_pkg(), "mariadb-server");
        assert_eq!(md.init_bin(), "mariadbd");
        assert_eq!(md.admin_bin(), "mariadb-admin");
        assert_eq!(md.service_name(), "mariadb");
    }

    #[test]
    fn shell_quote_strips_all_quotes() {
        assert_eq!(shell_quote(""), "");
        assert_eq!(shell_quote("plain"), "plain");
        assert_eq!(shell_quote("a'b'c"), "abc");
        assert_eq!(shell_quote("'"), "");
        assert_eq!(shell_quote("O'Reilly"), "OReilly");
    }

    #[test]
    fn shell_quote_strips_injection_chars() {
        // Everything that could break out of the shell string or inject a
        // second SQL statement is removed; innocuous chars (space, `/`)
        // pass through.
        assert_eq!(shell_quote("pw\"; rm -rf /"), "pw rm -rf /");
        assert_eq!(shell_quote("pw$(whoami)"), "pw(whoami)");
        assert_eq!(shell_quote("pw`id`"), "pwid");
        assert_eq!(shell_quote("pw;DROP TABLE x;"), "pwDROP TABLE x");
        assert_eq!(shell_quote("line1\nline2"), "line1line2");
        assert_eq!(shell_quote("a\\b"), "ab");
        assert_eq!(shell_quote("null\0char"), "nullchar");
    }

    #[test]
    fn shell_quote_keeps_alphanumerics_and_common_punctuation() {
        assert_eq!(shell_quote("p@ss-w0rd_1"), "p@ss-w0rd_1");
        assert_eq!(shell_quote("ABCdef123-_@."), "ABCdef123-_@.");
    }

    #[test]
    fn default_tuning_large_is_bigger() {
        assert!(DEFAULT_LARGE.contains("max_connections = 200"));
        assert!(DEFAULT_MEDIUM.contains("max_connections = 70"));
        assert!(DEFAULT_SMALL.contains("max_connections = 30"));
    }

    #[test]
    fn is_dir_empty_helper() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(is_dir_empty(tmp.path().to_str().unwrap()).unwrap());
        std::fs::write(tmp.path().join("f"), "x").unwrap();
        assert!(!is_dir_empty(tmp.path().to_str().unwrap()).unwrap());
        // Nonexistent path is "not empty" from the helper's perspective.
        assert!(!is_dir_empty("/nonexistent-dir-xyz").unwrap());
    }

    #[test]
    fn configure_shell_quotes_db_and_user() {
        use crate::process::Exec;
        use crate::steps::Context;
        use std::sync::{Arc, Mutex};

        struct Recording(Arc<Mutex<Vec<String>>>);
        impl Exec for Recording {
            fn run(&self, cmd: &str) -> Result<std::process::Output> {
                self.0.lock().unwrap().push(cmd.to_string());
                Ok(std::process::Output {
                    status: std::os::unix::process::ExitStatusExt::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
        }
        let cmds = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = Context {
            config: crate::config::Config::default(),
            prompter: crate::io::Prompter::new(true),
            style: crate::io::Style,
            exec: Arc::new(Recording(cmds.clone())),
            dry_run: true, // don't touch /root/.my.cnf or /var/lib/mysql
            non_interactive: true,
            config_path: None,
        };
        ctx.config.app.db_driver = DbDriver::Mysql;
        ctx.config.app.db = "my_db;DROP TABLE x;--".to_string();
        ctx.config.app.dbuser = "u'name\"$x".to_string();
        ctx.config.app.dbpass = "p'w;d".to_string();
        ctx.config.app.dbrootpass = "r'oot".to_string();
        configure(&mut ctx).unwrap();
        let cmds = cmds.lock().unwrap();
        // The raw hostile chars must never reach the emitted shell strings.
        let joined = cmds.join("\n");
        assert!(
            !joined.contains(";"),
            "semicolon injection leaked: {joined}"
        );
        assert!(
            !joined.contains("u'name"),
            "shell injection leaked: {joined}"
        );
        assert!(!joined.contains("\"$"), "shell injection leaked: {joined}");
        // The scrubbed identifier forms must appear.
        assert!(
            joined.contains("CREATE DATABASE my_dbDROP TABLE x"),
            "got: {joined}"
        );
        assert!(
            joined.contains("CREATE USER 'unamex'@'localhost'"),
            "got: {joined}"
        );
        assert!(joined.contains("IDENTIFIED BY 'pwd'"), "got: {joined}");
    }
}
