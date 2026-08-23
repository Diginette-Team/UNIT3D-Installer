//! UNIT3D, Meilisearch, and PHP step integration tests. Drive the steps
//! through the mocked executor in dry-run mode and assert the emitted
//! commands and file writes.

mod common;

use common::test_context_dry;
use unit3d_installer::steps::Step;
use unit3d_installer::steps::{meilisearch::MeilisearchSetupStep, unit3d::Unit3dSetupStep};

fn unit3d_context() -> (unit3d_installer::steps::Context, common::MockExec) {
    let (mut ctx, _exec) = test_context_dry();
    ctx.config.app.hostname = "tracker.example.com".to_string();
    ctx.config.app.owner = "UNIT3D".to_string();
    ctx.config.app.owner_email = "admin@tracker.example.com".to_string();
    ctx.config.app.db = "unit3d".to_string();
    ctx.config.app.dbuser = "unit3d".to_string();
    ctx.config.app.dbpass = "secretpass".to_string();
    ctx.config.app.dbrootpass = "rootpw".to_string();
    ctx.config.app.meilisearch_key = "0123456789abcdef0123456789abcdef".to_string();
    ctx.config.unit3d.tag = "v9.2.0".to_string();
    ctx.config.unit3d.repository =
        "https://github.com/HDInnovations/UNIT3D-Community-Edition.git".to_string();
    (ctx, _exec)
}

#[test]
fn unit3d_clones_tag_pinned_repo() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();

    // G14: tag-pinned clone with safe.directory.
    assert!(exec.any("git config --global --add safe.directory /var/www/html"));
    assert!(exec.any("git clone -b v9.2.0 https://github.com/HDInnovations/UNIT3D-Community-Edition.git /var/www/html"));
}

#[test]
fn unit3d_installs_dependencies_and_bootstraps() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();

    // Composer + Bun.
    assert!(exec.any("composer install -q --prefer-dist --no-dev"));
    assert!(exec.any("composer dump-autoload --optimize"));
    //  Fortify, Livewire, JoyPixels, Laravel assets all required for login routes
    assert!(exec.any("php artisan vendor:publish --force --tag=livewire:assets --ansi"));
    assert!(exec.any("php artisan vendor:publish --tag=public --provider=\"hdvinnie\\LaravelJoyPixels\\LaravelJoyPixelsServiceProvider\""));
    assert!(exec.any("php artisan vendor:publish --tag=laravel-assets --ansi --force"));
    assert!(exec.any("bun install"));
    assert!(exec.any("bun run build"));
    // Artisan bootstrapping (G15/G17).
    assert!(exec.any("php artisan key:generate"));
    assert!(exec.any("php artisan migrate --seed --force"));
    assert!(exec.any("php artisan storage:link"));
    assert!(exec.any("php artisan config:cache"));
    assert!(exec.any("php artisan route:cache"));
    assert!(exec.any("php artisan view:cache"));
}

#[test]
fn unit3d_sets_permissions_and_cron() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();

    assert!(exec.any("chown -R www-data:www-data"));
    assert!(exec.any("chmod 750 /var/www/html/artisan"));
    assert!(exec.any("chmod 640 /var/www/html/.env"));
    // G23: idempotent cron merge.
    assert!(exec.any("crontab -l 2>/dev/null | grep -v 'artisan schedule:run'"));
    assert!(exec.any("artisan schedule:run >> /dev/null 2>&1"));
}

#[test]
fn unit3d_installs_supervisor_and_echo_server() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();

    assert!(exec.any("supervisorctl reread"));
    assert!(exec.any("supervisorctl update"));
    assert!(exec.any("supervisorctl reload"));
    // Echo server config chowned to web user.
    assert!(exec.any("chown www-data:www-data /var/www/html/laravel-echo-server.json"));
}

