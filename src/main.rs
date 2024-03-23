use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, Parser, Subcommand};
use image::{ImageBuffer, RgbaImage};
use image_util::ImageBufferExt;
use lua::LuaOutput;
use rayon::prelude::*;

#[macro_use]
extern crate log;

mod image_util;
mod logger;
mod lua;

#[derive(Parser, Debug)]
#[command(version, about, long_about=None)]
struct Cli {
    #[clap(subcommand)]
    command: GenerationCommand,
}

#[derive(Subcommand, Debug)]
enum GenerationCommand {
    /// Generate sprite sheets from a folder of images
    Spritesheet {
        // args
        #[clap(flatten)]
        args: SpritesheetArgs,
    },

    /// Generate a mipmap icon from a folder of images
    ///
    /// The individual images are used as the respective mip levels and combined into a single image
    Icon {
        // args
        #[clap(flatten)]
        args: IconArgs,
    },
}

impl std::ops::Deref for GenerationCommand {
    type Target = SharedArgs;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Spritesheet { args } => &args.shared,
            Self::Icon { args } => &args.shared,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum CommandError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("image error: {0}")]
    ImageError(#[from] image::ImageError),

    #[error("{0}")]
    ImgUtilError(#[from] image_util::ImgUtilError),

    #[error("output path is not a directory")]
    OutputPathNotDir,

    #[error("{0}")]
    SpriteSheetError(#[from] SpriteSheetError),

    #[error("{0}")]
    IconError(#[from] IconError),
}

#[derive(Args, Debug)]
struct SpritesheetArgs {
    // shared args
    #[clap(flatten)]
    shared: SharedArgs,

    /// Recursive search for images. Each folder will be a separate sprite sheet
    #[clap(short, long, action)]
    pub recursive: bool,

    /// Resolution in pixel per tile
    #[clap(short, long, default_value = "32")]
    pub tile_resolution: usize,

    /// Set when this is considered a high resolution texture
    #[clap(long, action)]
    pub hr: bool,

    /// Set when the sprites should not be cropped
    #[clap(long, action)]
    pub no_crop: bool,
}

impl std::ops::Deref for SpritesheetArgs {
    type Target = SharedArgs;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl SpritesheetArgs {
    fn execute(&self) -> Result<(), CommandError> {
        fs::create_dir_all(&self.output)?;

        if !self.output.is_dir() {
            return Err(CommandError::OutputPathNotDir);
        }

        let sources = if self.recursive {
            fs::read_dir(&self.source)?
                .filter_map(|entry| {
                    let path = entry.ok()?.path();

                    if path.is_dir() {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        } else {
            vec![self.source.clone()]
        };

        if sources.is_empty() {
            warn!("no source directories found");
            return Ok(());
        }

        let res = sources
            .par_iter()
            .filter_map(|source| match generate_spritesheet(self, source) {
                Ok(res_name) => {
                    if res_name.is_empty() {
                        None
                    } else {
                        Some(res_name)
                    }
                }
                Err(err) => {
                    error!("{}: {err}", source.display());
                    None
                }
            })
            .collect::<Vec<_>>();

        'r_group: {
            if self.recursive && self.lua && !res.is_empty() {
                #[allow(clippy::unwrap_used)]
                let name = self
                    .source
                    .components()
                    .last()
                    .unwrap()
                    .as_os_str()
                    .to_string_lossy()
                    .to_string();

                if res.contains(&name) {
                    warn!("skipping lua generation for recursive group: collision with source folder name");
                    break 'r_group;
                }

                let mut out_path = self.output.join(name);
                out_path.set_extension("lua");

                let mut output = LuaOutput::new();

                for name in res {
                    output = output.reexport(name);
                }

                output.save(out_path)?;
            }
        }

        Ok(())
    }

    const fn tile_res(&self) -> usize {
        if self.hr {
            64
        } else {
            self.tile_resolution
        }
    }
}

#[derive(Args, Debug)]
struct IconArgs {
    // shared args
    #[clap(flatten)]
    shared: SharedArgs,
}

impl std::ops::Deref for IconArgs {
    type Target = SharedArgs;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

#[derive(Args, Debug)]
struct SharedArgs {
    /// Folder containing the individual sprites
    pub source: PathBuf,

    /// Output folder
    pub output: PathBuf,

    /// Enable lua output generation
    #[clap(short, long, action)]
    lua: bool,
}

fn main() -> ExitCode {
    let args = Cli::parse();
    logger::init("info");
    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let res = match args.command {
        GenerationCommand::Spritesheet { args } => args.execute(),
        GenerationCommand::Icon { args } => generate_mipmap_icon(&args),
    };

    if let Err(err) = res {
        error!("{}", err);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn output_name(
    source: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    id: Option<usize>,
    extension: &str,
) -> PathBuf {
    #[allow(clippy::unwrap_used)]
    let name = source
        .as_ref()
        .components()
        .last()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .to_string();

    let suffixed_name = match id {
        Some(id) => format!("{name}-{id}"),
        None => name,
    };

    let mut out = output_dir.as_ref().join(suffixed_name);
    out.set_extension(extension);

    out
}

#[derive(Debug, thiserror::Error)]
enum IconError {
    #[error("source image is not square")]
    ImageNotSquare,

    #[error("unable to generate {0} mipmap levels, max possible for this icon is {1}")]
    TooManyImages(usize, usize),

    #[error("unable to divide image size by 2 for mipmap level {0}")]
    OddImageSizeForMipLevel(usize),

    #[error("source image has wrong size, {0} != {1}")]
    WrongImageSize(u32, u32),
}

fn generate_mipmap_icon(args: &IconArgs) -> Result<(), CommandError> {
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
    let mut total_width = 0;
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
        total_width += next_width;
    }

    image::imageops::crop_imm(&res, 0, 0, total_width, res.height())
        .to_image()
        .save_optimized_png(output_name(&args.source, &args.output, None, "png"))?;

    if args.lua {
        LuaOutput::new()
            .set("icon_size", base_width)
            .set("icon_mipmaps", images.len())
            .save(output_name(&args.source, &args.output, None, "lua"))?;
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum SpriteSheetError {
    #[error("all source images must be the same size")]
    ImagesNotSameSize,
}

#[allow(clippy::too_many_lines)]
fn generate_spritesheet(
    args: &SpritesheetArgs,
    path: impl AsRef<Path>,
) -> Result<String, CommandError> {
    let source = path.as_ref();
    let mut images = image_util::load_from_path(source)?;

    if images.is_empty() {
        warn!("{}: no source images found", source.display());
        return Ok(String::new());
    }

    let (shift_x, shift_y) = if args.no_crop {
        (0.0, 0.0)
    } else {
        image_util::crop_images(&mut images)?
    };

    #[allow(clippy::unwrap_used)]
    let (sprite_width, sprite_height) = images.first().unwrap().dimensions();
    let sprite_count = images.len() as u32;

    let max_size: u32 = if args.hr { 8192 } else { 2048 };

    let max_cols_per_sheet = max_size / sprite_width;
    let max_rows_per_sheet = max_size / sprite_height;
    let max_per_sheet = max_rows_per_sheet * max_cols_per_sheet;

    // unnecessarily overengineered PoS to calculate special sheet sizes if only 1 sheet is needed
    let (sheet_width, sheet_height, cols_per_sheet, _rows_per_sheet, max_per_sheet) =
        if max_per_sheet <= sprite_count {
            (
                sprite_width * max_cols_per_sheet,
                sprite_height * max_rows_per_sheet,
                max_cols_per_sheet,
                max_rows_per_sheet,
                max_per_sheet,
            )
        } else {
            // everything can fit 1 sheet -> custom arrange in as square as possible
            if sprite_width == sprite_height {
                let sheet_size = (sprite_count as f64).sqrt().ceil() as u32;
                (
                    sprite_width * sheet_size,
                    sprite_height * sheet_size,
                    sheet_size,
                    sheet_size,
                    sheet_size * sheet_size,
                )
            } else {
                let mut cols = 1;
                let mut rows = 1;

                while cols * rows < sprite_count {
                    if cols * sprite_width <= rows * sprite_height {
                        cols += 1;
                    } else {
                        rows += 1;
                    }
                }

                (
                    sprite_width * cols,
                    sprite_height * rows,
                    cols,
                    rows,
                    cols * rows,
                )
            }
        };

    let sheet_count = images.len() / max_per_sheet as usize
        + usize::from(images.len().rem_euclid(max_per_sheet as usize) > 0);

    let mut sheets: Vec<(RgbaImage, PathBuf)> = Vec::with_capacity(sheet_count);

    if sheet_count == 1 {
        sheets.push((
            RgbaImage::new(sheet_width, sheet_height),
            output_name(source, &args.output, None, "png"),
        ));
    } else {
        for idx in 0..sheet_count {
            sheets.push((
                RgbaImage::new(sheet_width, sheet_height),
                output_name(source, &args.output, Some(idx), "png"),
            ));
        }
    }

    // arrange sprites on sheets
    for (idx, sprite) in images.iter().enumerate() {
        if sprite.width() != sprite_width || sprite.height() != sprite_height {
            Err(SpriteSheetError::ImagesNotSameSize)?;
        }

        let sheet_idx = idx / max_per_sheet as usize;
        let sprite_idx = idx as u32 % max_per_sheet;

        let row = sprite_idx % cols_per_sheet;
        let line = sprite_idx / cols_per_sheet;

        let x = row * sprite_width;
        let y = line * sprite_height;

        image::imageops::replace(&mut sheets[sheet_idx].0, sprite, i64::from(x), i64::from(y));
    }

    // save sheets
    for (sheet, path) in sheets {
        sheet.save_optimized_png(path)?;
    }

    #[allow(clippy::unwrap_used)]
    let name = source
        .components()
        .last()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .to_string();

    if args.no_crop {
        info!("completed {name}, size: ({sprite_width}px, {sprite_height}px)");
    } else {
        info!(
                "completed {name}, size: ({sprite_width}px, {sprite_height}px), shift: ({shift_x}px, {shift_y}px)"
            );
    }

    if args.lua {
        LuaOutput::new()
            .set("width", sprite_width)
            .set("height", sprite_height)
            .set("shift", (shift_x, shift_y, args.tile_res()))
            .set("sprite_count", sprite_count)
            .set("line_length", cols_per_sheet)
            .save(output_name(source, &args.output, None, "lua"))?;
    }

    Ok(name)
}

/*

simple modes
- mipmap icon
  - get all levels from folder and combine as mipmap icon

- spritesheet
  - crop all images equally if possible (trim transparent edges)

different generation modes (important for lua generation)

- https://wiki.factorio.com/Types/SpriteFlags

- mipmap icon
  - https://wiki.factorio.com/Types/IconSpecification
  - either get all levels and check sizes or fallback to let factorio generate them

- sprite
  - https://wiki.factorio.com/Prototype/Sprite
  - can have multiple layers
  - can have HR version

- animation
    - https://wiki.factorio.com/Prototype/Animation
    - can have multiple layers
    - can have HR version
    - check if all layers have same frame count (custom sequences / repeats not supported)

- auto
  - get output name from source folder name
  - generate all modes (detect applicable modes from source file/folder names)



GRAPHIC TYPES

- Animations
  - Animation
  - RotatedAnimation
  - Animation4Way
  - RotatedAnimation4Way
  - AnimationVariations
  - RotatedAnimationVariations
- Sprites
  - Sprite
  - RotatedSprite
  - Sprite4Way
  - Sprite8Way
  - SpriteNWaySheet
  - SpriteVariations
  - (WaterReflectionDefinition)
  - (CircuitConnectorSprites)
- Icons
  - IconSpecification
  - IconData
- Tiles
  - TileTransitionSprite
  - (TileTransitions)

*/
