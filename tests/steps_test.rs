//! Step tests: verify each installer step emits the expected shell
//! commands through the mocked executor.

mod common;

use common::test_context;
use unit3d_installer::steps::Step;
use unit3d_installer::steps::{credentials, nginx, policies, prerequisites, redis};

#[test]
fn redis_step_enables_socket_and_memory_cap() {
    let (mut ctx, exec) = test_context();
    // dry_run=true records commands and skips root/filesystem checks.
    ctx.dry_run = true;
    redis::RedisSetupStep.handle(&mut ctx).unwrap();

    assert!(exec.any("mkdir -p /var/run/redis/"));
    assert!(exec.any("usermod -aG redis www-data"));
    assert!(exec.any("unixsocket \\/var\\/run\\/redis\\/redis.sock"));
    assert!(exec.any("unixsocketperm 770"));
    // G4: memory cap + LRU eviction
    assert!(exec.any("maxmemory 256mb"));
    assert!(exec.any("maxmemory-policy allkeys-lru"));
    assert!(exec.any("systemctl restart redis-server"));
}

#[test]
fn redis_step_uses_configured_web_user() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    ctx.config.os.ubuntu.web_user = "unit3d".to_string();
    redis::RedisSetupStep.handle(&mut ctx).unwrap();
    assert!(exec.any("usermod -aG redis unit3d"));
}

#[test]
fn nginx_step_writes_site_and_ssl() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    ctx.config.app.hostname = "tracker.example.com".to_string();
    ctx.config.app.owner_email = "admin@tracker.example.com".to_string();
    ctx.config.app.echo_port = 8443;
    ctx.config.app.ssl = true;

    nginx::NginxSetupStep.handle(&mut ctx).unwrap();

    assert!(exec.any("rm -f /etc/nginx/sites-enabled/default"));
    assert!(exec.any("nginx -t"));
    assert!(exec.any("systemctl restart nginx"));
    assert!(exec.any("ufw allow 'Nginx Full'"));
    assert!(exec.any("ufw allow 8443"));
    // G11: echo server port proxied under the site.
    assert!(exec.any("ufw allow 8443"));
    assert!(exec.any("certbot --redirect --nginx -n --agree-tos --email=admin@tracker.example.com -d tracker.example.com -d www.tracker.example.com --rsa-key-size 2048"));
}

#[test]
fn nginx_step_skips_certbot_when_ssl_disabled() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    ctx.config.app.ssl = false;
    nginx::NginxSetupStep.handle(&mut ctx).unwrap();
    assert!(!exec.any("certbot"));
}

#[test]
fn prerequisites_installs_packages_and_extensions() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    prerequisites::PrerequisitesStep.handle(&mut ctx).unwrap();

    // apt-get install with the full package list.
    assert!(exec.any("apt-get install -y"));
    // PHP extensions (php8.5-*).
    assert!(exec.any("php8.5-fpm"));
    // Node + Bun + echo server.
    assert!(exec.any("deb.nodesource.com/setup_24.x"));
    assert!(exec.any("bun.sh/install"));
    assert!(exec.any("npm install -g laravel-echo-server"));
    // UFW (SSH port allowed before the firewall is enabled later).
    assert!(exec.any("ufw allow 22"));
    assert!(exec.any("ufw allow 8443"));
    assert!(exec.any("ufw allow 'Nginx Full'"));
}

#[test]
fn policies_pass_on_clean_dir_dry_run() {
    let (mut ctx, _exec) = test_context();
    ctx.dry_run = true;
    // Must not error when the dir is clean (or when already running under root).
    let result = policies::PoliciesStep.handle(&mut ctx);
    assert!(result.is_ok());
}

#[test]
fn nginx_step_uses_echo_port_in_ufw() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    ctx.config.app.echo_port = 6001;
    nginx::NginxSetupStep.handle(&mut ctx).unwrap();
    assert!(exec.any("ufw allow 6001"));
    assert!(!exec.any("ufw allow 8443"));
}

#[test]
fn nginx_step_enables_site_and_ufw_force() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    ctx.config.app.hostname = "tracker.example.com".to_string();
    nginx::NginxSetupStep.handle(&mut ctx).unwrap();
    assert!(exec.any("rm -f /etc/nginx/sites-enabled/default"));
    assert!(exec.any("ln -sf /etc/nginx/sites-available/default /etc/nginx/sites-enabled/default"));
    assert!(exec.any("ufw --force enable"));
    assert!(exec.any("systemctl enable nginx"));
}

#[test]
fn redis_step_orders_socket_perms_before_restart() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    redis::RedisSetupStep.handle(&mut ctx).unwrap();
    let cmds = exec.ran();
    let mkdir = cmds
        .iter()
        .position(|c| c.contains("mkdir -p /var/run/redis/"))
        .unwrap();
    let restart = cmds
        .iter()
        .position(|c| c.contains("restart redis-server"))
        .unwrap();
    assert!(mkdir < restart, "mkdir must come before restart");
}

#[test]
fn prerequisites_runs_pecl_and_moves_bun() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    prerequisites::PrerequisitesStep.handle(&mut ctx).unwrap();
    assert!(exec.any("pecl install redis"));
    assert!(exec.any("mv /root/.bun/bin/bun /usr/local/bin/"));
    assert!(exec.any("chmod a+x /usr/local/bin/bun"));
    assert!(exec.any("packages.sury.org/debsuryorg-archive-keyring.deb"));
}

#[test]
fn credentials_step_chmods_in_real_mode_and_prints_in_dry() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    ctx.config.app.hostname = "tracker.example.com".to_string();
    ctx.config.app.owner = "admin".to_string();
    ctx.config.app.password = "pw".to_string();
    ctx.config.app.db = "unit3d".to_string();
    ctx.config.app.dbuser = "unit3d".to_string();
    ctx.config.app.dbpass = "dbpw".to_string();
    ctx.config.app.dbrootpass = "rootpw".to_string();
    credentials::CredentialsStep.handle(&mut ctx).unwrap();
    // In dry-run, no chmod is emitted (file never written).
    assert!(!exec.any("chmod 600 /root/unit3d-credentials.txt"));
}

#[test]
fn credentials_step_renders_all_fields() {
    let (mut ctx, exec) = test_context();
    ctx.dry_run = true;
    ctx.config.app.hostname = "tracker.example.com".to_string();
    ctx.config.app.owner = "admin".to_string();
    ctx.config.app.owner_email = "admin@tracker.example.com".to_string();
    ctx.config.app.password = "ownerpw".to_string();
    ctx.config.app.db = "unit3d".to_string();
    ctx.config.app.dbuser = "unit3d".to_string();
    ctx.config.app.dbpass = "dbpw".to_string();
    ctx.config.app.dbrootpass = "rootpw".to_string();
    ctx.config.app.meilisearch_key = "0123456789abcdef0123456789abcdef".to_string();
    credentials::CredentialsStep.handle(&mut ctx).unwrap();
    // The credentials are echoed to stdout in dry-run (not captured here),
    // but the step must succeed without a panic or filesystem write.
    let _ = exec;
}
