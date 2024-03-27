use std::{fs, ops::Deref, path::Path};

use image::{
    codecs::png, EncodableLayout, ImageBuffer, ImageEncoder, Pixel, PixelWithColorType, RgbaImage,
};

#[derive(Debug, thiserror::Error)]
pub enum ImgUtilError {
    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("image error: {0}")]
    ImageError(#[from] image::ImageError),

    #[error("no images to crop")]
    NoImagesToCrop,

    #[error("all images must be the same size")]
    NotSameSize,

    #[error("unable to crop, all images are empty")]
    AllImagesEmpty,
}

type ImgUtilResult<T> = std::result::Result<T, ImgUtilError>;

pub fn load_from_path(path: &Path) -> ImgUtilResult<Vec<RgbaImage>> {
    if !path.exists() {
        return Err(ImgUtilError::IOError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("path not found: {}", path.display()),
        )));
    }

    if path.is_file() && path.extension().unwrap_or_default() == "png" {
        return Ok(vec![load_image_from_file(path)?]);
    }

    let mut images = Vec::new();
    let mut files = fs::read_dir(path)?
        .filter_map(|res| res.map_or(None, |e| Some(e.path())))
        .collect::<Vec<_>>();

    files.sort();

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

        images.push(load_image_from_file(&path)?);
    }

    Ok(images)
}

fn load_image_from_file(path: &Path) -> ImgUtilResult<RgbaImage> {
    let image = image::open(path)?.to_rgba8();
    Ok(image)
}

pub fn crop_images(images: &mut Vec<RgbaImage>) -> ImgUtilResult<(f64, f64)> {
    if images.is_empty() {
        return Err(ImgUtilError::NoImagesToCrop);
    }

    #[allow(clippy::unwrap_used)]
    let (raw_width, raw_height) = images.first().unwrap().dimensions();

    let mut min_x = std::u32::MAX;
    let mut min_y = std::u32::MAX;
    let mut max_x = std::u32::MIN;
    let mut max_y = std::u32::MIN;

    for image in images.iter() {
        // ensure image has same size
        if image.width() != raw_width || image.height() != raw_height {
            return Err(ImgUtilError::NotSameSize);
        }

        let mut x = image
            .enumerate_pixels()
            .filter_map(|(x, _, pxl)| if pxl[3] > 0 { Some(x) } else { None })
            .collect::<Vec<_>>();
        x.sort_unstable();

        let mut y = image
            .enumerate_pixels()
            .filter_map(|(_, y, pxl)| if pxl[3] > 0 { Some(y) } else { None })
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

    // are all images are empty? (or some other edge case?)
    if min_x == std::u32::MAX
        || min_y == std::u32::MAX
        || max_x == std::u32::MIN
        || max_y == std::u32::MIN
    {
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
    let shift_x = -((f64::from(raw_width - cropped_width) / 2.0) - f64::from(min_x));
    let shift_y = -((f64::from(raw_height - cropped_height) / 2.0) - f64::from(min_y));

    trace!("shifted by ({shift_x}, {shift_y})");

    Ok((shift_x, shift_y))
}

pub trait ImageBufferExt<P, C> {
    fn save_optimized_png(&self, path: impl AsRef<Path>) -> ImgUtilResult<()>;
}

impl<P, C> ImageBufferExt<P, C> for ImageBuffer<P, C>
where
    P: Pixel + PixelWithColorType,
    [P::Subpixel]: EncodableLayout,
    C: Deref<Target = [P::Subpixel]>,
{
    fn save_optimized_png(&self, path: impl AsRef<Path>) -> ImgUtilResult<()> {
        let mut file = fs::File::create(path)?;

        let (width, height) = self.dimensions();
        png::PngEncoder::new_with_quality(
            &mut file,
            png::CompressionType::Best,
            png::FilterType::default(),
        )
        .write_image(
            self.as_bytes(),
            width,
            height,
            <P as PixelWithColorType>::COLOR_TYPE,
        )?;

        Ok(())
    }
}
