use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let command = &args[3];
    let command_args = &args[4..];
    let output = std::process::Command::new(command)
        .args(command_args)
        .output()
        .with_context(|| {
            format!(
                "Tried to run '{}' with arguments {:?}",
                command, command_args
            )
        })?;

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
