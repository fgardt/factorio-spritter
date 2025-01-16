use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{builder::PossibleValue, Args, Parser, Subcommand, ValueEnum};
use image::{
    imageops::{self, FilterType},
    ImageBuffer, RgbaImage,
};
use rayon::prelude::*;
use strum::{EnumIter, VariantArray};

#[macro_use]
extern crate log;

mod image_util;
mod logger;
mod lua;

use image_util::{save_sheets, ImageBufferExt, ImgUtilError};
use lua::LuaOutput;

#[derive(Parser, Debug)]
#[command(version, about, long_about=None)]
struct Cli {
    #[clap(subcommand)]
    command: GenerationCommand,
}

#[derive(Subcommand, Debug)]
enum GenerationCommand {
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

#[allow(clippy::struct_excessive_bools)]
#[derive(Args, Debug)]
struct SpritesheetArgs {
    // shared args
    #[clap(flatten)]
    shared: SharedArgs,

    /// Recursive search for images. Each folder will be a separate sprite sheet
    #[clap(short, long, action)]
    pub recursive: bool,

    /// Resolution of the input sprites in pixels / tile
    #[clap(short, long, default_value_t = 64)]
    pub tile_resolution: usize,

    /// Set when the sprites should not be cropped
    #[clap(long, action)]
    pub no_crop: bool,

    /// Sets the max alpha value to consider a pixel as transparent [0-255].
    /// Use a higher value in case your inputs have slightly transparent pixels and don't crop nicely.
    #[clap(short = 'a', long, default_value_t = 0, verbatim_doc_comment)]
    pub crop_alpha: u8,

    /// Set a scaling factor to rescale the used sprites by.
    /// Values < 1.0 will shrink the sprites. Values > 1.0 will enlarge them.
    #[clap(short, long, default_value_t = 1.0, verbatim_doc_comment)]
    pub scale: f64,

    /// The scaling filter to use when scaling sprites
    #[clap(long, default_value_t = ScaleFilter::CatmullRom, verbatim_doc_comment)]
    pub scale_filter: ScaleFilter,

    /// Automatically split each frame into multiple subframes if the frames would not fit on a single sheet.
    /// This allows you to use large sprites for graphic types that do not allow to specify multiple files for a single layer.
    #[clap(long, action, verbatim_doc_comment)]
    pub single_sheet_split_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, VariantArray)]
enum ScaleFilter {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

impl std::fmt::Display for ScaleFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nearest => write!(f, "nearest"),
            Self::Triangle => write!(f, "triangle"),
            Self::CatmullRom => write!(f, "catmull-rom"),
            Self::Gaussian => write!(f, "gaussian"),
            Self::Lanczos3 => write!(f, "lanczos3"),
        }
    }
}

impl From<ScaleFilter> for FilterType {
    fn from(value: ScaleFilter) -> Self {
        match value {
            ScaleFilter::Nearest => Self::Nearest,
            ScaleFilter::Triangle => Self::Triangle,
            ScaleFilter::CatmullRom => Self::CatmullRom,
            ScaleFilter::Gaussian => Self::Gaussian,
            ScaleFilter::Lanczos3 => Self::Lanczos3,
        }
    }
}

impl ValueEnum for ScaleFilter {
    fn value_variants<'a>() -> &'a [Self] {
        Self::VARIANTS
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(PossibleValue::new(match self {
            Self::Nearest => "nearest",
            Self::Triangle => "triangle",
            Self::CatmullRom => "catmull-rom",
            Self::Gaussian => "gaussian",
            Self::Lanczos3 => "lanczos3",
        }))
    }
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

        let _ = sources
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

