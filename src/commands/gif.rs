use std::fs;

use clap::Args;

use super::{output_name, CommandError};
use crate::image_util;

#[derive(Args, Debug)]
pub struct GifArgs {
    // shared args
    #[clap(flatten)]
    shared: super::SharedArgs,

    /// Animation speed to use for the gif.
    /// This is identical to in-game speed. 1.0 means 60 frames per second.
    /// Note: GIFs frame delay is in steps of 10ms, so the actual speed might be slightly different.
    #[clap(short = 's', long, default_value = "1.0", verbatim_doc_comment)]
    pub animation_speed: f64,

    /// Alpha threshold to consider a pixel as transparent [0-255].
    /// Since GIFS only support 1-bit transparency, this is used to determine which pixels are transparent.
    #[clap(short, long, default_value = "0", verbatim_doc_comment)]
    pub alpha_threshold: u8,
}

impl std::ops::Deref for GifArgs {
    type Target = super::SharedArgs;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

pub fn generate_gif(args: &GifArgs) -> Result<(), CommandError> {
    use image::{codecs::gif, Delay, Frame};

    if args.lua {
        warn!("lua output is not supported for gifs");
    }

    if args.animation_speed <= 0.0 {
        warn!("animation speed must be greater than 0");
        return Ok(());
    }

    let mut images = image_util::load_from_path(&args.source)?;

    if images.is_empty() {
        warn!("no source images found");
        return Ok(());
    }

    for img in &mut images {
        for pxl in img.pixels_mut() {
            if pxl[3] <= 10 {
                pxl[0] = 0;
                pxl[1] = 0;
                pxl[2] = 0;
                pxl[3] = 0;
            }
        }
    }

    let mut file = fs::File::create(output_name(
        &args.source,
        &args.output,
        None,
        &args.prefix,
        ".gif",
    )?)?;

    let mut encoder = gif::GifEncoder::new(&mut file);
    encoder.set_repeat(gif::Repeat::Infinite)?;

    encoder.try_encode_frames(images.iter().map(|img| {
        Ok(Frame::from_parts(
            img.clone(),
            0,
            0,
            Delay::from_numer_denom_ms(100_000, (6000.0 * args.animation_speed).round() as u32),
        ))
    }))?;

    Ok(())
}
