use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversionRequest {
    pub db_path: PathBuf,
    pub rte_seg_path: PathBuf,
    pub reference_dir: PathBuf,
    pub output_dir: PathBuf,
}

pub fn parse_conversion_args<I, S>(args: I) -> Result<ConversionRequest>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let mut db_path = None;
    let mut rte_seg_path = None;
    let mut reference_dir = None;
    let mut output_dir = None;

    while let Some(argument) = args.next() {
        let option = argument
            .to_str()
            .context("command-line option is not valid Unicode")?;
        let value = args
            .next()
            .with_context(|| format!("missing value for {option}"))?;
        let value = PathBuf::from(value);
        match option {
            "--db" => db_path = Some(value),
            "--rte-seg" => rte_seg_path = Some(value),
            "--reference" => reference_dir = Some(value),
            "--output" => output_dir = Some(value),
            _ => bail!("unknown option: {option}"),
        }
    }

    Ok(ConversionRequest {
        db_path: db_path.context("missing required option --db")?,
        rte_seg_path: rte_seg_path.context("missing required option --rte-seg")?,
        reference_dir: reference_dir.context("missing required option --reference")?,
        output_dir: output_dir.context("missing required option --output")?,
    })
}

