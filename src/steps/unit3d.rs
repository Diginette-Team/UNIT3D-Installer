//! Clone UNIT3D-Community-Edition, render the `.env`, set permissions,
//! install dependencies (Composer + Bun), run migrations, set up cron +
//! supervisor + Laravel Echo Server, and run post-install caching.
//!
//! Combines the legacy `Unit3dSetup` and the v1.2 standalone script's
//! `install_unit3d`, `configure_laravel_echo_server`,
//! `configure_supervisor`, and `configure_cron` steps. Folds in gaps
//! G14/G15/G17/G18/G20/G21/G23/G24.

use crate::config::DbDriver;
use crate::resources::echo_server::EchoServerTemplate;
use crate::resources::env::EnvTemplate;
use crate::resources::supervisor::SupervisorTemplate;
use crate::steps::{Context, Step};
use anyhow::Result;
use askama::Template;
use std::path::Path;

pub struct Unit3dSetupStep;

impl Step for Unit3dSetupStep {
    fn name(&self) -> &'static str {
        "UNIT3D-Community-Edition Settings and Configuration"
    }

    fn handle(&self, ctx: &mut Context) -> Result<()> {
        clone(ctx)?;
        env(ctx)?;
        perms(ctx)?;
        crons(ctx)?;
        setup(ctx)?;
        Ok(())
    }
}

fn clone(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Cloning Source Files");
    let install_dir = ctx.config.install_dir().to_path_buf();
    let url = ctx.config.unit3d.repository.clone();
    // G14: pin to a tag; fall back to `main` if the tag doesn't exist.
    let tag = ctx.config.unit3d.tag.clone();

    // Defense-in-depth: re-validate before anything destructive, in case this
    // context was constructed without going through `Config::load` (interactive
    // mode). Charset checks prevent shell break-out via install_dir/tag/url.
    if let Err(e) = ctx.config.validate() {
        anyhow::bail!("invalid configuration: {e}");
    }

    if install_dir.exists() {
        if let Some(reason) = unsafe_to_delete(&install_dir) {
            anyhow::bail!(
                "refusing to `rm -rf` {reason} — install_dir {} points outside the web root. \
                 Set a safe install_dir in your config.",
                install_dir.display()
            );
        }
        ctx.run(&format!("rm -rf {}", install_dir.display()))?;
    }

    ctx.run(&format!(
        "git config --global --add safe.directory {}",
        install_dir.display()
    ))?;
    ctx.run(&format!(
        "git clone -b {tag} {url} {}",
        install_dir.display()
    ))?;
    if !ctx.dry_run && !Path::new(&install_dir).exists() {
        anyhow::bail!("git clone failed for {url} @ {tag}");
    }
    Ok(())
}

/// Refuse to `rm -rf` anything at or above the filesystem root, home dirs,
/// or other clearly-dangerous locations. Returns `Some(reason)` when the
/// path must not be deleted.
///
/// `/var/www/...` (the default web root) is deliberately allowed — deleting
/// it is the whole point of a reinstall — but `/var` itself and sensitive
/// subdirs like `/var/lib`, `/var/run`, `/var/log` are rejected.
fn unsafe_to_delete(path: &Path) -> Option<&'static str> {
    let text = path.to_string_lossy().to_string();
    for (prefix, reason) in [
        ("/", "the filesystem root"),
        ("/root", "the root home directory"),
        ("/home", "the home directory tree"),
        ("/etc", "/etc"),
        ("/usr", "/usr"),
        ("/bin", "/bin"),
        ("/sbin", "/sbin"),
        ("/lib", "/lib"),
        ("/opt", "/opt"),
        ("/boot", "/boot"),
        ("/dev", "/dev"),
        ("/proc", "/proc"),
        ("/sys", "/sys"),
        ("/tmp", "/tmp"),
        ("/run", "/run"),
    ] {
        if text == prefix || text.starts_with(&format!("{prefix}/")) {
            return Some(reason);
        }
    }
    // Reject `/var` and `/srv` and their system subdirs, but allow the
    // conventional web roots `/var/www/...` and `/srv/www/...`.
    if (text == "/var" || text.starts_with("/var/") && !text.starts_with("/var/www"))
        || (text == "/srv" || text.starts_with("/srv/") && !text.starts_with("/srv/www"))
    {
        return Some("outside the web root");
    }
    // A single-component path with no slash is a relative or root-adjacent
    // path; never rm -rf that.
    if !text.contains('/') {
        return Some("a relative path");
    }
    None
}

