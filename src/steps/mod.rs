//! Installation pipeline: each phase is a [`Step`] run sequentially by
//! [`StepRunner`]. The structure mirrors the legacy `src/Commands/
//! InstallCommand.php` `$steps` array.

pub mod context;
pub mod credentials;
pub mod database;
pub mod meilisearch;
pub mod nginx;
pub mod php;
pub mod policies;
pub mod prerequisites;
pub mod redis;
pub mod server;
pub mod unit3d;

pub use context::Context;
pub use context::Steps;

use crate::io::Style;
use anyhow::Result;

/// One unit of work in the install pipeline.
pub trait Step: Send + Sync {
    /// Human-readable header shown before the step runs.
    fn name(&self) -> &'static str;

    /// Execute the step. May mutate [`Context`] (e.g. fill prompts) and
    /// shell out via `ctx.exec`.
    fn handle(&self, ctx: &mut Context) -> Result<()>;
}

/// Runs an ordered list of steps, printing headers and `✔ DONE` markers
/// between each (mirrors the legacy `head()/done()` loop).
#[derive(Default)]
pub struct StepRunner;

impl StepRunner {
    pub fn run(&self, ctx: &mut Context) -> Result<()> {
        let style = Style;
        for step in Steps::ordered() {
            style.head(step.name());
            step.handle(ctx)?;
            style.ok();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_ordered_is_complete_and_in_order() {
        let names: Vec<&str> = Steps::ordered().iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            [
                "Server Setup",
                "Prerequisites",
                "Validating Installer Policies",
                "Redis Setup & Configurations",
                "Configuring & Securing Database",
                "PHP & PHP-FPM Configuration",
                "Nginx Setup & Configurations",
                "UNIT3D Settings and Configuration",
                "Meilisearch Setup & Configuration",
                "Finalizing Install (credentials file)",
            ]
        );
    }

    #[test]
    fn ordered_steps_are_all_send_sync() {
        // The `Vec<Box<dyn Step>>` must satisfy Send + Sync (Step: Send + Sync).
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Vec<Box<dyn Step>>>();
    }
}
