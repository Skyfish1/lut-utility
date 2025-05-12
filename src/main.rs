use clap::{Args, Parser, Subcommand, ValueEnum};
use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, Pixel, Rgb, RgbImage};
use std::f32;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

/// A utility to generate and convert different types of LUT images to .cube LUTs and to apply LUTs to images. More Information: https://github.com/Skyfish1/lut-utility
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generates an identity LUT image in specified format (HALD ((√(cube³) x √(cube³))) or Unwrapped Cube (cube² x cube)).
    Generate(GenerateArgs),
    /// Converts a supported LUT image (.png) to a .cube file. Can process a single file or a directory in batch mode.
    Convert(ConvertArgs),
    /// Applies a LUT (.png or .cube) or a folder of LUTs to an image (.jpg or .png). Can process a single LUT or a folder of LUTs in batch mode.
    Apply(ApplyArgs),
    /// Shows examples of how to use the tool.
    Examples,
}

// ValueEnum for LUT format
#[derive(ValueEnum, Debug, Clone, Copy)]
enum LutFormat {
    #[clap(alias = "H")]
    Hald,
    #[clap(alias = "U")]
    UnwrappedCube,
}

// New GenerateArgs struct
#[derive(Args, Debug)]
struct GenerateArgs {
    /// The format of the identity LUT to generate (hald or unwrapped-cube).
    #[clap(short, long, value_enum)]
    format: LutFormat,

    /// The base cube size for LUT generation (is same as level^2 for HALD).
    ///   Affects density and image size:
    ///     Image dimension = HALD: √(cube³) x √(cube³) UNWRAPPED CUBE: cube² x cube.
    ///     Practical range: HALD: 1-144 UNWRAPPED CUBE 1-64. Higher values generate very large images.
    #[clap(short, long)]
    cube: u32,

    /// Output filename for the generated PNG image. Must have a .png extension.
    #[clap(short, long)]
    output: PathBuf,

    /// Desired bit depth for the output PNG (8 or 16). Only applicable for Unwrapped Cube format.
    #[clap(short, long, default_value = "8")]
    bit_depth: u8,
}

#[derive(Args, Debug)]
struct ConvertArgs {
    /// Input LUT image file (.png) or a directory containing .png files for batch conversion.
    #[clap(short, long)]
    input: PathBuf,

    /// Output .cube filename (.cube) for single file conversion, or output directory for batch conversion.
    #[clap(short, long)]
    output: PathBuf,

    /// Optional target size for the output .cube file (e.g., 32, 33, 64).
    /// Must be <= the input LUT's native cube size.
    #[clap(short, long)]
    target_cube: Option<u32>,

    /// Enable batch processing mode. Input and output must be directories.
    #[clap(short, long)]
    batch: bool,
}

#[derive(Args, Debug)]
struct ApplyArgs {
    /// Path to the LUT file (.png or .cube) or a directory containing LUT files for batch application.
    #[clap(short, long)]
    lut: PathBuf,

    /// Path to the input image file (.jpg or .png). This remains a single file in batch mode.
    #[clap(short, long)]
    input: PathBuf,

    /// Path to save the output image file (.png) for single LUT application, or output directory for batch application.
    #[clap(short, long)]
    output: PathBuf,

    /// Enable batch processing mode. LUT input must be a directory, output must be a directory.
    #[clap(short, long)]
    batch: bool,
}