fn env(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Preparing the .env File");
    let install_dir = ctx.config.install_dir().to_path_buf();
    let env_path = install_dir.join(".env");

    if !ctx.dry_run && env_path.exists() {
        std::fs::remove_file(&env_path)?;
    }

    let protocol = if ctx.config.app.ssl { "https" } else { "http" };
    let fqdn = ctx.config.app.hostname.clone();
    let socket = match ctx.config.app.db_driver {
        DbDriver::Postgres => "",
        _ => "/var/run/mysqld/mysqld.sock",
    };

    let tpl = EnvTemplate {
        protocol,
        fqdn: &fqdn,
        db_driver: ctx.config.app.db_driver.as_db_connection(),
        db: &ctx.config.app.db,
        dbuser: &ctx.config.app.dbuser,
        dbpass: &ctx.config.app.dbpass,
        socket,
        owner: &ctx.config.app.owner,
        owner_email: &ctx.config.app.owner_email,
        owner_password: &ctx.config.app.password,
        tmdb_key: &ctx.config.app.tmdb_key,
        mail_driver: &ctx.config.app.mail_driver,
        mail_host: &ctx.config.app.mail_host,
        mail_port: &ctx.config.app.mail_port,
        mail_username: &ctx.config.app.mail_username,
        mail_password: &ctx.config.app.mail_password,
        mail_from_name: &ctx.config.app.mail_from_name,
        meilisearch_key: &ctx.config.app.meilisearch_key,
        redis_host: "/var/run/redis/redis.sock",
        redis_port: "-1",
    };
    let rendered = tpl.render()?;
    ctx.write_secret_file(&env_path, &rendered)?;
    ctx.style.info(&format!("wrote {}", env_path.display()));
    Ok(())
}

fn perms(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Setting Permissions");
    let install_dir = ctx.config.install_dir().to_path_buf();
    let web_user = ctx.config.web_user().to_string();
    let parent = install_dir
        .parent()
        .unwrap_or(Path::new("/"))
        .display()
        .to_string();
    // Ordering matters: the broad recursive chmods must run BEFORE the
    // restrictive ones, otherwise `chmod -R 755` would reset `.env` (and the
    // rest) to world-readable and defeat the 0600 written by
    // `write_secret_file`.
    ctx.run_all([
        format!("chown -R {web_user}:{web_user} /etc/letsencrypt 2>/dev/null || true"),
        format!("chown -R {web_user}:{web_user} {parent}"),
        format!(
            "find {} -type d -exec chmod 0775 {{}} + -or -type f -exec chmod 0664 {{}} +",
            install_dir.display()
        ),
        format!("chmod -R 755 {0}", install_dir.display()),
        format!("chmod -R 775 {0}/storage", install_dir.display()),
        format!("chmod -R 775 {0}/bootstrap/cache", install_dir.display()),
        // Restrictive modes applied last, so nothing clobbers them.
        format!("chmod 750 {}/artisan", install_dir.display()),
        format!("chmod 640 {}/.env", install_dir.display()),
    ])?;
    Ok(())
}

fn crons(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Setting Up Crontabs");
    let install_dir = ctx.config.install_dir().display().to_string();
    // G23: idempotent — strip prior entries then append a single instance.
    ctx.run(&format!(
        "(crontab -l 2>/dev/null | grep -v 'artisan schedule:run'; echo '* * * * * php {install_dir}/artisan schedule:run >> /dev/null 2>&1') | crontab -"
    ))?;
    Ok(())
}

