<div align="center">

# UNIT3D Community Edition Installer

**A one-shot, unattended installer for [UNIT3D-Community-Edition](https://github.com/HDInnovations/UNIT3D-Community-Edition), rewritten from PHP to a single static Rust binary.**

**⚠️Is currently in a working state if there are any issues let us know⚠️**

[![CI](https://github.com/InfinityHD-Net/UNIT3D-Installer/actions/workflows/ci.yml/badge.svg)](https://github.com/InfinityHD-Net/UNIT3D-Installer/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)
![License](https://img.shields.io/github/license/InfinityHD-Net/UNIT3D-Installer)

</div>

---

> **⚠️ Important:** This installer is designed for a **fresh server** running a *supported OS* and nothing else. It installs, configures, and hardens the entire stack end-to-end. Do **not** run it against an existing production machine — it will overwrite configuration files in place.

---

## Table of Contents

- [Why Rust?](#why-rust)
- [Features](#features)
- [Requirements](#requirements)
- [Quick Start](#quick-start)
- [Manual Installation](#manual-installation)
- [Configuration](#configuration)
  - [CLI Options](#cli-options)
  - [TOML Reference](#toml-reference)
- [What It Installs](#what-it-installs)
- [Credentials](#credentials)
- [Building From Source](#building-from-source)
- [Testing & CI](#testing--ci)
- [Project Layout](#project-layout)
- [Frequently Asked Questions](#frequently-asked-questions)
- [License](#license)

---

## Why Rust?

The original installer was a PHP console application packaged as a PHAR with Box. It required PHP, Composer, and a pile of dependencies on the target server **before** installation could even begin.

The Rust rewrite compiles to a **single static binary** with zero runtime dependencies:

- **No toolchain prerequisites** — the target server only needs `curl` and `tar`.
- **One-line bootstrap** via `curl | sh` that pulls the latest release binary.
- **Typed, validated configuration** instead of error-prone PHP arrays.
- **Deterministic, testable pipeline** — every step can be exercised with a mocked executor in CI.
- **Faster and safer** than an interpreted installer touching a production box.

## Features

- **Ubuntu LTS only** (20.04, 22.04, 24.04, 26.04) with root privilege checks.
- **Database choice**: MySQL, MariaDB, or PostgreSQL — auto-provisioned and secured with randomized credentials.
- **PHP 8.5** via the Ondrej PPA with opcache/JIT tuning, plus Node.js 24 LTS, Bun, and `laravel-echo-server`.
- **Redis over unix sockets** for sub-millisecond IPC, with RAM-bounded LRU eviction (`maxmemory`).
- **Nginx** site configuration with security headers, gzip, static-asset caching, `.env`/`.git` protection, and a `/socket.io` proxy for the chat server on the configured echo port.
- **Let's Encrypt SSL** via `certbot` (automatic when `ssl = true`).
- **Meilisearch** installed as a hardened systemd service, with `scout` index settings and import.
- **Queue worker** managed by Supervisor (`queue:work redis --sleep=3 --tries=3 --max-time=3600`).
- **Idempotent `crontab` merge** for `artisan schedule:run`.
- **Laravel post-install caching** (`config:cache`, `route:cache`, `view:cache`, `storage:link`).
- **Dry-run mode** that prints the entire plan without touching the system.

## Requirements

| Requirement | Value |
| --- | --- |
| OS | Ubuntu 20.04 / 22.04 / 24.04 / 26.04 LTS |
| Privileges | `root` (or `sudo`) |
| Network | A valid domain with an `A` record (and `CNAME` for `www`) pointing at the server |
| Memory | 4 GB+ recommended (Redis, PHP-FPM, and Meilisearch all run concurrently) |
| Tooling | `curl` and `tar` (the bootstrap installs them if missing) |

## Quick Start

On a fresh server, with DNS already pointing at its IP:

```bash
curl -sSL https://raw.githubusercontent.com/InfinityHD-Net/UNIT3D-Installer/master/install.sh | sudo bash
```

The bootstrap downloads the latest static `unit3d-installer` binary from the GitHub Releases page, installs it to `/usr/local/bin`, and launches it.

To pass a configuration file through the bootstrap:

```bash
curl -sSL https://raw.githubusercontent.com/InfinityHD-Net/UNIT3D-Installer/master/install.sh | \
  sudo bash -s -- --config /path/to/unit3d-installer.toml
```

## Manual Installation

```bash
sudo apt -y install git
git clone https://github.com/InfinityHD-Net/UNIT3D-Installer.git installer
cd installer
sudo ./install.sh
```

## Configuration

The installer works in one of two ways:

1. **Interactive (default)** — run without `--config` and answer the prompts. Every setting (server name, domain, owner, database, mail, chat port, API keys) is asked one at a time, with sensible pre-filled defaults.
2. **Config file** — pass a [TOML](https://toml.io) file with `--config` to pre-fill answers, then skip (or keep) the prompts. Every field is optional; anything omitted falls back to a baked-in default. Copy [`unit3d-installer.example.toml`](unit3d-installer.example.toml) and edit it to your needs.

```bash
sudo ./install.sh --config /path/to/unit3d-installer.toml
```

> **Note:** if the file you pass via `--config` is **empty** (only comments or whitespace), the installer **refuses to run** rather than silently proceeding with all-default settings. Either fill the file in or drop `--config` and answer the questions interactively.

To preview the exact commands and files the installer will produce **without changing anything**:

```bash
sudo ./install.sh --dry-run --non-interactive --config unit3d-installer.example.toml
```

### CLI Options

```
Usage: unit3d-installer [OPTIONS]

Options:
  -c, --config <FILE>      Path to a TOML configuration file
      --non-interactive    Skip all prompts (requires a complete config)
      --dry-run            Print the plan without touching the system
  -v, --verbosity <COUNT>  Increase logging (-v info, -vv debug, -vvv trace)
  -h, --help               Print help
  -V, --version            Print version
```

### TOML Reference

```toml
# =============================================================================
# unit3d-installer.toml — every section is optional.
# =============================================================================

[unit3d]
min_php_version = "8.5"              # Minimum PHP required on the box
repository      = "https://github.com/HDInnovations/UNIT3D-Community-Edition.git"
tag             = "v9.2.0"           # Tag/branch to checkout (pinned clone)

[app]
server_name    = "tracker"           # Server display name
hostname       = "tracker.example.com"  # Public domain (A record -> server IP)
ip             = ""                  # Leave blank to auto-detect
ssl            = true                # Enable Let's Encrypt
branch         = "master"            # UNIT3D branch (usually master)

owner          = "UNIT3D"            # Default admin username
owner_email    = "admin@tracker.example.com"
password       = ""                  # Auto-generated if blank

db_driver      = "MariaDb"           # "MariaDb" | "MySql" | "Postgres"
db             = "unit3d"            # Database name
dbuser         = "unit3d"            # Database user
dbpass         = ""                  # Auto-generated if blank
dbrootpass     = ""                  # DB root password (required non-interactively)

echo_port      = 8443                # Laravel Echo Server port

mail_driver    = "smtp"
mail_host      = ""
mail_port      = "587"
mail_username  = ""
mail_password  = ""
mail_from_name = "UNIT3D"

tmdb_key       = ""                  # TMDB API key (optional)
meilisearch_key = ""                 # Auto-generated if blank

[os.ubuntu]
pkg_manager                = "apt-get"
web_user                   = "www-data"
install_dir                = "/var/www/html"
nginx_sites_available_path = "/etc/nginx/sites-available"

# Omit [os.ubuntu.software] to use the built-in package list and PHP
# extension set, or override with the full lists:
# [os.ubuntu.software]
# packages = { "nginx" = "Web Server", ... }
# php_extensions = ["php8.5-fpm", ...]
```

## What It Installs

The pipeline mirrors the classic installer flow, in this order:

| # | Step | What happens |
| --- | --- | --- |
| 1 | **Policies** | Verifies root, supported Ubuntu release, no existing install, PHP version |
| 2 | **Server** | Hostname, locale, timezone, swap, security hardening, SSH |
| 3 | **Redis** | Unix socket + group permissions, `maxmemory` cap, LRU policy, restart |
| 4 | **Prerequisites** | PPA, apt packages, Node 24 LTS, Bun, `laravel-echo-server`, UFW rules |
| 5 | **Database** | Installs & secures MySQL/MariaDB/PostgreSQL, creates DB + user |
| 6 | **PHP** | PHP-FPM, opcache/JIT tuning, `php.ini` hardening |
| 7 | **Nginx** | Site config, security headers, `/socket.io` proxy, certbot SSL |
| 8 | **UNIT3D** | Tag-pinned clone, `.env`, permissions, cron, Composer+Bun, Supervisor, Echo Server, migrations, caching |
| 9 | **Meilisearch** | Systemd service, master key, `scout:sync-index-settings`, `scout:import` |
| 10 | **Credentials** | Writes credentials file, prints final summary |

## Credentials

At the end of a successful run, the installer writes a credentials file to:

```
/root/unit3d-credentials.txt
```

It contains the admin login, database passwords, and the Meilisearch key.

> **🔒 Security:** the file is only readable by `root`. **Save its contents somewhere safe and delete the file** — it cannot be recovered later.

## Building From Source

Requires Rust **1.85+** (edition 2024).

```bash
cargo build --release
# binary at target/release/unit3d-installer
```

The release profile uses thin LTO, `codegen-units = 1`, and symbol stripping for a small (~2 MB) optimized binary. See [`Cargo.toml`](Cargo.toml).

## Testing & CI

The project ships a layered test suite:

```bash
cargo test                        # unit + integration + snapshot tests
cargo clippy --all-targets -- -D warnings   # lints (warnings are errors)
cargo fmt --check                 # formatting
```

- **Unit tests** — config defaults, password generation, OS detection, policy helpers.
- **Integration tests** — drive real steps against a mocked shell executor and assert emitted commands.
- **Snapshot tests** — golden files lock every generated template (`.env`, nginx site, php-fpm, supervisor, Meilisearch, echo server, credentials).
- **CLI smoke tests** — run the compiled binary in dry-run mode and assert the full pipeline.

[GitHub Actions](.github/workflows/ci.yml) runs formatting, clippy, tests on stable **and** the MSRV (1.85), plus a release build with a dry-run smoke test on every push and PR.

## Project Layout

```
├── .github/workflows/ci.yml   # CI pipeline
├── src/
│   ├── main.rs                # Binary entry point (thin wrapper)
│   ├── lib.rs                 # run(), tracing, banner
│   ├── cli.rs                 # clap CLI definition
│   ├── config/                # Typed TOML config + defaults
│   ├── steps/                 # The 10-step install pipeline
│   ├── resources/             # Askama template structs
│   ├── process/               # Exec trait (real / dry-run / mock)
│   ├── io/                    # Prompts + styled output
│   ├── system/                # OS detection, memory, network, privileges
│   ├── password.rs            # Random password / key generation
│   └── credentials.rs         # Credentials output
├── templates/                 # Askama templates (compiled at build time)
├── tests/                     # Integration + snapshot tests
└── install.sh                 # curl|sh bootstrap
```

## Frequently Asked Questions

**Do I need PHP or Composer installed first?**
No — that is the whole point of the Rust rewrite. The binary is self-contained; only `curl` and `tar` are needed to bootstrap.

**Can I re-run it on a server that already has UNIT3D?**
No. The installer refuses to run if an existing installation is detected in the install directory.

**What happens if my domain isn't pointed at the server yet?**
The installer will still run, but certbot (Let's Encrypt) will fail to validate. Point DNS at the server before installing.

**Can I use a different PHP version?**
The default is PHP 8.5 from the Ondrej PPA, matching the current UNIT3D requirements. Adjust `php_extensions` and `min_php_version` in the config if you override.

**How do I update UNIT3D afterwards?**
Use the standard UNIT3D update procedure (`git pull` + `composer install` + migrations) in the install directory; the installer is only for fresh provisioning.

## License

[MIT](LICENSE) — see the [LICENSE](LICENSE) file for details. A big thanks to the [UNIT3D-Community-Edition](https://github.com/HDInnovations/UNIT3D-Community-Edition) project and all its contributors.

---

## Contributing

Bug reports, feature requests, and pull requests are welcome. Please open an [issue](https://github.com/InfinityHD-Net/UNIT3D-Installer/issues/new) or submit a PR — keep changes lint- and test-clean (`cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`).
