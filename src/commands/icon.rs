use std::fs;

use clap::Args;
use image::ImageBuffer;

use super::{output_name, CommandError};
use crate::{
    image_util::{self, ImageBufferExt as _},
    lua::LuaOutput,
};

#[derive(Debug, thiserror::Error)]
pub enum IconError {
    #[error("source image is not square")]
    ImageNotSquare,

    #[error("unable to generate {0} mipmap levels, max possible for this icon is {1}")]
    TooManyImages(usize, usize),

    #[error("unable to divide image size by 2 for mipmap level {0}")]
    OddImageSizeForMipLevel(usize),

    #[error("source image has wrong size, {0} != {1}")]
    WrongImageSize(u32, u32),
}

#[derive(Args, Debug)]
pub struct IconArgs {
    // shared args
    #[clap(flatten)]
    shared: super::SharedArgs,
}

impl std::ops::Deref for IconArgs {
    type Target = super::SharedArgs;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

pub fn generate_mipmap_icon(args: &IconArgs) -> Result<(), CommandError> {
    fs::create_dir_all(&args.output)?;
    if !args.output.is_dir() {
        return Err(CommandError::OutputPathNotDir);
    }

    let mut images = image_util::load_from_path(&args.source)?;
    if images.is_empty() {
        warn!("no source images found");
        return Ok(());
    }

    images.sort_by_key(ImageBuffer::width);
    images.reverse();

    #[allow(clippy::unwrap_used)]
    let (base_width, base_height) = images.first().unwrap().dimensions();
    if base_width != base_height {
        Err(IconError::ImageNotSquare)?;
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_mipmap_levels = (f64::from(base_width)).log2().floor() as usize;

    if images.len() > max_mipmap_levels {
        Err(IconError::TooManyImages(images.len(), max_mipmap_levels))?;
    }

    let mut res = ImageBuffer::new(base_width * 2, base_height);

    let mut next_width = base_width;
    let mut next_x = 0;

    for (idx, sprite) in images.iter().enumerate() {
        if next_width.rem_euclid(2) != 0 {
            Err(IconError::OddImageSizeForMipLevel(idx))?;
        }

        if sprite.width() != sprite.height() {
            Err(IconError::ImageNotSquare)?;
        }

        if sprite.width() != next_width {
            Err(IconError::WrongImageSize(sprite.width(), next_width))?;
        }

        image::imageops::replace(&mut res, sprite, i64::from(next_x), 0);

        next_x += next_width;
        next_width /= 2;
    }

    image::imageops::crop_imm(&res, 0, 0, next_x, res.height())
        .to_image()
        .save_optimized_png(
            output_name(&args.source, &args.output, None, &args.prefix, "png")?,
            args.lossy,
        )?;

    if args.lua || args.json {
        let data = LuaOutput::new()
            .set("icon_size", base_width)
            .set("icon_mipmaps", images.len());

        if args.lua {
            let out = output_name(&args.source, &args.output, None, &args.prefix, "lua")?;
            data.save(&out)?;
        }
        if args.json {
            let out = output_name(&args.source, &args.output, None, &args.prefix, "json")?;
            data.save_as_json(out)?;
        }
    }

    Ok(())
}