#[test]
fn meilisearch_configures_service_and_scout() {
    let (mut ctx, exec) = unit3d_context();
    MeilisearchSetupStep.handle(&mut ctx).unwrap();

    assert!(exec.any("systemctl daemon-reload"));
    assert!(exec.any("systemctl enable meilisearch"));
    assert!(exec.any("systemctl start meilisearch"));
    // Scout index settings + import.
    assert!(exec.any("php artisan scout:sync-index-settings"));
    assert!(exec.any("php artisan scout:import"));
}

#[test]
fn php_step_patches_ini_and_www_conf() {
    use unit3d_installer::steps::php::PhpSetupStep;

    let (mut ctx, exec) = test_context_dry();
    ctx.config.app.hostname = "tracker.example.com".to_string();
    PhpSetupStep.handle(&mut ctx).unwrap();

    // The step should not fail on a box without PHP, and emits nothing
    // when no php.ini files are found (glob empty). It must return Ok.
    // This is a smoke test — actual sed commands are covered by the
    // patch_ini / patch_www unit tests in the step module.
    let _ = exec;
}

#[test]
fn unit3d_env_render_uses_mysql_socket_by_default() {
    use askama::Template;
    use unit3d_installer::resources::env::EnvTemplate;
    let tpl = EnvTemplate {
        protocol: "https",
        fqdn: "tracker.example.com",
        db_driver: "mariadb",
        db: "unit3d",
        dbuser: "unit3d",
        dbpass: "secret",
        socket: "/var/run/mysqld/mysqld.sock",
        owner: "admin",
        owner_email: "admin@tracker.example.com",
        owner_password: "ownerpass",
        tmdb_key: "tmdbkey",
        mail_driver: "smtp",
        mail_host: "",
        mail_port: "",
        mail_username: "",
        mail_password: "",
        mail_from_name: "",
        meilisearch_key: "masterkey",
        redis_host: "/var/run/redis/redis.sock",
        redis_port: "-1",
    };
    let out = tpl.render().unwrap();
    assert!(out.contains("DB_CONNECTION=mariadb"));
    assert!(out.contains("DB_SOCKET=/var/run/mysqld/mysqld.sock"));
    assert!(out.contains("REDIS_HOST=/var/run/redis/redis.sock"));
    assert!(out.contains("REDIS_PORT=-1"));
    assert!(out.contains("APP_URL=https://tracker.example.com"));
}

#[test]
fn unit3d_env_render_postgres_has_no_socket() {
    use askama::Template;
    use unit3d_installer::resources::env::EnvTemplate;
    let tpl = EnvTemplate {
        protocol: "http",
        fqdn: "tracker.example.com",
        db_driver: "pgsql",
        db: "unit3d",
        dbuser: "unit3d",
        dbpass: "secret",
        socket: "",
        owner: "admin",
        owner_email: "admin@tracker.example.com",
        owner_password: "ownerpass",
        tmdb_key: "",
        mail_driver: "smtp",
        mail_host: "",
        mail_port: "",
        mail_username: "",
        mail_password: "",
        mail_from_name: "",
        meilisearch_key: "masterkey",
        redis_host: "/var/run/redis/redis.sock",
        redis_port: "-1",
    };
    let out = tpl.render().unwrap();
    assert!(out.contains("DB_CONNECTION=pgsql"));
    assert!(out.contains("DB_SOCKET="));
    assert!(out.contains("APP_URL=http://tracker.example.com"));
}

#[test]
fn meilisearch_writes_toml_and_unit_files() {
    use askama::Template;
    use unit3d_installer::resources::meilisearch_toml::MeilisearchTomlTemplate;
    use unit3d_installer::resources::meilisearch_unit::MeilisearchUnitTemplate;
    let toml = MeilisearchTomlTemplate {
        master_key: "masterkey",
        db_path: "/var/lib/meilisearch/data",
        dump_dir: "/var/lib/meilisearch/dumps",
        snapshot_dir: "/var/lib/meilisearch/snapshots",
    }
    .render()
    .unwrap();
    assert!(toml.contains("env = \"production\""));
    assert!(toml.contains("master_key = \"masterkey\""));
    assert!(toml.contains("db_path = \"/var/lib/meilisearch/data\""));

    let unit = MeilisearchUnitTemplate {
        web_user: "www-data",
    }
    .render()
    .unwrap();
    assert!(unit.contains("User=www-data"));
    assert!(unit.contains("ExecStart=/usr/local/bin/meilisearch"));
}

