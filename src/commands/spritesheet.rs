use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{builder::PossibleValue, Args, ValueEnum};
use image::{
    imageops::{self, FilterType},
    RgbaImage,
};
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use strum::{EnumIter, VariantArray};

use super::{CommandError, SharedArgs};
use crate::{commands::output_name, image_util, lua::LuaOutput};

#[allow(clippy::struct_excessive_bools)]
#[derive(Args, Debug)]
pub struct SpritesheetArgs {
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

    /// Sets the max channel value to consider a pixel as black.
    /// All "black" pixels will be turned fully transparent.
    #[clap(short = 'b', long, default_value = None, verbatim_doc_comment)]
    pub transparent_black: Option<u8>,

    /// Remove duplicate empty frames before building the sprite sheet.
    /// This will generate a `frame_sequence` in the data output to restore the original frame order.
    /// Make sure to have the --lua or --json flag set to receive the data output!
    #[clap(short, long, action, verbatim_doc_comment)]
    pub deduplicate_empty_frames: bool,

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

    /// Maximum size of a single sheet in frames per axis.
    /// A value of 0 means unlimited.
    #[clap(short, long, default_value_t = 0, verbatim_doc_comment)]
    pub max_sheet_size: u32,

    /// Maximum width of a single sheet in frames.
    /// A value of 0 means unlimited.
    /// Use this in combination with --max-sheet-size to precisely control the size of sheets.
    #[clap(short = 'w', long, default_value_t = 0, verbatim_doc_comment)]
    pub max_sheet_width: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, VariantArray)]
pub enum ScaleFilter {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

#[derive(Debug, thiserror::Error)]
pub enum SpriteSheetError {
    #[error("all source images must be the same size")]
    ImagesNotSameSize,
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
    pub fn execute(&self) -> Result<(), CommandError> {
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

    if let Some(black_limit) = args.transparent_black {
        image_util::transparent_black(&mut images, black_limit);
    }

    let (shift_x, shift_y) = if args.no_crop {
        (0.0, 0.0)
    } else {
        image_util::crop_images(&mut images, args.crop_alpha)?
    };

    // dedup empty frames
    let frame_sequence = if args.deduplicate_empty_frames {
        let (dedup_imgs, sequence) = image_util::dedup_empty_frames(images);
        images = dedup_imgs;
        Some(sequence)
    } else {
        None
    };

    #[allow(clippy::unwrap_used)]
    let (sprite_width, sprite_height) = images.first().unwrap().dimensions();
    let sprite_count = images.len() as u32;

    let (max_cols_per_sheet, max_rows_per_sheet) = {
        let technical_max_cols = MAX_SIZE / sprite_width;
        let technical_max_rows = MAX_SIZE / sprite_height;

        match (args.max_sheet_size, args.max_sheet_width) {
            (0, 0) => (technical_max_cols, technical_max_rows),
            (max_sheet_size, 0) if max_sheet_size > 0 => (
                max_sheet_size.min(technical_max_cols),
                max_sheet_size.min(technical_max_rows),
            ),
            (0, max_sheet_width) if max_sheet_width > 0 => {
                (max_sheet_width.min(technical_max_cols), technical_max_rows)
            }
            (max_sheet_size, max_sheet_width) => (
                max_sheet_width.min(technical_max_cols),
                max_sheet_size.min(technical_max_rows),
            ),
        }
    };

    let max_per_sheet = max_rows_per_sheet * max_cols_per_sheet;

    let sheet_count = images.len() / max_per_sheet as usize
        + usize::from(images.len().rem_euclid(max_per_sheet as usize) > 0);

    #[allow(clippy::unwrap_used)]
    let name = source
        .canonicalize()?
        .components()
        .next_back()
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

            let mut data = LuaOutput::new()
                .set("width", *width)
                .set("height", *height)
                .set("shift", (*shift_x, *shift_y, args.tile_res()))
                .set("scale", 32.0 / args.tile_res() as f64)
                .set("sprite_count", sprite_count)
                .set("line_length", *cols)
                .set("lines_per_file", *rows);

            if let Some(sequence) = &frame_sequence {
                data = data.set("frame_sequence", sequence.as_slice());
            }

            lua_layers.push(data);

            sheets.push((sheet.clone(), out));
        }

        image_util::save_sheets(&sheets, args.lossy, true)?;

        if args.lua {
            LuaOutput::new()
                .set("single_sheet_split_layers", lua_layers.as_slice())
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
            debug!("using maximized sheet: {max_cols_per_sheet}x{max_rows_per_sheet}");

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
                if (cols < max_cols_per_sheet) && (cols * sprite_width <= rows * sprite_height) {
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
    image_util::save_sheets(&sheets, args.lossy, true)?;

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

    if args.lua || args.json {
        let mut data = LuaOutput::new()
            .set("width", sprite_width)
            .set("height", sprite_height)
            .set("shift", (shift_x, shift_y, args.tile_res()))
            .set("scale", 32.0 / args.tile_res() as f64)
            .set("sprite_count", sprite_count)
            .set("line_length", cols_per_sheet)
            .set("lines_per_file", rows_per_sheet)
            .set("file_count", sheet_count);

        if let Some(sequence) = frame_sequence {
            data = data.set("frame_sequence", sequence.as_slice());
        }

        if args.lua {
            let out = output_name(source, &args.output, None, &args.prefix, "lua")?;
            data.save(out)?;
        }
        if args.json {
            let out = output_name(source, &args.output, None, &args.prefix, "json")?;
            data.save_as_json(out)?;
        }
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
