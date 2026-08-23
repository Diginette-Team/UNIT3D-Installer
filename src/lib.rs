//! UNIT3D-Community-Edition installer (Rust port) — library crate.
//!
//! Exposes the modules so integration tests (in `tests/`) can drive the
//! step pipeline with a mocked [`Exec`]. The binary `main.rs` is a thin
//! wrapper around [`run`].

pub mod cli;
pub mod config;
pub mod credentials;
pub mod io;
pub mod password;
pub mod process;
pub mod resources;
pub mod secrets;
pub mod steps;
pub mod system;

pub use crate::process::Exec;

use anyhow::Result;
use clap::Parser;

/// Entrypoint shared by the `main` binary.
pub fn run() -> Result<()> {
    let args = cli::Args::parse();
    // If running the binary in non-interactive mode with no explicit
    // `--config`, prefer a local example TOML when present so `--non-interactive`
    // can be exercised by operators. Do NOT do this inside tests (they call
    // `Context::build` directly) — keep test behavior deterministic.
    let mut args_for_ctx = args.clone();
    if args_for_ctx.non_interactive && args_for_ctx.config.is_none() {
        if let Ok(cwd) = std::env::current_dir() {
            let example = cwd.join("unit3d-installer.example.toml");
            if example.exists() {
                args_for_ctx.config = Some(example);
            }
        }
    }

    init_tracing(args.verbosity);

    print_intro();

    let mut ctx = steps::Context::build(&args_for_ctx)?;
    let runner = steps::StepRunner;
    runner.run(&mut ctx)?;

    ctx.style.final_summary(&ctx.config);
    Ok(())
}

/// Print the ASCII banner.
fn print_intro() {
    use crate::resources::intro::IntroTemplate;
    use askama::Template;
    let tpl = IntroTemplate;
    println!("{}", tpl.render().unwrap_or_default());
}

/// Initialize `tracing` based on the CLI verbosity.
fn init_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => "unit3d_installer=warn",
        1 => "unit3d_installer=info",
        2 => "unit3d_installer=debug",
        _ => "unit3d_installer=trace,debug",
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Args;
    use clap::Parser;

    #[test]
    fn cli_parses_minimal() {
        let args = Args::parse_from(["unit3d-installer", "--non-interactive"]);
        assert!(args.non_interactive);
        assert!(!args.dry_run);
        assert!(args.config.is_none());
        assert_eq!(args.verbosity, 0);
    }

    #[test]
    fn cli_parses_config_path() {
        let args = Args::parse_from([
            "unit3d-installer",
            "--config",
            "unit3d-installer.example.toml",
            "--dry-run",
            "-vvv",
        ]);
        assert!(args.dry_run);
        assert_eq!(
            args.config.unwrap().to_str(),
            Some("unit3d-installer.example.toml")
        );
        assert_eq!(args.verbosity, 3);
    }

    #[test]
    fn cli_accepts_alias_for_non_interactive() {
        let args = Args::parse_from(["unit3d-installer", "--yes-to-all"]);
        assert!(args.non_interactive);
    }

    #[test]
    fn cli_defaults_are_safe() {
        let args = Args::parse_from(["unit3d-installer"]);
        assert!(!args.non_interactive);
        assert!(!args.dry_run);
        assert!(args.config.is_none());
        assert_eq!(args.verbosity, 0);
    }

    #[test]
    fn intro_renders_without_panic() {
        print_intro();
    }

    #[test]
    fn init_tracing_never_panics_for_any_verbosity() {
        for v in 0..=5 {
            init_tracing(v);
        }
    }
}
