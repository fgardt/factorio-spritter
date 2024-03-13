use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Args, Parser, Subcommand};
use image::{ImageBuffer, RgbaImage};

mod image_util;

#[derive(Parser, Debug)]
#[command(version, about, long_about=None)]
struct Cli {
    /// icon subcommand
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

    /// Generate mipmap icon from a single image or a folder of images
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
    // /// Enable lua output generation
    // #[clap(short, long, action)]
    // lua: bool,
}

#[derive(Debug)]
pub struct Error;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "spritter error")
    }
}

impl std::error::Error for Error {}

fn main() {
    let args = Cli::parse();

    match args.command {
        GenerationCommand::Spritesheet { args } => {
            generate_spritesheet(&args);
        }
        GenerationCommand::Icon { args } => {
            generate_mipmap_icon(&args);
        }
    }
}

fn output_name(source: &Path, output_dir: &Path, id: Option<usize>, extension: &str) -> PathBuf {
    let name = source
        .components()
        .last()
        .unwrap()
        .as_os_str()
        .to_str()
        .unwrap_or_default()
        .to_owned();

    let suffixed_name = match id {
        Some(id) => format!("{name}-{id}"),
        None => name,
    };

    let mut out = output_dir.to_path_buf().join(suffixed_name);
    out.set_extension(extension);

    out
}

fn generate_mipmap_icon(args: &IconArgs) {
    // TODO: move this into it's own helper function and resultify it
    fs::create_dir_all(&args.output).unwrap();

    if !args.output.is_dir() {
        println!("output path is not a directory");
        return;
    }

    let mut images = image_util::load_from_path(&args.source).unwrap();

    if images.is_empty() {
        println!("no source images found");
        return;
    }

    images.sort_by_key(ImageBuffer::width);
    images.reverse();

    let (base_width, base_height) = images.first().unwrap().dimensions();
    if base_width != base_height {
        println!("source image is not square");
        return;
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_mipmap_levels = (f64::from(base_width)).log2().floor() as usize;

    if images.len() > max_mipmap_levels {
        println!("unable to generate {} mipmap levels, max possible for this icon is {max_mipmap_levels}", images.len());
        return;
    }

    let mut res = ImageBuffer::new(base_width * 2, base_height);

    let mut next_width = base_width;
    let mut total_width = 0;
    let mut next_x = 0;

    for (idx, sprite) in images.iter().enumerate() {
        if next_width.rem_euclid(2) != 0 {
            println!("unable to divide image size by 2 for mipmap level {idx}");
            return;
        }

        if sprite.width() != sprite.height() {
            println!("source image is not square");
            return;
        }

        if sprite.width() != next_width {
            println!(
                "source image has wrong size, {} != {next_width}",
                sprite.width()
            );
            return;
        }

        image::imageops::replace(&mut res, sprite, i64::from(next_x), 0);

        next_x += next_width;
        next_width /= 2;
        total_width += next_width;
    }

    image::imageops::crop_imm(&res, 0, 0, total_width, res.height())
        .to_image()
        .save_with_format(
            output_name(&args.source, &args.output, None, "png"),
            image::ImageFormat::Png,
        )
        .unwrap();
}

#[allow(clippy::too_many_lines)]
fn generate_spritesheet(args: &SpritesheetArgs) {
    // TODO: move this into it's own helper function and resultify it
    fs::create_dir_all(&args.output).unwrap();

    if !args.output.is_dir() {
        println!("output path is not a directory");
        return;
    }

    let sources = if args.recursive {
        fs::read_dir(&args.source)
            .unwrap()
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
        vec![args.source.clone()]
    };

    if sources.is_empty() {
        println!("no source directories found");
        return;
    }

    for source in sources {
        let mut images = image_util::load_from_path(&source).unwrap();

        if images.is_empty() {
            println!("no source images found");
            return;
        }

        let (_shift_x, _shift_y) = if args.no_crop {
            (0, 0)
        } else {
            image_util::crop_images(&mut images).unwrap()
        };

        let sprite_count = u32::try_from(images.len()).unwrap();
        let sprite_width = images.first().unwrap().width();
        let sprite_height = images.first().unwrap().height();

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
                output_name(&source, &args.output, None, "png"),
            ));
        } else {
            for idx in 0..sheet_count {
                sheets.push((
                    RgbaImage::new(sheet_width, sheet_height),
                    output_name(&source, &args.output, Some(idx), "png"),
                ));
            }
        }

        // arrange sprites on sheets
        for (idx, sprite) in images.iter().enumerate() {
            if sprite.width() != sprite_width || sprite.height() != sprite_height {
                println!("all source images must be the same size");
                return;
            }

            let sheet_idx = idx / max_per_sheet as usize;
            let sprite_idx = idx % max_per_sheet as usize;

            let row = u32::try_from(sprite_idx).unwrap() % cols_per_sheet;
            let line = u32::try_from(sprite_idx).unwrap() / cols_per_sheet;

            let x = row * sprite_width;
            let y = line * sprite_height;

            image::imageops::replace(&mut sheets[sheet_idx].0, sprite, i64::from(x), i64::from(y));
        }

        // save sheets
        for (sheet, path) in sheets {
            sheet
                .save_with_format(path, image::ImageFormat::Png)
                .unwrap();
        }
    }
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