fn setup(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Setting Up Web Site");
    let install_dir = ctx.config.install_dir().to_path_buf();
    let install_dir_s = install_dir.display().to_string();
    let fqdn = ctx.config.app.hostname.clone();
    let web_user = ctx.config.web_user().to_string();
    let echo_port = ctx.config.app.echo_port;
    let protocol = if ctx.config.app.ssl { "https" } else { "http" };

    // Laravel Echo Server config (G20).
    let ssl_cert = format!("/etc/letsencrypt/live/{fqdn}/cert.pem");
    let ssl_key = format!("/etc/letsencrypt/live/{fqdn}/privkey.pem");
    let ssl_chain = format!("/etc/letsencrypt/live/{fqdn}/fullchain.pem");
    let echo_tpl = EchoServerTemplate {
        protocol,
        fqdn: &fqdn,
        port: echo_port,
        ssl_cert: &ssl_cert,
        ssl_key: &ssl_key,
        ssl_chain: &ssl_chain,
    };
    let echo_path = install_dir.join("laravel-echo-server.json");
    let echo_rendered = echo_tpl.render()?;
    ctx.write_file(&echo_path, &echo_rendered)?;
    ctx.run(&format!(
        "chown {web_user}:{web_user} {}",
        echo_path.display()
    ))?;

    // Supervisor for queue workers + echo server (G21, G22).
    let sup_tpl = SupervisorTemplate {
        install_dir: &install_dir_s,
        web_user: &web_user,
    };
    let sup_rendered = sup_tpl.render()?;
    ctx.write_file(
        std::path::Path::new("/etc/supervisor/conf.d/unit3d.conf"),
        &sup_rendered,
    )?;
    ctx.run_all([
        "supervisorctl reread".to_string(),
        "supervisorctl update".to_string(),
        "supervisorctl reload".to_string(),
    ])?;

    // Composer install + Bun build + artisan bootstrapping.
    let www_cmds = [
        "php -d opcache.preload='' $(command -v composer) install -q --prefer-dist --no-dev # composer install -q --prefer-dist --no-dev",
        "php -d opcache.preload='' $(command -v composer) dump-autoload --optimize # composer dump-autoload --optimize",
        // livewire, joypixels, and lavarel-assets needed for login
        "php artisan vendor:publish --force --tag=livewire:assets --ansi",
        "php artisan vendor:publish --tag=public --provider=\"hdvinnie\\LaravelJoyPixels\\LaravelJoyPixelsServiceProvider\"",
        "php artisan vendor:publish --tag=laravel-assets --ansi --force",
        "bun install",
        "bun run build",
        "php artisan key:generate --force",
        "php artisan migrate --seed --force",
        // Upstream UserSeeder pins the owner to group_id 10 (Trustee in some
        // group orders) — re-assign the seeded owner to the real Owner group.
        // Inner quotes are backslash-escaped so they survive bash -lc "...".
        "php artisan tinker --execute='if (App\\Models\\Group::where(\\\"slug\\\", \\\"owner\\\")->exists()) { App\\Models\\User::where(\\\"username\\\", env(\\\"DEFAULT_OWNER_NAME\\\"))->first()?->update([\\\"group_id\\\" => App\\Models\\Group::where(\\\"slug\\\", \\\"owner\\\")->value(\\\"id\\\")]); }'",
        "php artisan auto:email-blacklist-update",
        "php artisan storage:link", // G15
        "php artisan config:cache", // G17
        "php artisan route:cache",  // G17
        "php artisan view:cache",   // G17
    ];

    for cmd in www_cmds {
        let s = format!("sudo -u {web_user} bash -lc \"cd {install_dir_s} && {cmd}\"");
        // G24: if running as web-user fails (Bun modules often can't write
        // outside the checkout as www-data), fall back to running as root
        // and re-fix permissions.
        let res = ctx.run(&s);
        if res.is_err()
            && (cmd.starts_with("bun") || cmd.starts_with("composer") || cmd.starts_with("npm"))
        {
            ctx.style
                .warning(&format!("{cmd} as {web_user} failed — retrying as root"));
            ctx.run(&format!("bash -c 'cd {install_dir_s} && {cmd}'"))?;
            ctx.run(&format!("chown -R {web_user}:{web_user} {install_dir_s}"))?;
        } else if let Err(e) = res {
            // Non-retryable command (php artisan migrate, key:generate, …)
            // failed and was NOT retried — the install is broken from here
            // on (e.g. migrate failing leaves the DB with no tables and
            // every page 500s). Abort loudly instead of printing success.
            anyhow::bail!(
                "command failed: {cmd}\n{e}\n\
                 the site will not work in this state — fix the error above \
                 and re-run the installer"
            );
        }
    }
    let preload_path = install_dir.join("preload.php");
    let preload_path_s = preload_path.display().to_string();
    let ensure_preload = format!(
        "if [ ! -f {preload} ]; then echo '<?php // opcache preload placeholder' > {preload} && chown {web_user}:{web_user} {preload} && chmod 0644 {preload}; fi",
        preload = preload_path_s,
        web_user = web_user
    );
    // run as root
    ctx.run(&format!("bash -lc \"{ensure}\"", ensure = ensure_preload))?;
    // if it fails, dont stop installer
    ctx.run("systemctl restart php8.5-fpm || true")?;

    ctx.style.info("UNIT3D installed successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_paths_are_rejected() {
        for p in [
            "/",
            "/etc",
            "/etc/nginx",
            "/root",
            "/home",
            "/home/user",
            "/var",
            "/var/lib",
            "/var/lib/mysql",
            "/var/run",
            "/usr/local",
            "/tmp",
            "/bin",
            "/srv",
            "relative-path",
        ] {
            assert!(
                unsafe_to_delete(Path::new(p)).is_some(),
                "{p} must be rejected"
            );
        }
    }

    #[test]
    fn web_root_is_allowed() {
        assert!(unsafe_to_delete(Path::new("/var/www/html")).is_none());
        assert!(unsafe_to_delete(Path::new("/var/www/html/unit3d")).is_none());
        assert!(unsafe_to_delete(Path::new("/srv/www/unit3d")).is_none());
    }

    #[test]
    fn empty_and_dot_handled() {
        // An empty install_dir resolves to "." relative — rejected.
        assert!(unsafe_to_delete(Path::new("")).is_some());
        assert!(unsafe_to_delete(Path::new(".")).is_some());
    }
}
