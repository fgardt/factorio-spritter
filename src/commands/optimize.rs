use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::Args;

use super::CommandError;
use crate::image_util::{self, ImageBufferExt as _, ImgUtilError};

#[derive(Args, Debug)]
pub struct OptimizeArgs {
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

pub fn optimize(args: &OptimizeArgs) -> Result<(), CommandError> {
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