        Ok(())
    }

    fn tile_res(&self) -> usize {
        (self.tile_resolution as f64 * self.scale).round() as usize
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
struct GifArgs {
    // shared args
    #[clap(flatten)]
    shared: SharedArgs,

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
    type Target = SharedArgs;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

#[derive(Args, Debug)]
struct OptimizeArgs {
    pub target: PathBuf,

    /// Recursively search for images in the target folder.
    #[clap(short, long, action)]
    pub recursive: bool,

    /// Treat images as a group and optimize them together instead of individually.
    /// This only has an effect with lossy compression.
    #[clap(short, long, action, verbatim_doc_comment)]
    pub group: bool,

    /// Allow lossy compression.
    #[clap(long, action)]
    pub lossy: bool,
}

#[derive(Args, Debug)]
struct SharedArgs {
    /// Folder containing the individual sprites.
    pub source: PathBuf,

    /// Output folder.
    pub output: PathBuf,

    /// Enable lua output generation.
    #[clap(short, long, action)]
    lua: bool,

    /// Prefix to add to the output file name.
    #[clap(short, long, default_value_t = String::new())]
    prefix: String,

    /// Allow lossy compression for the output images.
    /// This is using pngquant / imagequant internally.
    #[clap(long, action)]
    lossy: bool,
}

fn main() -> ExitCode {
    let args = Cli::parse();
    logger::init("info,oxipng=warn");
    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let res = match args.command {
        GenerationCommand::Spritesheet { args } => args.execute(),
        GenerationCommand::Icon { args } => generate_mipmap_icon(&args),
        GenerationCommand::Gif { args } => generate_gif(&args),
        GenerationCommand::Optimize { args } => optimize(&args),
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

    if args.lua {
        LuaOutput::new()
            .set("icon_size", base_width)
            .set("icon_mipmaps", images.len())
            .save(output_name(
                &args.source,
                &args.output,
                None,
                &args.prefix,
                "lua",
            )?)?;
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum SpriteSheetError {
    #[error("all source images must be the same size")]
    ImagesNotSameSize,
}

/// Maximum side length of a single graphic file to load in Factorio
static MAX_SIZE: u32 = 8192;

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
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

    // scale images
    if (args.scale - 1.0).abs() > f64::EPSILON {
        for image in &mut images {
            let (width, height) = image.dimensions();
            let width = (f64::from(width) * args.scale).round() as u32;
            let height = (f64::from(height) * args.scale).round() as u32;

            *image = imageops::resize(image, width, height, args.scale_filter.into());
        }
    }

    let (shift_x, shift_y) = if args.no_crop {
        (0.0, 0.0)
    } else {
        image_util::crop_images(&mut images, args.crop_alpha)?
    };

    #[allow(clippy::unwrap_used)]
    let (sprite_width, sprite_height) = images.first().unwrap().dimensions();
    let sprite_count = images.len() as u32;

    let max_cols_per_sheet = MAX_SIZE / sprite_width;
    let max_rows_per_sheet = MAX_SIZE / sprite_height;
    let max_per_sheet = max_rows_per_sheet * max_cols_per_sheet;

    let sheet_count = images.len() / max_per_sheet as usize
        + usize::from(images.len().rem_euclid(max_per_sheet as usize) > 0);

    #[allow(clippy::unwrap_used)]
    let name = source
        .canonicalize()?
        .components()
        .last()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .to_string();

    if args.single_sheet_split_mode && sheet_count > 1 {
        debug!("sprites don't fit on a single sheet, splitting into multiple layers");
        let layers =
            generate_subframe_sheets(args, &images, sprite_width, sprite_height, shift_x, shift_y);
        let mut lua_layers = Vec::with_capacity(layers.len());
        let mut sheets = Vec::with_capacity(layers.len());

        for (idx, layer) in layers.iter().enumerate() {
            let (sheet, (width, height), (shift_x, shift_y), (cols, rows)) = layer;
            let out = output_name(source, &args.output, Some(idx), &args.prefix, "png")?;

            lua_layers.push(
                LuaOutput::new()
                    .set("width", *width)
                    .set("height", *height)
                    .set("shift", (*shift_x, *shift_y, args.tile_res()))
                    .set("scale", 32.0 / args.tile_res() as f64)
                    .set("sprite_count", sprite_count)
                    .set("line_length", *cols)
                    .set("lines_per_file", *rows),
            );

            sheets.push((sheet.clone(), out));
        }

        save_sheets(&sheets, args.lossy, true)?;

        if args.lua {
            LuaOutput::new()
                .set("single_sheet_split_layers", lua_layers.into_boxed_slice())
                .save(output_name(
                    source,
                    &args.output,
                    None,
                    &args.prefix,
                    "lua",
                )?)?;
        }

        info!(
            "completed {}{name}, split into {} layers",
            args.prefix,
            layers.len()
        );
        return Ok(name);
    }

    // unnecessarily overengineered PoS to calculate special sheet sizes if only 1 sheet is needed
    let (sheet_width, sheet_height, cols_per_sheet, rows_per_sheet, max_per_sheet) =
        if max_per_sheet <= sprite_count {
            debug!("multiple sheets needed: {max_cols_per_sheet}x{max_rows_per_sheet}");

            (
                sprite_width * max_cols_per_sheet,
                sprite_height * max_rows_per_sheet,
                max_cols_per_sheet,
                max_rows_per_sheet,
                max_per_sheet,
            )
        } else {
            // everything can fit 1 sheet -> custom arrange in as square as possible
            let mut cols = 1;
            let mut rows = 1;

            trace!("calculating custom sheet size");
            while cols * rows < sprite_count {
                if cols * sprite_width <= rows * sprite_height {
                    cols += 1;
                    trace!("cols++ | {cols}x{rows}");
                } else {
                    rows += 1;
                    trace!("rows++ | {cols}x{rows}");
                }
            }

            let empty = cols * rows - sprite_count;
            if empty / cols > 0 {
                rows -= empty / cols;
                trace!("rows-- | {cols}x{rows}");
            }

            debug!("singular custom sheet: {cols}x{rows}");

            (
                sprite_width * cols,
                sprite_height * rows,
                cols,
                rows,
                cols * rows,
            )
        };

    debug!("sheet size: {sheet_width}x{sheet_height}");

    let mut sheets: Vec<(RgbaImage, PathBuf)> = Vec::with_capacity(sheet_count);

    if sheet_count == 1 {
        sheets.push((
            RgbaImage::new(sheet_width, sheet_height),
            output_name(source, &args.output, None, &args.prefix, "png")?,
        ));
    } else {
        for idx in 0..(sheet_count - 1) {
            sheets.push((
                RgbaImage::new(sheet_width, sheet_height),
                output_name(source, &args.output, Some(idx), &args.prefix, "png")?,
            ));
        }

        // last sheet can be smaller
        let mut last_count = sprite_count % max_per_sheet;
        if last_count == 0 {
            last_count = max_per_sheet;
        }

        sheets.push((
            RgbaImage::new(
                sheet_width,
                sprite_height
                    * (f64::from(last_count) / f64::from(max_cols_per_sheet)).ceil() as u32,
            ),
            output_name(
                source,
                &args.output,
                Some(sheet_count - 1),
                &args.prefix,
                "png",
            )?,
        ));
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

        imageops::replace(&mut sheets[sheet_idx].0, sprite, i64::from(x), i64::from(y));
    }

    // save sheets
    save_sheets(&sheets, args.lossy, true)?;

    if args.no_crop {
        info!(
            "completed {}{name}, size: ({sprite_width}px, {sprite_height}px)",
            args.prefix
        );
    } else {
        info!(
            "completed {}{name}, size: ({sprite_width}px, {sprite_height}px), shift: ({shift_x}px, {shift_y}px)",
            args.prefix
        );
    }

    if args.lua {
        let out = output_name(source, &args.output, None, &args.prefix, "lua")?;
        LuaOutput::new()
            .set("width", sprite_width)
            .set("height", sprite_height)
            .set("shift", (shift_x, shift_y, args.tile_res()))
            .set("scale", 32.0 / args.tile_res() as f64)
            .set("sprite_count", sprite_count)
            .set("line_length", cols_per_sheet)
            .set("lines_per_file", rows_per_sheet)
            .set("file_count", sheet_count)
            .save(out)?;
    }

    Ok(name)
}

type SubframeData = (RgbaImage, (u32, u32), (f64, f64), (u32, u32));

fn generate_subframe_sheets(
    _args: &SpritesheetArgs,
    images: &[RgbaImage],
    sprite_width: u32,
    sprite_height: u32,
    shift_x: f64,
    shift_y: f64,
) -> Box<[SubframeData]> {
    let sprite_count = images.len() as u32;

    // figure out how many splits are needed (vertically / horizontally)
    let mut frags_x = 1;
    let mut frags_y = 1;

    loop {
        let frag_width = sprite_width.div_ceil(frags_x);
        let frag_height = sprite_height.div_ceil(frags_y);

        let frags_per_row = MAX_SIZE / frag_width;
        let frags_per_col = MAX_SIZE / frag_height;

        if frags_per_row * frags_per_col >= sprite_count {
            break;
        }

        if frag_width >= frag_height {
            frags_x += 1;
        } else {
            frags_y += 1;
        }
    }

    let frag_width = sprite_width.div_ceil(frags_x);
    let frag_height = sprite_height.div_ceil(frags_y);
    let mut frag_groups = Vec::with_capacity((frags_x * frags_y) as usize);
    for y in 0..frags_y {
        for x in 0..frags_x {
            // calculate dimesions, offset and shift for each subframe
            let tx = x * frag_width;
            let ty = y * frag_height;
            let width = frag_width.min(sprite_width - tx);
            let height = frag_height.min(sprite_height - ty);

            // frag_shift = tx + (width / 2) - (sprite_width / 2) + shift_x
            let frag_shift_x =
                (f64::from(width) - f64::from(sprite_width)).mul_add(0.5, f64::from(tx) + shift_x);
            let frag_shift_y = (f64::from(height) - f64::from(sprite_height))
                .mul_add(0.5, f64::from(ty) + shift_y);

            let frags = images
                .iter()
                .map(|frame| imageops::crop_imm(frame, tx, ty, width, height))
                .collect::<Box<_>>();

            // TODO: autocrop subframes again (?)

            frag_groups.push((frags, (width, height), (frag_shift_x, frag_shift_y)));
        }
    }

    // arrange subframes on sheets
    frag_groups
        .iter()
        .map(|(frags, (width, height), (shift_x, shift_y))| {
            let cols = MAX_SIZE / width;
            let sheet_width = cols * width;
            let rows = sprite_count.div_ceil(cols);
            let sheet_height = rows * height;

            let mut sheet = RgbaImage::new(sheet_width, sheet_height);

            for (idx, frag) in frags.iter().enumerate() {
                let row = idx as u32 % cols;
                let line = idx as u32 / cols;

                let x = row * width;
                let y = line * height;

                imageops::replace(&mut sheet, &frag.to_image(), i64::from(x), i64::from(y));
            }

            (sheet, (*width, *height), (*shift_x, *shift_y), (cols, rows))
        })
        .collect()
}

fn generate_gif(args: &GifArgs) -> Result<(), CommandError> {
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

fn optimize(args: &OptimizeArgs) -> Result<(), CommandError> {
    let mut paths = Vec::new();

    if args.target.is_dir() {
        paths.extend(pngs_in_folder(&args.target)?);

        if args.recursive {
            let folders = recursive_folders(&args.target)?;

            for folder in &folders {
                paths.extend(pngs_in_folder(folder)?);
            }

            info!(
                "found {} images after searching through {} folders",
                paths.len(),
                folders.len()
            );
        }
    } else {
        if args.recursive {
            warn!("target is not a directory, recursive search disabled");
        }

        if args.target.extension().is_some_and(|ext| ext == "png") {
            paths.push(args.target.clone());
        }
    }

    if paths.is_empty() {
        warn!("no source images found");
        return Ok(());
    }

    if args.group {
        if args.lossy {
            return optimize_lossy_grouped(&paths);
        }

        warn!("group optimization only has an effect with lossy compression, ignoring group flag");
    }

    optimize_seq_runner(&paths, |path| optimize_single(path, args.lossy));

    Ok(())
}

fn optimize_lossy_grouped(paths: &[PathBuf]) -> Result<(), CommandError> {
    let quant = image_util::quantization_attributes()?;
    let mut histo = imagequant::Histogram::new(&quant);

    info!("generating histogram of all images");
    let known_good_paths = paths
        .iter()
        .filter(|path| match image_util::load_image_from_file(path) {
            Ok(img) => {
                if let Err(err) = histo.add_colors(&img.get_histogram(), 0.0) {
                    warn!("{}: {err}", path.display());
                    false
                } else {
                    true
                }
            }
            Err(err) => {
                warn!("{}: {err}", path.display());
                false
            }
        })
        .cloned()
        .collect::<Box<_>>();

    if known_good_paths.is_empty() {
        warn!("no source images found");
        return Ok(());
    }

    let mut qres = histo.quantize(&quant).map_err(ImgUtilError::from)?;
    qres.set_dithering_level(1.0).map_err(ImgUtilError::from)?;
    let palette = image_util::convert_palette(qres.palette());

    info!("optimizing images");

    optimize_seq_runner(&known_good_paths, |path| {
        optimize_single_quantized(path, &quant, &mut qres, &palette)
    });

    Ok(())
}

fn optimize_seq_runner<S>(paths: &[PathBuf], mut step: S)
where
    S: FnMut(&PathBuf) -> Result<(u64, u64), ImgUtilError>,
{
    let mut total_in = 0;
    let mut total_out = 0;

    for path in paths {
        match step(path) {
            Ok((b_in, b_out)) => {
                total_in += b_in;
                total_out += b_out;
            }
            Err(err) => {
                error!("{}: {err}", path.display());
            }
        }
    }

    let reduced_by = total_in - total_out;
    let percent = ((total_out as f64 / total_in as f64) - 1.0) * 100.0;
    info!(
        "total: {percent:.2}%, saved {}",
        human_readable_bytes(reduced_by)
    );
}

fn optimize_single(path: &PathBuf, lossy: bool) -> Result<(u64, u64), ImgUtilError> {
    let orig = std::fs::read(path)?;
    let orig_size = orig.len() as u64;
    let res_size = image_util::load_image_from_file(path)?.save_optimized_png(path, lossy)?;

    optimize_common_res(path, &orig, orig_size, res_size)
}

fn optimize_single_quantized(
    path: &PathBuf,
    quant: &imagequant::Attributes,
    qres: &mut imagequant::QuantizationResult,
    palette: &[[u8; 4]],
) -> Result<(u64, u64), ImgUtilError> {
    let orig = std::fs::read(path)?;
    let orig_size = orig.len() as u64;

    let img = image_util::load_image_from_file(path)?;
    let (width, height) = img.dimensions();
    let w_usize = width as usize;
    let h_usize = height as usize;
    let mut img = quant.new_image(img.to_quant_img(), w_usize, h_usize, 0.0)?;

    let mut pxls = Vec::with_capacity(w_usize * h_usize);
    qres.remap_into_vec(&mut img, &mut pxls)?;

    let res_size = image_util::optimize_png(
        &image_util::image_buf_from_palette(width, height, palette, &pxls),
        width,
        height,
        path,
    )?;

    optimize_common_res(path, &orig, orig_size, res_size)
}

fn optimize_common_res(
    path: &PathBuf,
    orig: &[u8],
    orig_size: u64,
    res_size: u64,
) -> Result<(u64, u64), ImgUtilError> {
    if res_size >= orig_size {
        info!("{}: could not optimize further", path.display());
        std::fs::write(path, orig)?;
        Ok((orig_size, orig_size))
    } else {
        let reduced_by = orig_size - res_size;
        let percent = ((res_size as f64 / orig_size as f64) - 1.0) * 100.0;

        info!(
            "{}: {percent:.2}% smaller, saved {}",
            path.display(),
            human_readable_bytes(reduced_by)
        );

        Ok((orig_size, res_size))
    }
}

fn recursive_folders(path: impl AsRef<Path>) -> std::io::Result<Box<[PathBuf]>> {
    let mut folders = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            folders.push(path);
        }
    }

    let mut descent = Vec::new();
    for folder in &folders {
        descent.extend(recursive_folders(folder)?);
    }

    folders.extend(descent);
    Ok(folders.into_boxed_slice())
}

fn pngs_in_folder(path: impl AsRef<Path>) -> std::io::Result<Box<[PathBuf]>> {
    let mut pngs = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
            pngs.push(path);
        }
    }

    Ok(pngs.into_boxed_slice())
}

fn human_readable_bytes(bytes: u64) -> String {
    static UNITS: [&str; 6] = ["B", "kB", "MB", "GB", "TB", "PB"]; // wtf are you doing if this saves you petabytes -.-

    if bytes < 1000 {
        return format!("{bytes}{}", UNITS[0]);
    }

    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1000.0 && unit < UNITS.len() - 1 {
        size /= 1000.0;
        unit += 1;
    }

    format!("{:.2}{}", size, UNITS[unit])
}
