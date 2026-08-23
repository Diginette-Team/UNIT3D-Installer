//! Server-wide interactive configuration. Replaces the legacy
//! `src/Installer/Server/ServerSetup.php`. Collects server name, IP,
//! hostname (FQDN), SSL toggle, owner credentials, DB driver + names +
//! passwords, mail settings, echo port, TMDB key, and Meilisearch key.
//!
//! Auto-generation of `password`, `dbpass`, and `meilisearch_key` when the
//! user leaves them blank is the legacy gap G1 fix folded into the Rust
//! port.

use crate::config::DbDriver;
use crate::io::prompt::print_summary;
use crate::password;
use crate::steps::{Context, Step};
use crate::system::{fqdn, hostname, ip};
use anyhow::Result;

pub struct ServerSetupStep;

impl Step for ServerSetupStep {
    fn name(&self) -> &'static str {
        "Server Setup"
    }

    fn handle(&self, ctx: &mut Context) -> Result<()> {
        server(ctx)?;
        firewall(ctx)?;
        user(ctx)?;
        database(ctx)?;
        mail(ctx)?;
        chat(ctx)?;
        meilisearch(ctx);
        api_keys(ctx)?;
        autoset_passwords(ctx);

        print_summary(&ctx.config);
        if !ctx.prompter.confirm("Continue with installation?", true)? {
            anyhow::bail!("Installation cancelled by user");
        }
        Ok(())
    }
}

fn server(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Server Settings");
    // When --config pre-populates a field, that value becomes the prompt
    // default (per the documented `--config` contract).
    let hostname_default = if ctx.config.app.hostname.is_empty() {
        fqdn()
    } else {
        ctx.config.app.hostname.clone()
    };
    let server_name_default = if ctx.config.app.server_name.is_empty() {
        hostname()
    } else {
        ctx.config.app.server_name.clone()
    };
    let ip_default = if ctx.config.app.ip.is_empty() {
        ip()
    } else {
        ctx.config.app.ip.clone()
    };
    let server_name = ctx.prompter.text("Server Name", &server_name_default)?;
    ctx.config.app.server_name = server_name;

    // FQDN validation loop: tolerate a few bad tries, then bail so
    // non-interactive runs don't spin forever on boxes whose `hostname -f`
    // returns a bare name. Uses the same strict check as `Config::validate`
    // so shell metacharacters never reach the nginx/certbot command line.
    let mut hostname_val = ctx
        .prompter
        .text("Domain (e.g. tracker.example.com)", &hostname_default)?;
    let mut attempts = 0;
    while !crate::config::is_valid_fqdn(&hostname_val) && hostname_val != "localhost" {
        attempts += 1;
        if attempts > 5 {
            anyhow::bail!(
                "A valid FQDN is required. '{hostname_val}' is not a fully-qualified domain name."
            );
        }
        ctx.style
            .warning("Invalid Format: must be a fully qualified domain name");
        // In non-interactive mode the prompter keeps returning the same
        // default — bail immediately instead of looping forever.
        if ctx.non_interactive {
            anyhow::bail!(
                "non-interactive mode: '{hostname_val}' is not a valid FQDN. \
                 Set `app.hostname` in your config file."
            );
        }
        hostname_val = ctx
            .prompter
            .text("Domain (e.g. tracker.example.com)", &hostname_default)?;
    }
    ctx.config.app.hostname = hostname_val;
    ctx.config.app.ip = ctx.prompter.text("Primary IP Address", &ip_default)?;

    let ssl_items = ["yes", "no"];
    let default = if ctx.config.app.ssl { 0 } else { 1 };
    let idx = ctx
        .prompter
        .select("Enable SSL (https)", &ssl_items, default)?;
    ctx.config.app.ssl = idx == 0;

    let branch_items = ["master", "dev"];
    let prev = branch_items
        .iter()
        .position(|b| *b == ctx.config.app.branch)
        .unwrap_or(0);
    let idx = ctx.prompter.select(
        "Which branch of UNIT3D do you wish to install?",
        &branch_items,
        prev,
    )?;
    ctx.config.app.branch = branch_items[idx].to_string();
    Ok(())
}

