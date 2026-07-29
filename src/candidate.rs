use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub fn copy_template_to_candidate(reference_dir: &Path, output_dir: &Path) -> Result<()> {
    if !reference_dir.is_dir() {
        bail!(
            "official TFDI template does not exist: {}",
            reference_dir.display()
        );
    }
    if output_dir.exists() {
        bail!("candidate output already exists: {}", output_dir.display());
    }

    copy_directory(reference_dir, output_dir)
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination).with_context(|| {
        format!(
            "failed to create candidate directory: {}",
            destination.display()
        )
    })?;

    for entry in fs::read_dir(source)
        .with_context(|| format!("failed to read template directory: {}", source.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read template entry under {}", source.display()))?;
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect template entry: {}",
                entry.path().display()
            )
        })?;
        let target = destination.join(entry.file_name());
        if file_type.is_symlink() {
            bail!(
                "template contains unsupported symbolic link: {}",
                entry.path().display()
            );
        }
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target).with_context(|| {
                format!(
                    "failed to copy template file {} to {}",
                    entry.path().display(),
                    target.display()
                )
            })?;
        }
    }

    Ok(())
}
