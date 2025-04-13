mod gif;
mod icon;
mod optimize;
mod spritesheet;

pub use gif::*;
pub use icon::*;
pub use optimize::*;
pub use spritesheet::*;

use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug)]
pub enum GenerationCommand {
    /// Generate sprite sheets from a folder of images.
    Spritesheet {
        // args
        #[clap(flatten)]
        args: SpritesheetArgs,
    },

    /// Generate a mipmap icon from a folder of images.
    ///
    /// The individual images are used as the respective mip levels and combined into a single image.
    Icon {
        // args
        #[clap(flatten)]
        args: IconArgs,
    },

    /// Generate a gif from a folder of images.
    ///
    /// Note: Don't use gifs for in-game graphics. This is meant for documentation / preview purposes only.
    Gif {
        // args
        #[clap(flatten)]
        args: GifArgs,
    },

    /// Optimize an image or a folder of images.
    ///
    /// This is using oxipng (and optionally pngquant / imagequant when lossy is enabled).
    /// Note: the original images will be replaced with the optimized versions.
    Optimize {
        // args
        #[clap(flatten)]
        args: OptimizeArgs,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("image error: {0}")]
    ImageError(#[from] image::ImageError),

    #[error("{0}")]
    ImgUtilError(#[from] crate::image_util::ImgUtilError),

    #[error("output path is not a directory")]
    OutputPathNotDir,

    #[error("{0}")]
    SpriteSheetError(#[from] SpriteSheetError),

    #[error("{0}")]
    IconError(#[from] IconError),
}

#[derive(Args, Debug)]
pub struct SharedArgs {
    /// Folder containing the individual sprites.
    pub source: PathBuf,

    /// Output folder.
    pub output: PathBuf,

    /// Enable lua output generation.
    #[clap(short, long, action)]
    lua: bool,

    /// Enable json output generation.
    #[clap(short, long, action)]
    json: bool,

    /// Prefix to add to the output file name.
    #[clap(short, long, default_value_t = String::new())]
    prefix: String,

    /// Allow lossy compression for the output images.
    /// This is using pngquant / imagequant internally.
    #[clap(long, action)]
    lossy: bool,
}

fn output_name(
    source: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    id: Option<usize>,
    prefix: &str,
    extension: &str,
) -> Result<PathBuf, CommandError> {
    #[allow(clippy::unwrap_used)]
    let name = source
        .as_ref()
        .canonicalize()?
        .components()
        .last()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .to_string();

    let pre_suff_name = id.map_or_else(
        || format!("{prefix}{name}"),
        |id| format!("{prefix}{name}-{id}"),
    );

    let mut out = output_dir.as_ref().join(pre_suff_name);
    out.set_extension(extension);

    Ok(out)
}
