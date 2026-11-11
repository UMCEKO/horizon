//! The game-owned, side-effect-free startup validation boundary.
use super::*;

impl Options {
    /// The complete game-owned command-line boundary, before leases or native hooks.
    /// A bad command line is a [`launch_api::UsageError`] (exit code 2, no crash report).
    pub(crate) fn validate(launch: LaunchArguments) -> Result<Option<Self>> {
        let Some(mut o) =
            Self::from_launch(launch).map_err(|e| launch_api::UsageError(e.to_string()))?
        else {
            return Ok(None);
        };
        // Paths given on the command line stay relative to where the game was started.
        if o.game_dir.is_some() {
            o.control = o.control.map(std::path::absolute).transpose()?;
            o.resources = std::path::absolute(&o.resources)?;
            o.report = o.report.map(std::path::absolute).transpose()?;
        }
        #[cfg(not(feature = "java-mods"))]
        if o.java_mods.is_some() || std::env::var_os("HORIZON_JAVA_RUN").is_some() {
            return Err(launch_api::UsageError(
                "this Horizon was built without Java mods support; use a build with them".into(),
            )
            .into());
        }
        // Refused here, before the account's file is used up or anything loads.
        if let Some(path) = &o.control {
            horizon_client::control::check_path(path).map_err(|e| {
                launch_api::UsageError(format!("--control-socket {}: {e}", path.display()))
            })?;
        }
        Ok(Some(o))
    }
}

#[cfg(all(test, not(feature = "java-mods")))]
mod tests {
    use super::*;

    #[test]
    fn a_build_without_java_mods_refuses_a_run_folder() {
        let launch = LaunchArguments::parse(
            ["--java-mods".to_string(), "x".to_string()].into_iter(),
            &std::env::current_dir().unwrap(),
            std::path::Path::new("/home"),
            None,
        )
        .unwrap();
        let error = Options::validate(launch).err().unwrap();
        let usage = error.downcast_ref::<launch_api::UsageError>().unwrap();
        assert_eq!(
            usage.0,
            "this Horizon was built without Java mods support; use a build with them"
        );
    }
}
