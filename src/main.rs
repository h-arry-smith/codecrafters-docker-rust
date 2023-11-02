use std::process::Stdio;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    layers: Vec<Layer>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Layer {
    digest: String,
    size: u64,
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    dbg!(&args);
    let image = &args[2];
    let command = &args[3];
    let command_args = &args[4..];

    let (image_name, version) = image.split_once("/").unwrap_or((image, "latest"));
    let client = reqwest::blocking::Client::new();
    // first, get an auth token from the registry for this image
    let url = format!(
        "https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/{}:pull",
        image_name
    );
    let auth_response = client
        .get(url)
        .send()
        .context("Failed to build auth token request")?;
    let auth_response: serde_json::Value = auth_response.json().unwrap();
    let auth_token = auth_response
        .get("access_token")
        .context("Failed to get auth token")?
        .as_str()
        .context("Failed to convert auth token to string")?;

    let temp_directory = tempfile::tempdir().context("Failed to create temporary directory")?;

    let manifest_response = client
        .get(format!(
            "https://registry-1.docker.io/v2/library/{}/manifests/{}",
            image_name, version
        ))
        .header("Authorization", format!("Bearer {}", auth_token))
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .send()
        .context("Failed to build manifest request")?;

    let manifest = manifest_response
        .json::<Manifest>()
        .context("Failed to parse manifest response")?;

    for layer in manifest.layers {
        let layer_response = client
            .get(format!(
                "https://registry-1.docker.io/v2/library/{}/blobs/{}",
                image_name, layer.digest
            ))
            .header("Authorization", format!("Bearer {}", auth_token))
            .send()
            .context("Failed to build layer request")?;

        dbg!(&layer_response);

        // get layer bytes
        let layer_bytes = layer_response
            .bytes()
            .context("Failed to get layer bytes")?;

        // Decompress the file
        let decoder = GzDecoder::new(&layer_bytes[..]);
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(temp_directory.path())
            .context("Failed to unpack layer")?;
    }

    // create empty /dev/null file in temp_directory
    std::fs::create_dir_all(temp_directory.path().join("dev"))
        .context("Failed to create 'dev' directory")?;
    std::fs::File::create(temp_directory.path().join("dev/null"))
        .context("Failed to create 'dev/null' file")?;

    // copy the binary into the temp_directory
    let new_command_path = temp_directory
        .path()
        .join(command.strip_prefix("/").unwrap());
    std::fs::copy(command, new_command_path.clone()).context(format!(
        "Failed to copy {} to '{}'",
        &command,
        &new_command_path.display()
    ))?;

    // chroot into the new directory
    std::os::unix::fs::chroot(temp_directory.path()).context("Failed to chroot")?;
    // chdir into the new directory
    std::env::set_current_dir("/").context("Failed to chdir into new root")?;

    const CLONE_NEWPID: libc::c_int = 0x20000000; // Constant value for creating a new PID namespace

    // Call the unshare system call to create a new PID namespace
    unsafe {
        if libc::unshare(CLONE_NEWPID) != 0 {
            eprintln!("Failed to create new PID namespace");
            return Err(anyhow::anyhow!("Failed to create new PID namespace"));
        }
    }

    let output = std::process::Command::new(command)
        .args(command_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
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
