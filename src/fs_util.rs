use std::path::Path;

/// Resolve the user's home directory, or error if unset.
pub fn home_dir() -> anyhow::Result<std::path::PathBuf> {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .map_err(|_| anyhow::anyhow!("HOME environment variable is not set"))
}

#[cfg(unix)]
pub fn set_secure_dir_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| anyhow::anyhow!("failed to chmod 700 {}: {e}", path.display()))
}

#[cfg(not(unix))]
pub fn set_secure_dir_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn set_secure_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| anyhow::anyhow!("failed to chmod 600 {}: {e}", path.display()))
}

#[cfg(not(unix))]
pub fn set_secure_file_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}
