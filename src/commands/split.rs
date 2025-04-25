use std::{num::NonZeroU32, path::PathBuf};

use clap::Args;
use image::GenericImageView as _;

use super::CommandError;

#[derive(Args, Debug)]
pub struct SplitArgs {
    /// The spritesheet to split into individual frames.
    pub source: PathBuf,

    /// Number of frames horizontally.
    pub width: NonZeroU32,
    /// Number of frames vertically.
    pub height: NonZeroU32,

    /// Output folder.
    pub output: PathBuf,
}

pub fn split(args: &SplitArgs) -> Result<(), CommandError> {
    let source = crate::image_util::load_image_from_file(&args.source)?;

    let width = args.width.get();
    let height = args.height.get();
    let (px_w, px_h) = source.dimensions();
    let frame_w = px_w / width;
    let frame_h = px_h / height;

    std::fs::create_dir_all(&args.output)?;

    for y in 0..height {
        let pos_y = y * frame_h;

        for x in 0..width {
            let pos_x = x * frame_w;

            source
                .view(pos_x, pos_y, frame_w, frame_h)
                .to_image()
                .save(args.output.join(format!("./{}.png", x + width * y)))?;
        }
    }

    Ok(())
}