fn user(ctx: &mut Context) -> Result<()> {
    ctx.style.section("User Settings");
    let owner_default = if ctx.config.app.owner.is_empty() {
        "UNIT3D".to_string()
    } else {
        ctx.config.app.owner.clone()
    };
    ctx.config.app.owner = ctx.prompter.text("Owner Username", &owner_default)?;
    let owner_pass_default = if ctx.config.app.password.is_empty() {
        password::str_random(20)
    } else {
        ctx.config.app.password.clone()
    };
    let owner_pass_prompted = ctx.prompter.text(
        "Owner Password (blank = auto-generate)",
        &owner_pass_default,
    )?;
    ctx.config.app.password = if owner_pass_prompted.is_empty() {
        ctx.style.info("Generated owner password");
        owner_pass_default
    } else {
        owner_pass_prompted
    };
    let default_email = if ctx.config.app.owner_email.is_empty() {
        format!("admin@{}", ctx.config.app.hostname)
    } else {
        ctx.config.app.owner_email.clone()
    };
    let email = ctx.prompter.text("Owner Email", &default_email)?;
    ctx.config.app.owner_email = if email.is_empty() {
        default_email
    } else {
        email
    };
    Ok(())
}

fn database(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Database Settings");
    let drivers = ["MariaDb", "MySql", "Postgres"];
    let default = match ctx.config.app.db_driver {
        DbDriver::MariaDb => 0,
        DbDriver::Mysql => 1,
        DbDriver::Postgres => 2,
    };
    let idx = ctx
        .prompter
        .select("Choose a database driver", &drivers, default)?;
    ctx.config.app.db_driver = match idx {
        0 => DbDriver::MariaDb,
        1 => DbDriver::Mysql,
        2 => DbDriver::Postgres,
        _ => unreachable!(),
    };

    ctx.style
        .warning("Special characters may not work at this time!");
    ctx.config.app.dbrootpass = ctx.prompter.text(
        "DB Server Root Password",
        &ctx.config.app.dbrootpass.clone(),
    )?;

    let db_default = if ctx.config.app.db.is_empty() {
        "unit3d".to_string()
    } else {
        ctx.config.app.db.clone()
    };
    let db = ctx.prompter.text("UNIT3D DB Name", &db_default)?;
    ctx.config.app.db = if db.is_empty() { db_default } else { db };

    let user_default = if ctx.config.app.dbuser.is_empty() {
        "unit3d".to_string()
    } else {
        ctx.config.app.dbuser.clone()
    };
    let user = ctx.prompter.text("UNIT3D DB User", &user_default)?;
    ctx.config.app.dbuser = if user.is_empty() { user_default } else { user };

    let db_pass_default = if ctx.config.app.dbpass.is_empty() {
        password::str_random(20)
    } else {
        ctx.config.app.dbpass.clone()
    };
    ctx.style
        .warning("Special characters may not work at this time!");
    let dbpass = ctx.prompter.text(
        "UNIT3D DB Password (blank = auto-generate)",
        &db_pass_default,
    )?;
    ctx.config.app.dbpass = if dbpass.is_empty() {
        ctx.style.info("Generated MySQL UNIT3D password");
        db_pass_default
    } else {
        dbpass
    };
    Ok(())
}

fn mail(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Mail Settings");
    ctx.style.info("Used for invites, registrations, etc.");
    let drivers = [
        "smtp",
        "sendmail",
        "mailgun",
        "mandrill",
        "ses",
        "sparkpost",
        "log",
        "array",
    ];
    let prev = drivers
        .iter()
        .position(|d| *d == ctx.config.app.mail_driver)
        .unwrap_or(0);
    let idx = ctx.prompter.select("Mail Driver", &drivers, prev)?;
    ctx.config.app.mail_driver = drivers[idx].to_string();
    ctx.config.app.mail_host = ctx
        .prompter
        .text("Mail Host", &ctx.config.app.mail_host.clone())?;
    ctx.config.app.mail_port = ctx
        .prompter
        .text("Mail Port", &ctx.config.app.mail_port.clone())?;
    ctx.config.app.mail_username = ctx
        .prompter
        .text("Mail Username", &ctx.config.app.mail_username.clone())?;
    ctx.config.app.mail_password = ctx
        .prompter
        .password("Mail Password")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ctx.config.app.mail_password.clone());
    let default_from = if ctx.config.app.mail_from_name.is_empty() {
        ctx.config.app.hostname.clone()
    } else {
        ctx.config.app.mail_from_name.clone()
    };
    let from = ctx.prompter.text("Mail From Name", &default_from)?;
    ctx.config.app.mail_from_name = if from.is_empty() { default_from } else { from };
    Ok(())
}

