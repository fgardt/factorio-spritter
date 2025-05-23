use std::{
    borrow::Cow,
    collections::HashMap,
    fs,
    io::Write,
    ops::Deref,
    path::{Path, PathBuf},
};

use image::{
    codecs::png, EncodableLayout, ImageBuffer, ImageEncoder, ImageReader, PixelWithColorType, Rgba,
    RgbaImage,
};
use imagequant::{Attributes, Histogram, HistogramEntry};

#[derive(Debug, thiserror::Error)]
pub enum ImgUtilError {
    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("image error: {0}")]
    ImageError(#[from] image::ImageError),

    #[error("imagequant error: {0}")]
    ImageQuantError(#[from] imagequant::Error),

    #[error("oxipng error: {0}")]
    OxipngError(#[from] oxipng::PngError),

    #[error("no images to crop")]
    NoImagesToCrop,

    #[error("all images must be the same size")]
    NotSameSize,

    #[error("unable to crop, all images are empty")]
    AllImagesEmpty,
}

type ImgUtilResult<T> = std::result::Result<T, ImgUtilError>;

pub fn load_from_path_with_path(path: &Path) -> ImgUtilResult<Vec<(RgbaImage, PathBuf)>> {
    if !path.exists() {
        return Err(ImgUtilError::IOError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("path not found: {}", path.display()),
        )));
    }

    if path.is_file() && path.extension().unwrap_or_default() == "png" {
        return Ok(vec![(load_image_from_file(path)?, path.to_path_buf())]);
    }

    let mut images = Vec::new();
    let mut files = fs::read_dir(path)?
        .filter_map(|res| res.map_or(None, |e| Some(e.path())))
        .collect::<Vec<_>>();

    files.sort_by(|a, b| {
        let a = a.to_string_lossy().into_owned();
        let b = b.to_string_lossy().into_owned();
        natord::compare(&a, &b)
    });

    for path in files {
        // skip directories, no recursive search
        if path.is_dir() {
            continue;
        }

        if path.extension().unwrap_or_default() != "png" {
            continue;
        }

        if !path.exists() {
            continue;
        }

        images.push((load_image_from_file(&path)?, path));
    }

    Ok(images)
}

pub fn load_from_path(path: &Path) -> ImgUtilResult<Vec<RgbaImage>> {
    let res = load_from_path_with_path(path)?;
    Ok(res.into_iter().map(|(img, _)| img).collect())
}

pub fn load_image_from_file(path: &Path) -> ImgUtilResult<RgbaImage> {
    trace!("loading image from {}", path.display());
    let image = ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?
        .to_rgba8();
    Ok(image)
}

pub fn transparent_black(images: &mut [RgbaImage], black_limit: u8) {
    static TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);

    for image in images.iter_mut() {
        for pxl in image.pixels_mut() {
            if pxl[0] <= black_limit && pxl[1] <= black_limit && pxl[2] <= black_limit {
                *pxl = TRANSPARENT;
            }
        }
    }
}