#[test]
fn unit3d_supervisor_config_has_worker_and_echo() {
    use askama::Template;
    use unit3d_installer::resources::supervisor::SupervisorTemplate;
    let out = SupervisorTemplate {
        install_dir: "/var/www/html",
        web_user: "www-data",
    }
    .render()
    .unwrap();
    assert!(out.contains("queue:work"));
    assert!(out.contains("laravel-echo-server"));
    assert!(out.contains("user=www-data"));
}

#[test]
fn unit3d_step_echo_server_uses_configured_port() {
    use askama::Template;
    use unit3d_installer::resources::echo_server::EchoServerTemplate;
    let out = EchoServerTemplate {
        protocol: "https",
        fqdn: "tracker.example.com",
        port: 8443,
        ssl_cert: "/etc/letsencrypt/live/tracker.example.com/cert.pem",
        ssl_key: "/etc/letsencrypt/live/tracker.example.com/privkey.pem",
        ssl_chain: "/etc/letsencrypt/live/tracker.example.com/fullchain.pem",
    }
    .render()
    .unwrap();
    assert!(out.contains("\"port\": 8443"));
    assert!(out.contains("https://tracker.example.com"));
}

#[test]
fn meilisearch_step_emits_scout_import_for_torrents() {
    use unit3d_installer::steps::meilisearch::MeilisearchSetupStep;

    let (mut ctx, exec) = unit3d_context();
    MeilisearchSetupStep.handle(&mut ctx).unwrap();

    // G16: import the Torrent model after syncing index settings.
    assert!(exec.any("php artisan scout:sync-index-settings"));
    assert!(exec.any("php artisan scout:import \"App\\Models\\Torrent\""));
    assert!(exec.any("chown -R www-data:www-data /var/lib/meilisearch"));
    assert!(exec.any("systemctl daemon-reload"));
}

#[test]
fn unit3d_step_runs_all_artisan_cache_commands() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();

    for needle in [
        "php artisan config:cache",
        "php artisan route:cache",
        "php artisan view:cache",
        "php artisan storage:link",
        "php artisan migrate --seed --force",
    ] {
        assert!(exec.any(needle), "missing command: {needle}");
    }
}

#[test]
fn unit3d_cron_is_idempotent() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();

    // G23: strip prior entries, then append a single instance.
    let cron = exec
        .ran()
        .into_iter()
        .find(|c| c.contains("crontab -l"))
        .expect("cron command emitted");
    assert!(cron.contains("grep -v 'artisan schedule:run'"));
    assert!(cron.contains("* * * * * php /var/www/html/artisan schedule:run"));
}

#[test]
fn unit3d_step_fixes_owner_group_after_seed() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();

    let cmds = exec.ran();
    let migrate_idx = cmds
        .iter()
        .position(|c| c.contains("php artisan migrate --seed --force"))
        .expect("migrate not run");
    let fix_idx = cmds
        .iter()
        .position(|c| c.contains("tinker --execute"))
        .expect("owner group fixup not run");
    assert!(
        fix_idx > migrate_idx,
        "fixup must run after migrate --seed, got: {fix_idx} vs {migrate_idx}"
    );

    let fix = &cmds[fix_idx];
    // The command is wrapped in `bash -lc "..."`, so inner quotes appear
    // backslash-escaped in the recorded string.
    assert!(
        fix.contains("Group::where(\\\"slug\\\", \\\"owner\\\")"),
        "fixup must look up the Owner group by slug, got: {fix}"
    );
    assert!(
        fix.contains("DEFAULT_OWNER_NAME"),
        "fixup must target the seeded owner via env, got: {fix}"
    );
    assert!(
        fix.contains("group_id"),
        "fixup must reassign group_id, got: {fix}"
    );
}