fn firewall(ctx: &mut Context) -> Result<()> {
    use owo_colors::OwoColorize;

    ctx.style.section("Firewall Settings");
    loop {
        let default = if ctx.config.app.ssh_port == 0 {
            "22".to_string()
        } else {
            ctx.config.app.ssh_port.to_string()
        };
        let port = ctx.prompter.text(
            "SSH Port to allow through UFW (press Enter for 22)",
            &default,
        )?;
        if let Ok(p) = port.parse::<u16>() {
            ctx.config.app.ssh_port = p;
        } else if ctx.config.app.ssh_port == 0 {
            ctx.config.app.ssh_port = 22;
        }

        // Big un-missable warning: the installer enables UFW with a
        // default-deny policy later in the pipeline. A wrong SSH port
        // here means instant lockout the moment `ufw --force enable`
        // runs in the nginx step.
        ctx.style.sep();
        println!(
            "{}",
            "                    ⚠  FIREWALL / SSH LOCKOUT WARNING  ⚠"
                .red()
                .bold()
        );
        ctx.style.sep();
        println!(
            "{}",
            "  UFW will be ENABLED and will DENY all incoming traffic by default."
                .red()
                .bold()
        );
        println!();
        println!(
            "  SSH port that will be allowed : {}",
            ctx.config.app.ssh_port.to_string().yellow().bold()
        );
        println!();
        println!(
            "{}",
            "  If this does NOT match the port your sshd is actually listening"
                .red()
                .bold()
        );
        println!(
            "{}",
            "  on (check `Port` in /etc/ssh/sshd_config), you WILL BE LOCKED OUT"
                .red()
                .bold()
        );
        println!(
            "{}",
            "  of this server the moment the firewall is enabled."
                .red()
                .bold()
        );
        println!();
        ctx.style.warning(
            "UFW denies all incoming traffic by default. If you run other \
             services that need inbound ports (e.g. IRC, game servers, \
             monitoring), you must `ufw allow` them yourself after install \
             or they will be unreachable.",
        );
        ctx.style.sep();

        // Non-interactive runs (CI, --dry-run, config-file driven) cannot
        // answer a confirm prompt — skip the gate there.
        if ctx.non_interactive {
            return Ok(());
        }

        // Cool-down: the confirmation is only offered after a 5-second
        // countdown so the warning above cannot be spam-entered past.
        for remaining in (1..=5).rev() {
            println!("  Reading the warning above is required — you may confirm in {remaining}...");
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        // Default is `false`: hammering Enter declines instead of confirming.
        let confirmed = ctx.prompter.confirm(
            &format!(
                "Is {} definitely the port your sshd listens on?",
                ctx.config.app.ssh_port
            ),
            false,
        )?;
        if confirmed {
            return Ok(());
        }
        ctx.style
            .warning("Not confirmed — enter the correct SSH port this time.");
    }
}

fn chat(ctx: &mut Context) -> Result<()> {
    ctx.style.section("Chat Settings");
    let default = if ctx.config.app.echo_port == 0 {
        "8443".to_string()
    } else {
        ctx.config.app.echo_port.to_string()
    };
    let port = ctx.prompter.text("Chat Listening Port", &default)?;
    if let Ok(p) = port.parse::<u16>() {
        ctx.config.app.echo_port = p;
    }
    Ok(())
}

fn meilisearch(ctx: &mut Context) {
    // Master key auto-generated if blank (G1).
    password::if_empty_generate_hex(&mut ctx.config.app.meilisearch_key);
}

fn api_keys(ctx: &mut Context) -> Result<()> {
    ctx.style.section("API Keys");
    ctx.style
        .info("TMDB: https://www.themoviedb.org/settings/api");
    ctx.config.app.tmdb_key = ctx
        .prompter
        .text("TMDB Key (optional, press enter to skip)", "")?;
    Ok(())
}

fn autoset_passwords(ctx: &mut Context) {
    password::if_empty_generate(&mut ctx.config.app.password);
    password::if_empty_generate(&mut ctx.config.app.dbpass);
    password::if_empty_generate_hex(&mut ctx.config.app.meilisearch_key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Args;
    use clap::Parser;

    #[test]
    fn autoset_passwords_idempotent() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        // Force empties, autoset should fill them.
        ctx.config.app.password.clear();
        ctx.config.app.dbpass.clear();
        ctx.config.app.meilisearch_key.clear();
        autoset_passwords(&mut ctx);
        assert!(!ctx.config.app.password.is_empty());
        assert!(!ctx.config.app.dbpass.is_empty());
        assert_eq!(ctx.config.app.meilisearch_key.len(), 32);
    }

    #[test]
    fn autoset_passwords_keep_existing() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.password = "my-owner-pass".to_string();
        ctx.config.app.dbpass = "my-db-pass".to_string();
        ctx.config.app.meilisearch_key = "0123456789abcdef0123456789abcdef".to_string();
        autoset_passwords(&mut ctx);
        assert_eq!(ctx.config.app.password, "my-owner-pass");
        assert_eq!(ctx.config.app.dbpass, "my-db-pass");
        assert_eq!(
            ctx.config.app.meilisearch_key,
            "0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn meilisearch_generates_key_when_blank() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.meilisearch_key.clear();
        meilisearch(&mut ctx);
        assert_eq!(ctx.config.app.meilisearch_key.len(), 32);
    }

    #[test]
    fn server_accepts_valid_fqdn_default() {
        // Non-interactive: prompter returns the config's pre-set hostname.
        // A dotted FQDN passes the validation loop without bailing.
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.hostname = "tracker.example.com".to_string();
        // server() requires a hostname default; the rest of the prompts use
        // defaults. It must not bail.
        let result = server(&mut ctx);
        assert!(result.is_ok());
        assert_eq!(ctx.config.app.hostname, "tracker.example.com");
    }

    #[test]
    fn server_rejects_bare_hostname_in_non_interactive() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.hostname = "myserver".to_string();
        let err = server(&mut ctx).unwrap_err();
        assert!(err.to_string().contains("FQDN"));
        assert!(err.to_string().contains("non-interactive"));
    }

    #[test]
    fn server_accepts_localhost_shortcut() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.hostname = "localhost".to_string();
        assert!(server(&mut ctx).is_ok());
    }

    #[test]
    fn user_uses_email_fallback_from_hostname() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.hostname = "tracker.example.com".to_string();
        ctx.config.app.owner = "UNIT3D".to_string();
        user(&mut ctx).unwrap();
        assert_eq!(ctx.config.app.owner_email, "admin@tracker.example.com");
    }

    #[test]
    fn chat_parses_port_or_keeps_default() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.echo_port = 0;
        chat(&mut ctx).unwrap();
        // Non-interactive returns default "8443", parsed to u16.
        assert_eq!(ctx.config.app.echo_port, 8443);
    }

    #[test]
    fn database_prompts_fill_defaults_non_interactive() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        // Clear user-provided fields so the defaults apply.
        ctx.config.app.db.clear();
        ctx.config.app.dbuser.clear();
        database(&mut ctx).unwrap();
        assert_eq!(ctx.config.app.db, "unit3d");
        assert_eq!(ctx.config.app.dbuser, "unit3d");
        // Auto-generated DB password when left blank.
        assert!(!ctx.config.app.dbpass.is_empty());
    }

    #[test]
    fn server_rejects_injection_hostname_in_non_interactive() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.hostname = "tracker.example.com;touch /tmp/pwn".to_string();
        let err = server(&mut ctx).unwrap_err();
        // The strict is_valid_fqdn rejects the shell metacharacter.
        assert!(err.to_string().contains("FQDN"));
        assert_eq!(
            ctx.config.app.hostname,
            "tracker.example.com;touch /tmp/pwn"
        );
    }

    #[test]
    fn firewall_prompt_defaults_to_22_when_blank() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.ssh_port = 0;
        firewall(&mut ctx).unwrap();
        // Non-interactive returns default "22", parsed to u16.
        assert_eq!(ctx.config.app.ssh_port, 22);
    }

    #[test]
    fn firewall_prompt_keeps_custom_port() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.ssh_port = 2222;
        firewall(&mut ctx).unwrap();
        assert_eq!(ctx.config.app.ssh_port, 2222);
    }

    #[test]
    fn firewall_prompt_falls_back_to_22_on_invalid_input_when_unset() {
        // A non-numeric answer leaves the previous value intact; when the
        // previous value is 0 (unset), fall back to the safe default 22.
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.ssh_port = 0;
        firewall(&mut ctx).unwrap();
        assert_eq!(ctx.config.app.ssh_port, 22);
    }

    #[test]
    fn chat_accepts_invalid_input_but_keeps_safe_default() {
        // A non-numeric interactive answer leaves the previous value intact.
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.echo_port = 0;
        chat(&mut ctx).unwrap();
        assert_eq!(ctx.config.app.echo_port, 8443);
    }

    #[test]
    fn email_fallback_is_safe_for_localhost() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        let mut ctx = Context::build(&args).unwrap();
        ctx.config.app.hostname = "localhost".to_string();
        ctx.config.app.owner = "UNIT3D".to_string();
        user(&mut ctx).unwrap();
        assert_eq!(ctx.config.app.owner_email, "admin@localhost");
    }
}