pub fn crop_images(images: &mut Vec<RgbaImage>, alpha_limit: u8) -> ImgUtilResult<(f64, f64)> {
    if images.is_empty() {
        return Err(ImgUtilError::NoImagesToCrop);
    }

    #[allow(clippy::unwrap_used)]
    let (raw_width, raw_height) = images.first().unwrap().dimensions();

    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = u32::MIN;
    let mut max_y = u32::MIN;

    for image in images.iter() {
        // ensure image has same size
        if image.width() != raw_width || image.height() != raw_height {
            return Err(ImgUtilError::NotSameSize);
        }

        let mut x = image
            .enumerate_pixels()
            .filter_map(|(x, _, pxl)| if pxl[3] > alpha_limit { Some(x) } else { None })
            .collect::<Vec<_>>();
        x.sort_unstable();

        let mut y = image
            .enumerate_pixels()
            .filter_map(|(_, y, pxl)| if pxl[3] > alpha_limit { Some(y) } else { None })
            .collect::<Vec<_>>();
        y.sort_unstable();

        // ensure image is not empty
        if x.is_empty() || y.is_empty() {
            continue;
        }

        let local_min_x = x[0];
        let local_min_y = y[0];
        let local_max_x = x[x.len() - 1];
        let local_max_y = y[y.len() - 1];

        if min_x > local_min_x {
            min_x = local_min_x;
        }

        if max_x < local_max_x {
            max_x = local_max_x;
        }

        if min_y > local_min_y {
            min_y = local_min_y;
        }

        if max_y < local_max_y {
            max_y = local_max_y;
        }
    }

    // are all images empty? (or some other edge case?)
    if min_x == u32::MAX || min_y == u32::MAX || max_x == u32::MIN || max_y == u32::MIN {
        return Err(ImgUtilError::AllImagesEmpty);
    }

    // do we need to crop?
    if min_x == 0 && min_y == 0 && max_x == (raw_width - 1) && max_y == (raw_height - 1) {
        // no cropping needed
        return Ok((0.0, 0.0));
    }

    let cropped_width = max_x - min_x + 1;
    let cropped_height = max_y - min_y + 1;

    debug!("cropping from {raw_width}x{raw_height} to {cropped_width}x{cropped_height}");
    trace!("min_x: {min_x}, min_y: {min_y}, max_x: {max_x}, max_y: {max_y}");

    // crop images
    for image in images {
        let cropped_image =
            image::imageops::crop_imm(image, min_x, min_y, cropped_width, cropped_height)
                .to_image();
        *image = cropped_image;
    }

    // calculate how the center point shifted relative to the original image
    let mut shift_x = -((f64::from(raw_width - cropped_width) / 2.0) - f64::from(min_x));
    let mut shift_y = -((f64::from(raw_height - cropped_height) / 2.0) - f64::from(min_y));

    if shift_x == 0.0 {
        shift_x = 0.0;
    }

    if shift_y == 0.0 {
        shift_y = 0.0;
    }

    trace!("shifted by ({shift_x}, {shift_y})");

    Ok((shift_x, shift_y))
}

pub fn dedup_empty_frames(images: Vec<RgbaImage>) -> (Vec<RgbaImage>, Vec<usize>) {
    let mut res = Vec::with_capacity(images.len());
    let mut sequence = Vec::with_capacity(images.len());
    let mut first_empty_idx = usize::MAX;

    for image in images {
        let empty = image.pixels().all(|pxl| pxl[3] == 0);
        if empty {
            if first_empty_idx == usize::MAX {
                first_empty_idx = res.len();
                res.push(image);
            }

            sequence.push(first_empty_idx);
        } else {
            sequence.push(res.len());
            res.push(image);
        }
    }

    (res, sequence)
}

pub trait ImageBufferExt<P, C> {
    fn save_optimized_png(&self, path: impl AsRef<Path>, lossy: bool) -> ImgUtilResult<u64>;

    fn get_histogram(&self) -> Box<[HistogramEntry]>;
    fn to_quant_img(&self) -> Box<[imagequant::RGBA]>;
}

impl<C> ImageBufferExt<Rgba<u8>, C> for ImageBuffer<Rgba<u8>, C>
where
    C: Deref<Target = [u8]>,
{
    fn save_optimized_png(&self, path: impl AsRef<Path>, lossy: bool) -> ImgUtilResult<u64> {
        trace!("saving image to {}", path.as_ref().display());
        let (width, height) = self.dimensions();

        let buf = if lossy {
            let quant = quantization_attributes()?;
            let mut img =
                quant.new_image(self.to_quant_img(), width as usize, height as usize, 0.0)?;

            let mut qres = quant.quantize(&mut img)?;
            qres.set_dithering_level(1.0)?;

            let (palette, pxls) = qres.remapped(&mut img)?;
            image_buf_from_palette(width, height, &convert_palette(&palette), &pxls)
        } else {
            Cow::Borrowed(self.as_bytes())
        };

        optimize_png(&buf, width, height, path)
    }

    fn get_histogram(&self) -> Box<[HistogramEntry]> {
        let mut res = HashMap::new();

        for pxl in self.pixels() {
            let key = (pxl[0], pxl[1], pxl[2], pxl[3]);
            let entry = res.entry(key).or_insert(0);
            *entry += 1;
        }

        res.iter()
            .map(|(&(r, g, b, a), v)| HistogramEntry {
                color: imagequant::RGBA { r, g, b, a },
                count: *v,
            })
            .collect()
    }

    fn to_quant_img(&self) -> Box<[imagequant::RGBA]> {
        self.pixels()
            .map(|pxl| imagequant::RGBA {
                r: pxl[0],
                g: pxl[1],
                b: pxl[2],
                a: pxl[3],
            })
            .collect::<Box<_>>()
    }
}