// Helper function to generate LUT image data
fn generate_lut_image_data(
    format: LutFormat,
    cube_size: u32,
    bit_depth: u8,
) -> Result<(DynamicImage, u32, u32), String> {
    match format {
        LutFormat::Hald => {
            // Validate cube size for HALD
            if !(1..=400).contains(&cube_size) {
                return Err(format!(
                    "Cube size {} is out of the practical range for HALD (1-400).",
                    cube_size
                ));
            }
            // Warn if bit depth is not 8 for HALD
            if bit_depth != 8 {
                eprintln!(
                    "Warning: Bit depth {} is not typically used for HALD. Generating 8-bit.",
                    bit_depth
                );
            }

            let image_dimension = (cube_size.pow(3) as f64).sqrt() as u32;
            let mut img_buf: RgbImage = ImageBuffer::new(image_dimension, image_dimension);

            let denominator = if cube_size <= 1 {
                1.0
            } else {
                (cube_size - 1) as f32
            };

            for y in 0..image_dimension {
                for x in 0..image_dimension {
                    let pixel_idx: u32 = y * image_dimension + x;
                    let r_loop_var = pixel_idx % cube_size;
                    let g_loop_var = (pixel_idx / cube_size) % cube_size;
                    let b_loop_var = pixel_idx / (cube_size.saturating_mul(cube_size));

                    let r_float = if cube_size <= 1 {
                        0.0
                    } else {
                        r_loop_var as f32 / denominator
                    };
                    let g_float = if cube_size <= 1 {
                        0.0
                    } else {
                        g_loop_var as f32 / denominator
                    };
                    let b_float = if cube_size <= 1 {
                        0.0
                    } else {
                        b_loop_var as f32 / denominator
                    };

                    let r_u8 = (r_float.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let g_u8 = (g_float.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let b_u8 = (b_float.clamp(0.0, 1.0) * 255.0).round() as u8;

                    img_buf.put_pixel(x, y, Rgb([r_u8, g_u8, b_u8]));
                }
            }
            Ok((
                DynamicImage::ImageRgb8(img_buf),
                image_dimension,
                image_dimension,
            ))
        }
        LutFormat::UnwrappedCube => {
            // Validate cube size for Unwrapped Cube
            if !(1..=64).contains(&cube_size) {
                return Err(format!(
                    "Cube size {} is out of the practical range for Unwrapped Cube (1-64).",
                    cube_size
                ));
            }

            let width: u32 = cube_size * cube_size;
            let height: u32 = cube_size;

            if bit_depth == 8 {
                let mut img_buf: RgbImage = ImageBuffer::new(width, height);
                let denominator = if cube_size <= 1 {
                    1.0
                } else {
                    (cube_size - 1) as f64
                };

                for y in 0..height {
                    for x in 0..width {
                        let pixel_idx = y * width + x;
                        let r_idx = pixel_idx % cube_size;
                        let g_idx = y;
                        let b_idx = x / cube_size;

                        let r = r_idx as f64 / denominator;
                        let g = g_idx as f64 / denominator;
                        let b = b_idx as f64 / denominator;

                        let r_u8 = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let g_u8 = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let b_u8 = (b.clamp(0.0, 1.0) * 255.0).round() as u8;

                        img_buf.put_pixel(x, y, Rgb([r_u8, g_u8, b_u8]));
                    }
                }
                Ok((DynamicImage::ImageRgb8(img_buf), width, height))
            } else {
                // bit_depth == 16
                let mut img_buf: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::new(width, height);
                let denominator = if cube_size <= 1 {
                    1.0
                } else {
                    (cube_size - 1) as f64
                };
                let max_val_u16 = u16::MAX as f64;

                for y in 0..height {
                    for x in 0..width {
                        let pixel_idx = y * width + x;
                        let r_idx = pixel_idx % cube_size;
                        let g_idx = y;
                        let b_idx = x / cube_size;

                        let r = r_idx as f64 / denominator;
                        let g = g_idx as f64 / denominator;
                        let b = b_idx as f64 / denominator;

                        let r_u16 = (r.clamp(0.0, 1.0) * max_val_u16).round() as u16;
                        let g_u16 = (g.clamp(0.0, 1.0) * max_val_u16).round() as u16;
                        let b_u16 = (b.clamp(0.0, 1.0) * max_val_u16).round() as u16;

                        img_buf.put_pixel(x, y, Rgb([r_u16, g_u16, b_u16]));
                    }
                }
                Ok((DynamicImage::ImageRgb16(img_buf), width, height))
            }
        }
    }
}

// Helper function to save the generated image
fn save_lut_image(img: DynamicImage, output_path: &Path, bit_depth: u8) -> Result<(), String> {
    let file = File::create(output_path).map_err(|e| format!("Failed to create file: {}", e))?;
    let mut writer = BufWriter::new(file);

    let format = ImageFormat::Png; // Always save as PNG

    match bit_depth {
        8 => {
            img.to_rgb8()
                .write_to(&mut writer, format)
                .map_err(|e| format!("Failed to save 8-bit image: {}", e))?;
        }
        16 => {
            // Need to ensure the DynamicImage is in a 16-bit format if bit_depth is 16
            // The generate_lut_image_data already returns ImageRgb16 for 16-bit
            // So we can try to write directly if it's already the correct type
            if let Some(img_rgb16) = img.as_rgb16() {
                img_rgb16
                    .write_to(&mut writer, format)
                    .map_err(|e| format!("Failed to save 16-bit image: {}", e))?;
            } else {
                // Fallback or error if the image is not in the expected 16-bit format
                return Err(
                    "Internal error: Image not in expected 16-bit format for saving.".to_string(),
                );
            }
        }
        _ => return Err(format!("Unsupported bit depth for saving: {}", bit_depth)), // Should be caught earlier
    }

    println!(
        "Successfully saved identity LUT PNG to {}",
        output_path.display()
    );
    Ok(())
}

// Helper to check if dimensions match a valid square HALD CLUT (width == height == level^3)
fn is_hald(width: u32, height: u32) -> bool {
    if width != height || width < 1 {
        return false;
    }
    // Check if width is level^3 for some integer level >= 1
    let dim_f = width as f64;
    let level_float = dim_f.cbrt();
    let level_round = level_float.round();
    let level_u32 = level_round as u32;

    // Check if the rounded level, when cubed, is exactly the width
    // Also check if the float level was very close to the rounded integer
    level_u32.checked_pow(3).map_or(false, |cubed_level| cubed_level == width)
        && (level_float - level_round).abs() < 1e-6 // Tolerance for floating point
        && level_u32 >= 1 // Level must be at least 1
}

// Helper to determine the native level of a HALD CLUT from its dimension (level^3)
fn determine_hald_level_from_dim(width: u32) -> Option<u32> {
    if width < 1 {
        return None;
    }
    let dim_f = width as f64;
    let level_float = dim_f.cbrt();
    let level_round = level_float.round();
    let level_u32 = level_round as u32;

    if level_u32
        .checked_pow(3)
        .map_or(false, |cubed_level| cubed_level == width)
        && (level_float - level_round).abs() < 1e-6
        && level_u32 >= 1
    {
        Some(level_u32)
    } else {
        None
    }
}
fn determine_unwrapped_cube_root_from_dim(w: u32, h: u32) -> Option<u32> {
    let size = (w * h) as f64;
    let root = size.cbrt().round() as u32;
    if is_unwrapped_cube(w, root) {
        Some(root)
    } else {
        None
    }
}

// Helper to check if dimensions match the fixed Unwrapped Cube LUT size
fn is_unwrapped_cube(width: u32, height: u32) -> bool {
    if width <= height {
        return false;
    }

    let size = (width * height) as f64;
    let root = size.cbrt().round() as u32;

    // Check if the width is a multiple of the root and height is equal to the root
    width % root == 0 && height == root
        // Check if the cube of the root matches the area of the image
        && root.pow(3) == width * height
}

fn convert_lut_to_cube(
    input_path: &Path,
    output_path: &Path,
    target_cube_size: Option<u32>,
) -> Result<(), String> {
    // 1. Load the image
    let img = image::open(input_path)
        .map_err(|e| format!("Failed to open image {}: {}", input_path.display(), e))?;

    let (width, height) = img.dimensions();
    println!("Loaded image: {}x{}", width, height);

    let is_hald = is_hald(width, height);
    let is_unwrapped_cube = is_unwrapped_cube(width, height);

    // 2. Determine LUT type and native size based on dimensions
    let (lut_type, native_cube_size) = if is_hald {
        let hald_level = determine_hald_level_from_dim(width).ok_or_else(|| {
            format!(
                "Image dimensions {}x{} resemble HALD but are not a valid size",
                width, height
            )
        })?;
        println!(
            "Detected HALD CLUT (Level {}, cube size {} Native Size {}x{})",
            hald_level,
            hald_level * hald_level,
            width,
            height
        );
        ("HALD", hald_level * hald_level)
    } else if is_unwrapped_cube {
        let unwrapped_cube_root = determine_unwrapped_cube_root_from_dim(width, height)
            .ok_or_else(|| {
                format!(
                    "Image dimensions {}x{} resemble UNWRAPPED_CUBE but are not a valid size",
                    width, height
                )
            })?;
        println!(
            "Detected Unwrapped Cube LUT (Cube size {}, Native Size {}x{})",
            unwrapped_cube_root, width, height
        );
        ("UNWRAPPED_CUBE", unwrapped_cube_root)
    } else {
        return Err(format!(
            "Image dimensions {}x{} do not match a known LUT format (HALD or UNWRAPPED_CUBE)",
            width, height
        ));
    };

    // 3. Determine output cube size and validate against native size
    let output_size = target_cube_size.unwrap_or(native_cube_size);

    if output_size == 0 {
        return Err("Target cube size must be at least 1.".to_string());
    }
    if output_size > native_cube_size {
        return Err(format!(
            "Cannot upscale LUT. Target cube size {} must be <= input native resolution {}",
            output_size, native_cube_size
        ));
    }
    if output_size < 2 && native_cube_size >= 2 {
        println!(
            "Warning: Target cube size {} is very small and may lose significant precision.",
            output_size
        );
    }

    // 4. Prepare .cube file writer
    let mut writer = BufWriter::new(File::create(output_path).map_err(|e| {
        format!(
            "Failed to create .cube file {}: {}",
            output_path.display(),
            e
        )
    })?);

    writeln!(writer, "TITLE \"Generated from {} LUT\"", lut_type).map_err(|e| {
        format!(
            "Failed to write to .cube file {}: {}",
            output_path.display(),
            e
        )
    })?;
    writeln!(writer, "LUT_3D_SIZE {}", output_size).map_err(|e| {
        format!(
            "Failed to write to .cube file {}: {}",
            output_path.display(),
            e
        )
    })?;
    writeln!(writer, "DOMAIN_MIN 0.0 0.0 0.0").map_err(|e| {
        format!(
            "Failed to write to .cube file {}: {}",
            output_path.display(),
            e
        )
    })?;
    writeln!(writer, "DOMAIN_MAX 1.0 1.0 1.0").map_err(|e| {
        format!(
            "Failed to write to .cube file {}: {}",
            output_path.display(),
            e
        )
    })?;

    if is_hald {
        convert_hald_to_cube_data(output_path, &img, width, native_cube_size, output_size, &mut writer)?;
    } else if is_unwrapped_cube {
        convert_unwrapped_cube_to_cube_data(
            output_path,
            &img,
            width,
            native_cube_size,
            output_size,
            &mut writer,
        )?;
    }

    writer
        .flush() // Ensure all data is written
        .map_err(|e| {
            format!(
                "Failed to flush writer for .cube file {}: {}",
                output_path.display(),
                e
            )
        })?;

    println!("Successfully converted LUT to: {}", output_path.display());
    Ok(())
}

fn convert_unwrapped_cube_to_cube_data(
    output_path: &Path,
    img: &DynamicImage,
    width: u32,
    input_cube_size: u32,
    output_cube_size: u32,
    writer: &mut BufWriter<File>,
) -> Result<(), String> {
    // Write the LUT data.
    // The .cube format expects the output colors for input colors
    // (i/(N-1), j/(N-1), k/(N-1)) in the order of k (Blue) varying slowest,
    // then j (Green), then i (Red) varying fastest.
    // We iterate through the output cube indices (i, j, k) and map them
    // to the corresponding pixel coordinates in the input PNG.
    let height = input_cube_size;

    let scaling_factor = if output_cube_size > 1 {
        (input_cube_size - 1) as f32 / (output_cube_size - 1) as f32
    } else {
        0.0 // Special case for output_cube_size == 1
    };

    for k in 0..output_cube_size {
        // Iterate through Blue dimension of the output cube
        for j in 0..output_cube_size {
            // Iterate through Green dimension of the output cube
            for i in 0..output_cube_size {
                // Iterate through Red dimension of the output cube

                // Calculate the corresponding indices in the input cube space (0 to input_cube_size - 1).
                // We scale the output indices (i, j, k) to the input range.
                let i_in = if output_cube_size > 1 {
                    (i as f32 * scaling_factor).round() as u32
                } else {
                    0 // For output_cube_size == 1, sample the first pixel of the first square.
                };
                let j_in = if output_cube_size > 1 {
                    (j as f32 * scaling_factor).round() as u32
                } else {
                    0
                };
                let k_in = if output_cube_size > 1 {
                    (k as f32 * scaling_factor).round() as u32
                } else {
                    0
                };

                // Calculate the pixel coordinates in the input PNG image for the
                // input color corresponding to (i_in, j_in, k_in) in the input cube space.
                // The x-coordinate is determined by the Blue value (k_in) which selects the square,
                // and the Red value (i_in) within that square.
                // The y-coordinate is determined by the Green value (j_in).
                let x = k_in * input_cube_size + i_in;
                let y = j_in;

                // Ensure the calculated coordinates are within the image bounds (should be, but good practice).
                if x >= width || y >= height {
                    eprintln!(
                        "Warning: Calculated pixel coordinates ({}, {}) are out of image bounds ({}, {}). This should not happen with correct input format and size handling.",
                        x, y, width, height
                    );
                    // Fallback to a default color or skip, depending on desired behavior.
                    // For now, let's just use the last valid pixel if out of bounds.
                    let clamped_x = x.min(width - 1);
                    let clamped_y = y.min(height - 1);
                    let pixel = img.get_pixel(clamped_x, clamped_y);
                    let rgb = pixel.to_rgb();
                    let r_norm = rgb[0] as f32 / 255.0;
                    let g_norm = rgb[1] as f32 / 255.0;
                    let b_norm = rgb[2] as f32 / 255.0;
                    writeln!(writer, "{:.6} {:.6} {:.6}", r_norm, g_norm, b_norm).map_err(|e| {
                        format!(
                            "Failed to write pixel data for point ({},{},{}) to .cube file {}: {}",
                            r_norm, g_norm, b_norm, output_path.display(), e
                        )
                    })?;
                    continue; // Move to the next iteration
                }

                // Get the pixel color at the calculated coordinates.
                let pixel = img.get_pixel(x, y);
                // Convert the pixel to an 8-bit RGB format (0-255 per channel).
                // Use to_rgb() which converts to Rgb<u8> for 8-bit images.
                let rgb = pixel.to_rgb();

                // Normalize the RGB values from 0-255 to 0.0-1.0 for the .cube file.
                let r_norm = rgb[0] as f32 / 255.0;
                let g_norm = rgb[1] as f32 / 255.0;
                let b_norm = rgb[2] as f32 / 255.0;

                // Write the normalized RGB values to the .cube file, formatted to 6 decimal places.
                writeln!(writer, "{:.6} {:.6} {:.6}", r_norm, g_norm, b_norm).map_err(|e| {
                    format!(
                        "Failed to write pixel data for point ({},{},{}) to .cube file {}: {}",
                        r_norm, g_norm, b_norm, output_path.display(), e
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn convert_hald_to_cube_data(
    output_path: &Path,
    img: &DynamicImage,
    width: u32,
    native_lut_size: u32,
    output_size: u32,
    writer: &mut BufWriter<File>,
) -> Result<(), String> {
    // 5. Generate cube entries
    let output_size_f = output_size as f32;
    let denominator_f = if output_size <= 1 {
        1.0
    } else {
        output_size_f - 1.0
    };

    for b in 0..output_size {
        // B varies slowest (outer loop for cube)
        for g in 0..output_size {
            // G varies middle
            for r in 0..output_size {
                // R varies fastest
                // Normalize cube coordinates (0.0 to 1.0)
                let r_norm = r as f32 / denominator_f;
                let g_norm = g as f32 / denominator_f;
                let b_norm = b as f32 / denominator_f;

                // Map (r_norm, g_norm, b_norm) back to HALD grid indices and sample pixel
                let hald_native_size_f = native_lut_size as f32;
                let hald_denominator_f = if native_lut_size <= 1 {
                    1.0
                } else {
                    hald_native_size_f - 1.0
                };

                let r_idx = (r_norm * hald_denominator_f).round() as u32;
                let g_idx = (g_norm * hald_denominator_f).round() as u32;
                let b_idx = (b_norm * hald_denominator_f).round() as u32;

                // Calculate linear index in the HALD image corresponding to (r_idx, g_idx, b_idx)
                // This must match the pixel layout order used during HALD generation (B, G, R fastest)
                let index = b_idx
                    .saturating_mul(native_lut_size)
                    .saturating_mul(native_lut_size)
                    + g_idx.saturating_mul(native_lut_size)
                    + r_idx;

                // Calculate pixel coordinates in the HALD image
                let image_dimension = width; // Assuming width == height for HALD
                let x = index % image_dimension;
                let y = index / image_dimension;

                // Get pixel color from the loaded image
                let pixel = img.get_pixel(x, y);
                let pixel_color = Rgb([pixel[0], pixel[1], pixel[2]]); // Ensure we get RGB channels

                // Write color to .cube file (normalized 0.0-1.0)
                writeln!(
                    writer,
                    "{:.6} {:.6} {:.6}",
                    pixel_color[0] as f32 / 255.0,
                    pixel_color[1] as f32 / 255.0,
                    pixel_color[2] as f32 / 255.0
                )
                .map_err(|e| {
                    format!(
                        "Failed to write pixel data for point ({},{},{}) to .cube file {}: {}",
                        r, g, b, output_path.display(), e
                    )
                })?;
            }
        }
    }
    Ok(())
}

// apply_lut_to_image function
fn apply_lut_to_image(
    lut_path: &Path,
    input_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    // 1. Load the input image
    let input_img = image::open(input_path)
        .map_err(|e| format!("Failed to open input image {}: {}", input_path.display(), e))?;

    // Convert input image to RGB if it's not already (e.g., grayscale, RGBA)
    let input_img_rgb = input_img.to_rgb8();

    // 2. Load the LUT data and determine its properties (size, data)
    let lut_data = load_lut_data(lut_path)?;
    let lut_size = lut_data.size;
    let lut_values = lut_data.values; // Vec<[f32; 3]> flattened 3D array

    // 3. Create a new output image buffer
    let (width, height) = input_img_rgb.dimensions();
    let mut output_img: RgbImage = ImageBuffer::new(width, height);

    // 4. Iterate over input image pixels and apply the LUT
    for y in 0..height {
        for x in 0..width {
            let pixel = input_img_rgb.get_pixel(x, y);
            // Convert pixel to normalized RGB (0.0-1.0)
            let r_in = pixel[0] as f32 / 255.0;
            let g_in = pixel[1] as f32 / 255.0;
            let b_in = pixel[2] as f32 / 255.0;

            // Sample the LUT to get the output color
            let out_color = sample_lut(&lut_values, lut_size, r_in, g_in, b_in);

            // Convert output color back to u8 (0-255)
            let r_out_u8 = (out_color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            let g_out_u8 = (out_color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
            let b_out_u8 = (out_color[2].clamp(0.0, 1.0) * 255.0).round() as u8;

            output_img.put_pixel(x, y, Rgb([r_out_u8, g_out_u8, b_out_u8]));
        }
    }

    // 5. Save the output image
    output_img.save(output_path).map_err(|e| {
        format!(
            "Failed to save output image {}: {}",
            output_path.display(),
            e
        )
    })?;

    println!(
        "Successfully applied LUT to {} and saved to {}",
        input_path.display(),
        output_path.display()
    );

    Ok(())
}

// struct to hold loaded LUT data
struct LutData {
    size: u32,
    values: Vec<[f32; 3]>, // Flattened 3D array: index = k * size * size + j * size + i
}

// load_lut_data function to handle different LUT file types
fn load_lut_data(lut_path: &Path) -> Result<LutData, String> {
    let extension = lut_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "png" => load_png_lut(lut_path),
        "cube" => load_cube_lut(lut_path),
        _ => Err(format!("Unsupported LUT file extension: .{}", extension)),
    }
}

// load_png_lut function to load LUT data from HALD or Unwrapped Cube PNGs
fn load_png_lut(png_path: &Path) -> Result<LutData, String> {
    let img = image::open(png_path)
        .map_err(|e| format!("Failed to open LUT image {}: {}", png_path.display(), e))?;

    let (width, height) = img.dimensions();

    let is_hald = is_hald(width, height);
    let is_unwrapped_cube = is_unwrapped_cube(width, height);

    let lut_size= if is_hald {
        let hald_level = determine_hald_level_from_dim(width).ok_or_else(|| {
            format!(
                "LUT image dimensions {}x{} resemble HALD but are not a valid size",
                width, height
            )
        })?;
        println!(
            "Detected HALD CLUT (Level {}, cube size {})",
            hald_level,
            hald_level * hald_level
        );
        hald_level * hald_level // HALD size is level^2
    } else if is_unwrapped_cube {
        let unwrapped_cube_root = determine_unwrapped_cube_root_from_dim(width, height)
            .ok_or_else(|| {
                format!(
                    "LUT image dimensions {}x{} resemble UNWRAPPED_CUBE but are not a valid size",
                    width, height
                )
            })?;
        println!(
            "Detected Unwrapped Cube LUT (Cube size {})",
            unwrapped_cube_root
        );
        unwrapped_cube_root // Unwrapped Cube size is the root
    } else {
        return Err(format!(
            "LUT image dimensions {}x{} do not match a known LUT format (HALD or UNWRAPPED_CUBE)",
            width, height
        ));
    };

    // Extract LUT values from the PNG
    let mut values: Vec<[f32; 3]> = Vec::with_capacity((lut_size * lut_size * lut_size) as usize);

    // Convert image to RGB8 for consistent pixel access
    let img_rgb8 = img.to_rgb8();

    if is_hald {
        let image_dimension = width; // width == height for HALD

        for b_idx in 0..lut_size {
            for g_idx in 0..lut_size {
                for r_idx in 0..lut_size {
                    // Calculate linear index in the HALD image corresponding to (r_idx, g_idx, b_idx)
                    let index = b_idx.saturating_mul(lut_size).saturating_mul(lut_size)
                        + g_idx.saturating_mul(lut_size)
                        + r_idx;

                    // Calculate pixel coordinates in the HALD image
                    let x = index % image_dimension;
                    let y = index / image_dimension;

                    let pixel = img_rgb8.get_pixel(x, y);
                    values.push([
                        pixel[0] as f32 / 255.0,
                        pixel[1] as f32 / 255.0,
                        pixel[2] as f32 / 255.0,
                    ]);
                }
            }
        }
    } else if is_unwrapped_cube {
        let unwrapped_cube_root = lut_size; // Unwrapped Cube size is the root

        for b_idx in 0..unwrapped_cube_root {
            // Corresponds to squares
            for g_idx in 0..unwrapped_cube_root {
                // Corresponds to rows in a square
                for r_idx in 0..unwrapped_cube_root {
                    // Corresponds to columns in a square
                    // Calculate pixel coordinates in the unwrapped cube image
                    let x = b_idx * unwrapped_cube_root + r_idx;
                    let y = g_idx;

                    let pixel = img_rgb8.get_pixel(x, y);
                    values.push([
                        pixel[0] as f32 / 255.0,
                        pixel[1] as f32 / 255.0,
                        pixel[2] as f32 / 255.0,
                    ]);
                }
            }
        }
    }

    // Double check that the number of values matches the expected size^3
    if values.len() != (lut_size.pow(3)) as usize {
        return Err(format!(
            "Mismatch between expected LUT size ({}^3 = {}) and number of values read ({}) from PNG.",
            lut_size,
            lut_size.pow(3),
            values.len()
        ));
    }

    Ok(LutData {
        size: lut_size,
        values,
    })
}

// load_cube_lut function to parse .cube files
fn load_cube_lut(cube_path: &Path) -> Result<LutData, String> {
    let file = File::open(cube_path)
        .map_err(|e| format!("Failed to open .cube file {}: {}", cube_path.display(), e))?;
    let reader = BufReader::new(file);

    let mut lut_size: Option<u32> = None;
    let mut values: Vec<[f32; 3]> = Vec::new();
    let mut reading_data = false;

    for line in reader.lines() {
        let line =
            line.map_err(|e| format!("Error reading .cube file {}: {}", cube_path.display(), e))?;
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue; // Skip empty lines and comments
        }

        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 2 && parts[0].to_ascii_uppercase() == "LUT_3D_SIZE" {
            if let Ok(size) = parts[1].parse::<u32>() {
                lut_size = Some(size);
                values.reserve((size.pow(3)) as usize); // Reserve space
                reading_data = true; // Start reading data after finding size
            } else {
                return Err(format!(
                    "Invalid LUT_3D_SIZE value in {}: {}",
                    cube_path.display(),
                    parts[1]
                ));
            }
        } else if reading_data && parts.len() == 3 {
            // Assuming we are reading RGB data points
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].parse::<f32>(),
                parts[1].parse::<f32>(),
                parts[2].parse::<f32>(),
            ) {
                values.push([r, g, b]);
            } else {
                // If we are reading data but the line doesn't have 3 floats, it might be an error or unexpected format
                eprintln!(
                    "Warning: Skipping unexpected data line in {}: {}",
                    cube_path.display(),
                    line
                );
            }
        }
        // Ignore other header lines like TITLE, DOMAIN_MIN, DOMAIN_MAX
    }

    let final_lut_size = lut_size.ok_or_else(|| {
        format!(
            "LUT_3D_SIZE not found in .cube file {}",
            cube_path.display()
        )
    })?;

    // Check if the number of values read matches the expected size^3
    if values.len() != (final_lut_size.pow(3)) as usize {
        // This might happen if the file is truncated or malformed after the size is declared
        return Err(format!(
            "Mismatch between expected LUT size ({}^3 = {}) and number of values read ({}) from .cube file {}. File might be truncated or malformed.",
            final_lut_size,
            final_lut_size.pow(3),
            values.len(),
            cube_path.display()
        ));
    }

    Ok(LutData {
        size: final_lut_size,
        values,
    })
}

// Samples the LUT using trilinear interpolation.
// lut_values: Flattened 3D array of output colors [R, G, B]
// lut_size: The size of the 3D LUT (N for NxNxN)
// r_in, g_in, b_in: Input color (normalized 0.0-1.0)
fn sample_lut(
    lut_values: &Vec<[f32; 3]>,
    lut_size: u32,
    r_in: f32,
    g_in: f32,
    b_in: f32,
) -> [f32; 3] {
    let size_f = lut_size as f32;
    let max_index = size_f - 1.0;

    // Clamp input color to the 0.0-1.0 range
    let r_in = r_in.clamp(0.0, 1.0);
    let g_in = g_in.clamp(0.0, 1.0);
    let b_in = b_in.clamp(0.0, 1.0);

    // Scale input color to LUT grid coordinates (0 to size-1)
    let r_scaled = r_in * max_index;
    let g_scaled = g_in * max_index;
    let b_scaled = b_in * max_index;

    // Get the indices of the 8 surrounding grid points
    let r0 = r_scaled.floor() as u32;
    let g0 = g_scaled.floor() as u32;
    let b0 = b_scaled.floor() as u32;

    // Clamp indices to the valid range [0, lut_size - 1]
    let r0 = r0.min(lut_size - 1);
    let g0 = g0.min(lut_size - 1);
    let b0 = b0.min(lut_size - 1);

    let r1 = (r0 + 1).min(lut_size - 1);
    let g1 = (g0 + 1).min(lut_size - 1);
    let b1 = (b0 + 1).min(lut_size - 1);

    // Get the fractional part for interpolation
    let r_frac = r_scaled - r_scaled.floor();
    let g_frac = g_scaled - g_scaled.floor();
    let b_frac = b_scaled - b_scaled.floor();

    // Get the colors at the 8 surrounding grid points
    // The index in the flattened array is k * size * size + j * size + i
    let c000 = lut_values[(b0 * lut_size * lut_size + g0 * lut_size + r0) as usize];
    let c100 = lut_values[(b0 * lut_size * lut_size + g0 * lut_size + r1) as usize];
    let c010 = lut_values[(b0 * lut_size * lut_size + g1 * lut_size + r0) as usize];
    let c110 = lut_values[(b0 * lut_size * lut_size + g1 * lut_size + r1) as usize];
    let c001 = lut_values[(b1 * lut_size * lut_size + g0 * lut_size + r0) as usize];
    let c101 = lut_values[(b1 * lut_size * lut_size + g0 * lut_size + r1) as usize];
    let c011 = lut_values[(b1 * lut_size * lut_size + g1 * lut_size + r0) as usize];
    let c111 = lut_values[(b1 * lut_size * lut_size + g1 * lut_size + r1) as usize];

    // Perform trilinear interpolation
    // Interpolate along Red axis
    let c00 = [
        c000[0] * (1.0 - r_frac) + c100[0] * r_frac,
        c000[1] * (1.0 - r_frac) + c100[1] * r_frac,
        c000[2] * (1.0 - r_frac) + c100[2] * r_frac,
    ];
    let c01 = [
        c010[0] * (1.0 - r_frac) + c110[0] * r_frac,
        c010[1] * (1.0 - r_frac) + c110[1] * r_frac,
        c010[2] * (1.0 - r_frac) + c110[2] * r_frac,
    ];
    let c10 = [
        c001[0] * (1.0 - r_frac) + c101[0] * r_frac,
        c001[1] * (1.0 - r_frac) + c101[1] * r_frac,
        c101[2] * (1.0 - r_frac) + c101[2] * r_frac,
    ];
    let c11 = [
        c011[0] * (1.0 - r_frac) + c111[0] * r_frac,
        c011[1] * (1.0 - r_frac) + c111[1] * r_frac,
        c011[2] * (1.0 - r_frac) + c111[2] * r_frac,
    ];

    // Interpolate along Green axis
    let c0 = [
        c00[0] * (1.0 - g_frac) + c01[0] * g_frac,
        c00[1] * (1.0 - g_frac) + c01[1] * g_frac,
        c00[2] * (1.0 - g_frac) + c01[2] * g_frac,
    ];
    let c1 = [
        c10[0] * (1.0 - g_frac) + c11[0] * g_frac,
        c10[1] * (1.0 - g_frac) + c11[1] * g_frac,
        c10[2] * (1.0 - g_frac) + c11[2] * g_frac,
    ];

    // Interpolate along Blue axis
    let final_color = [
        c0[0] * (1.0 - b_frac) + c1[0] * b_frac,
        c0[1] * (1.0 - b_frac) + c1[1] * b_frac,
        c0[2] * (1.0 - b_frac) + c1[2] * b_frac,
    ];

    final_color
}

// Function to print examples
fn run_examples() {
    println!("EXAMPLES:");
    println!();
    println!("Generate an 8-bit HALD LUT with cube size 64:");
    println!("  lut_tool generate --format hald --cube 64 --output hald_64.png");
    println!();
    println!("Generate a 16-bit Unwrapped Cube LUT with cube size 33:");
    println!(
        "  lut_tool generate --format unwrapped-cube --cube 33 --bit-depth 16 --output unwrapped_33_16bit.png"
    );
    println!();
    println!("Convert a single Unwrapped Cube PNG to a .cube LUT:");
    println!("  lut_tool convert --input unwrapped_33.png --output unwrapped_33.cube");
    println!();
    println!("Convert a HALD PNG and reduce to a 33-point .cube LUT:");
    println!("  lut_tool convert --input hald_64.png --output hald_64_to_33.cube --target-cube 33");
    println!();
    println!("Batch convert all PNG LUTs in a folder (and subfolders) to .cube files:");
    println!("  lut_tool convert --input ./input_luts --output ./output_cubes --batch");
    println!();
    println!("Apply a single .cube LUT to an image:");
    println!(
        "  lut_tool apply --lut my_color_grade.cube --input photo.jpg --output photo_graded.png"
    );
    println!();
    println!("Apply a HALD LUT PNG to an image:");
    println!(
        "  lut_tool apply --lut hald_64.png --input photo.png --output photo_hald_applied.png"
    );
    println!();
    println!("Apply all LUTs in a folder (and subfolders) to a single image:");
    println!(
        "  lut_tool apply --lut ./lut_collection --input photo.jpg --output ./applied_images --batch"
    );
}

// Helper function to process files in a directory recursively
fn process_directory<F>(
    input_dir: &Path,
    output_dir: &Path,
    file_extension: &str, // This is the filter for input files
    processor: &mut F,
) -> Result<(), String>
where
    F: FnMut(&Path, &Path) -> Result<(), String>,
{
    // Ensure output directory exists
    fs::create_dir_all(output_dir).map_err(|e| {
        format!(
            "Failed to create output directory {}: {}",
            output_dir.display(),
            e
        )
    })?;

    for entry in fs::read_dir(input_dir).map_err(|e| {
        format!(
            "Failed to read input directory {}: {}",
            input_dir.display(),
            e
        )
    })? {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            // Recursively process subdirectories
            let relative_path = path.strip_prefix(input_dir).map_err(|e| {
                format!("Failed to strip prefix from path {}: {}", path.display(), e)
            })?;
            let new_output_dir = output_dir.join(relative_path);
            process_directory(&path, &new_output_dir, file_extension, processor)?;
        } else if path.is_file() {
            // Process files with the specified extension
            let process_this_file = if file_extension.is_empty() {
                // If file_extension is empty, process all files
                true
            } else {
                // Otherwise, check if the file extension matches
                path.extension()
                    .map_or(false, |ext| ext.to_ascii_lowercase() == file_extension)
            };

            if process_this_file {
                let relative_path = path.strip_prefix(input_dir).map_err(|e| {
                    format!("Failed to strip prefix from path {}: {}", path.display(), e)
                })?;
                let output_file_path = output_dir.join(relative_path);

                // Ensure the parent directory for the output file exists
                if let Some(parent) = output_file_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!(
                            "Failed to create output file parent directory {}: {}",
                            parent.display(),
                            e
                        )
                    })?;
                }

                // Call the provided processor function
                processor(&path, &output_file_path)?;
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate(args) => {
            // Validate output path extension for Generate
            if args
                .output
                .extension()
                .map_or(true, |ext| ext.to_ascii_lowercase() != "png")
            {
                return Err("Output filename for generate must have a .png extension.".to_string());
            }

            let (img, width, height) =
                generate_lut_image_data(args.format, args.cube, args.bit_depth)?;
            println!("Generated image dimensions: {}x{}", width, height);
            save_lut_image(img, &args.output, args.bit_depth)?;
            Ok(())
        }
        Commands::Convert(args) => {
            // Validate input path extension for Convert (only PNG allowed for input)
            if !args.batch
                && args
                    .input
                    .extension()
                    .map_or(true, |ext| ext.to_ascii_lowercase() != "png")
            {
                return Err(
                    "Input filename for single convert must have a .png extension.".to_string(),
                );
            }
            // Validate output path extension for Convert (only cube allowed for output)
            if !args.batch
                && args
                    .output
                    .extension()
                    .map_or(true, |ext| ext.to_ascii_lowercase() != "cube")
            {
                return Err(
                    "Output filename for single convert must have a .cube extension.".to_string(),
                );
            }

            if args.batch {
                // Batch conversion: input is a directory of PNGs, output is a directory of .cube files
                if !args.input.is_dir() {
                    return Err(format!(
                        "Input path {} is not a directory for batch conversion.",
                        args.input.display()
                    ));
                }
                if !args.output.is_dir() {
                    // Try to create the output directory if it doesn't exist
                    fs::create_dir_all(&args.output).map_err(|e| {
                        format!(
                            "Failed to create output directory {}: {}",
                            args.output.display(),
                            e
                        )
                    })?;
                }

                println!(
                    "Starting batch conversion from {} to {}",
                    args.input.display(),
                    args.output.display()
                );

                process_directory(
                    &args.input,
                    &args.output,
                    "png",
                    &mut |input_file: &Path, output_file: &Path| {
                        // Construct the output .cube file path
                        let output_cube_path = output_file.with_extension("cube");
                        // Call the single file conversion logic
                        convert_lut_to_cube(input_file, &output_cube_path, args.target_cube)
                    },
                )?;

                println!("Batch conversion completed.");
            } else {
                // Single file conversion
                convert_lut_to_cube(&args.input, &args.output, args.target_cube)?;
            }
            Ok(())
        }
        Commands::Apply(args) => {
            // Validate output path extension for Apply (only PNG allowed for output)
            if !args.batch
                && args
                    .output
                    .extension()
                    .map_or(true, |ext| ext.to_ascii_lowercase() != "png")
            {
                return Err(
                    "Output filename for single apply must have a .png extension.".to_string(),
                );
            }
            // Validate input image extension for Apply
            let input_ext = args
                .input
                .extension()
                .map_or("", |ext| ext.to_str().unwrap_or(""))
                .to_ascii_lowercase();
            if input_ext != "png" && input_ext != "jpg" && input_ext != "jpeg" {
                return Err(
                    "Input image filename for apply must have a .png, .jpg, or .jpeg extension."
                        .to_string(),
                );
            }
            // Validate LUT input for Apply (file or directory)
            if !args.batch && !args.lut.is_file() {
                return Err(format!(
                    "LUT path {} is not a file for single apply.",
                    args.lut.display()
                ));
            }
            if args.batch && !args.lut.is_dir() {
                return Err(format!(
                    "LUT path {} is not a directory for batch apply.",
                    args.lut.display()
                ));
            }

            if args.batch {
                // Batch application: input is a single image, lut is a directory of LUTs, output is a directory
                if !args.output.is_dir() {
                    // Try to create the output directory if it doesn't exist
                    fs::create_dir_all(&args.output).map_err(|e| {
                        format!(
                            "Failed to create output directory {}: {}",
                            args.output.display(),
                            e
                        )
                    })?;
                }

                // Create a subdirectory in the output named after the input image
                let input_image_filename = args
                    .input
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("applied_results");
                let image_output_dir = args.output.join(input_image_filename);

                println!(
                    "Starting batch application of LUTs from {} to image {} and saving to {}",
                    args.lut.display(),
                    args.input.display(),
                    image_output_dir.display()
                );

                // Process LUT files in the LUT directory
                process_directory(
                    &args.lut,
                    &image_output_dir,
                    "", // process all entries, filter inside the closure
                    &mut |lut_file: &Path, output_base_path: &Path| {
                        let extension = lut_file.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
                        if extension != "png" && extension != "cube" {
                            println!("Skipping unsupported file in LUT directory: {}", lut_file.display());
                            return Ok(()); // Skip this file and continue
                        }
                        // Determine output filename based on LUT filename
                        let lut_filename = lut_file
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("applied_lut");
                        let output_image_path =
                            output_base_path.with_file_name(format!("{}.png", lut_filename));

                        // Call the single file application logic
                        apply_lut_to_image(lut_file, &args.input, &output_image_path)
                    },
                )?;

                println!("Batch application completed.");
            } else {
                // Single LUT application
                apply_lut_to_image(&args.lut, &args.input, &args.output)?;
            }
            Ok(())
        }
        Commands::Examples => {
            // Match the new Examples subcommand
            run_examples(); // Call the function to print examples
            Ok(())
        }
    }
}
