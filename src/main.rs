use std::process::Stdio;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let command = &args[3];
    let command_args = &args[4..];
    // if temp dir exists, delete it first
    if std::path::Path::new("temp").exists() {
        std::fs::remove_dir_all("temp").context("Failed to remove directory 'temp'")?;
    }
    let temp_directory = tempfile::tempdir().context("Failed to create temporary directory")?;

    // create empty /dev/null file in temp_directory
    std::fs::create_dir_all(temp_directory.path().join("dev"))
        .context("Failed to create 'dev' directory")?;
    std::fs::File::create(temp_directory.path().join("dev/null"))
        .context("Failed to create 'dev/null' file")?;

    // copy the binary into the temp_directory
    let new_command_path = temp_directory.path().join("init");
    std::fs::copy(command, new_command_path.clone()).context(format!(
        "Failed to copy {} to '{}'",
        &command,
        &new_command_path.display()
    ))?;

    // chroot into the new directory
    std::os::unix::fs::chroot(temp_directory.path()).context("Failed to chroot")?;
    // chdir into the new directory
    std::env::set_current_dir("/").context("Failed to chdir into new root")?;

    let output = std::process::Command::new("./init")
        .args(command_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("Tried to run './init' with arguments {:?}", command_args))?;

    if output.status.success() {
        let std_out = std::str::from_utf8(&output.stdout)?;
        let std_err = std::str::from_utf8(&output.stderr)?;
        print!("{}", std_out);
        eprint!("{}", std_err);
        Ok(())
    } else {
        let exit_code = output.status.code().unwrap_or(1);
        std::process::exit(exit_code);
    }
}