pub fn quantization_attributes() -> ImgUtilResult<Attributes> {
    let mut attr = Attributes::new();
    attr.set_speed(1)?;

    Ok(attr)
}

/// Encode image as PNG and optimize with [oxipng] before writing to disk.
pub fn optimize_png(
    buf: &[u8],
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
) -> ImgUtilResult<u64> {
    let mut data = Vec::new();
    png::PngEncoder::new_with_quality(
        &mut data,
        png::CompressionType::Fast,
        png::FilterType::default(),
    )
    .write_image(
        buf,
        width,
        height,
        <Rgba<u8> as PixelWithColorType>::COLOR_TYPE,
    )?;

    let mut opts = oxipng::Options::max_compression();
    opts.optimize_alpha = true;
    opts.scale_16 = true;
    opts.force = true;

    debug!("optimizing {}", path.as_ref().display());
    let res = oxipng::optimize_from_memory(&data, &opts)?;
    fs::File::create(path)?.write_all(&res)?;

    Ok(res.len() as u64)
}

pub fn convert_palette<'a>(palette: &[imagequant::RGBA]) -> Cow<'a, [[u8; 4]]> {
    palette
        .iter()
        .map(|color| [color.r, color.g, color.b, color.a])
        .collect()
}

pub fn image_buf_from_palette<'a>(
    width: u32,
    height: u32,
    palette: &[[u8; 4]],
    pixels: &[u8],
) -> Cow<'a, [u8]> {
    (0..width * height)
        .flat_map(|i| palette[pixels[i as usize] as usize])
        .collect()
}

/// Save sheets as PNG files.
///
/// This will also optimize the images using [oxipng].
/// When `lossy` is true the images will also be compressed using [imagequant].
/// When `group` is true and there are multiple sheets it will generate a histogram and quantize ahead of time.
pub fn save_sheets(
    sheets: &[(RgbaImage, PathBuf)],
    lossy: bool,
    group: bool,
) -> ImgUtilResult<Box<[u64]>> {
    let sheets_count = sheets.len();
    let mut sizes = Vec::with_capacity(sheets_count);
    // more than one sheet, lossy compression and grouping -> generate histogram and quantize ahead of time
    if sheets_count > 1 && lossy && group {
        info!("analyzing multiple images for quantization (grouped lossy compression)");

        let quant = quantization_attributes()?;
        let mut histo = Histogram::new(&quant);

        for (sheet, _) in sheets {
            histo.add_colors(&sheet.get_histogram(), 0.0)?;
        }

        let mut qres = histo.quantize(&quant)?;
        qres.set_dithering_level(1.0)?;
        let palette = convert_palette(qres.palette());

        info!("analyzing done, saving images");

        for (idx, (sheet, path)) in sheets.iter().enumerate() {
            trace!("saving image to {}", path.display());

            let (width, height) = sheet.dimensions();
            let w_usize = width as usize;
            let h_usize = height as usize;
            let mut img = quant.new_image(sheet.to_quant_img(), w_usize, h_usize, 0.0)?;

            let mut pxls = Vec::with_capacity(w_usize * h_usize);
            qres.remap_into_vec(&mut img, &mut pxls)?;

            sizes.push(optimize_png(
                &image_buf_from_palette(width, height, &palette, &pxls),
                width,
                height,
                path,
            )?);

            if sheets_count > 10 && (idx + 1) % 10 == 0 {
                info!("saved {}/{sheets_count}", idx + 1);
            }
        }

        if sheets_count > 10 && sheets_count % 10 != 0 {
            info!("saved {sheets_count}/{sheets_count}");
        }

        return Ok(sizes.into_boxed_slice());
    }

    // regular optimized saving
    info!("saving image(s)");
    for (idx, (sheet, path)) in sheets.iter().enumerate() {
        sizes.push(sheet.save_optimized_png(path, lossy)?);

        if sheets_count > 10 && (idx + 1) % 10 == 0 {
            info!("saved {}/{sheets_count}", idx + 1);
        }
    }

    if sheets_count > 10 && sheets_count % 10 != 0 {
        info!("saved {sheets_count}/{sheets_count}");
    }

    Ok(sizes.into_boxed_slice())
}
