use crate::config::YClassConfig;
use nemclass_sdk::Target;
use std::{fs, ops::Deref};

/// GUI-side process handle: a thin wrapper over the headless [`Target`] that
/// applies the app's plugin-path policy on attach. All memory operations are
/// inherited from [`Target`] via [`Deref`].
pub struct Process(Target);

impl Process {
    /// Attaches to `pid`. If a plugin library is configured (or the default
    /// `plugin.ycpl` exists), memory goes through that managed plugin; otherwise
    /// the OS-native backend is used.
    pub fn attach(pid: u32, config: &YClassConfig) -> eyre::Result<Self> {
        let path = config
            .plugin_path
            .clone()
            .unwrap_or_else(|| "plugin.ycpl".into());
        let plugin_required = config.plugin_path.is_some();

        let meta = fs::metadata(&path);
        let target = if meta.is_ok() {
            Target::attach_managed(pid, path.as_ref())?
        } else if plugin_required {
            // A plugin path was explicitly configured but is missing.
            return Err(meta.unwrap_err().into());
        } else {
            Target::attach_pid(pid)?
        };

        Ok(Self(target))
    }
}

impl Deref for Process {
    type Target = Target;

    fn deref(&self) -> &Target {
        &self.0
    }
}