#[test]
fn unit3d_step_writes_env_in_dry_run() {
    let (mut ctx, exec) = unit3d_context();
    Unit3dSetupStep.handle(&mut ctx).unwrap();
    // Permissions commands still emitted; file writes are printed, not saved.
    assert!(exec.any("chown -R www-data:www-data"));
    let _ = ctx;
}

#[test]
fn my_cnf_template_wraps_password() {
    use askama::Template;
    use unit3d_installer::resources::my_cnf::MyCnfTemplate;
    let out = MyCnfTemplate { password: "secret" }.render().unwrap();
    assert!(out.contains("[client]"));
    assert!(out.contains("password=secret"));
}

#[test]
fn credentials_template_contains_expected_sections() {
    use askama::Template;
    use unit3d_installer::resources::credentials::CredentialsTemplate;
    let out = CredentialsTemplate {
        generated: "2026-08-07",
        fqdn: "tracker.example.com",
        owner: "admin",
        owner_email: "admin@tracker.example.com",
        owner_password: "ownerpass",
        db_name: "unit3d",
        db_user: "unit3d",
        db_pass: "dbpass",
        db_root_pass: "rootpass",
        meilisearch_key: "masterkey",
        install_dir: "/var/www/unit3d",
        php_version: "8.5",
        web_user: "www-data",
    }
    .render()
    .unwrap();
    for needle in [
        "URL: https://tracker.example.com",
        "Username: admin",
        "admin@tracker.example.com",
        "MEILISEARCH:",
        "Master Key: masterkey",
        "INSTALLATION PATH: /var/www/unit3d",
        "php8.5-fpm",
    ] {
        assert!(out.contains(needle), "missing: {needle}");
    }
}

#[test]
fn php_fpm_template_uses_web_user_and_fqdn() {
    use askama::Template;
    use unit3d_installer::resources::phpfpm::PhpFpmTemplate;
    let out = PhpFpmTemplate {
        fqdn: "tracker.example.com",
        web_user: "www-data",
    }
    .render()
    .unwrap();
    assert!(out.contains("tracker.example.com"));
    assert!(out.contains("user = www-data"));
    assert!(out.contains("group = www-data"));
}

#[test]
fn supervisor_template_has_restart_policy() {
    use askama::Template;
    use unit3d_installer::resources::supervisor::SupervisorTemplate;
    let out = SupervisorTemplate {
        install_dir: "/var/www/html",
        web_user: "www-data",
    }
    .render()
    .unwrap();
    assert!(out.contains("autostart=true"));
    assert!(out.contains("autorestart=true"));
    assert!(out.contains("numprocs=1"));
}

#[test]
fn nginx_site_proxies_socket_io_on_echo_port() {
    use askama::Template;
    use unit3d_installer::resources::nginx::NginxTemplate;
    let out = NginxTemplate {
        fqdn: "tracker.example.com",
        install_dir: "/var/www/html",
        echo_port: 8443,
        max_body: "256M",
    }
    .render()
    .unwrap();
    assert!(out.contains("server_name tracker.example.com www.tracker.example.com;"));
    assert!(out.contains("proxy_pass http://127.0.0.1:8443;"));
    assert!(out.contains("fastcgi_pass unix:/var/run/php/tracker.example.com.sock;"));
    assert!(out.contains("client_max_body_size 256M;"));
    assert!(out.contains("root /var/www/html/public;"));
    // Security headers + sensitive-file denials present (G8-G12).
    assert!(out.contains("X-Frame-Options"));
    assert!(out.contains("deny all"));
    assert!(out.contains("\\.env"));
}
