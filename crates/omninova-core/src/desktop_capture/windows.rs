//! Windows desktop capture implementation using the screenshots crate.

use crate::desktop_capture::{calculate_file_hash, CaptureResult};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Capture the primary screen and save to the specified directory.
pub async fn capture_screen(captures_dir: &Path, prefix: &str) -> CaptureResult {
    // Ensure captures directory exists
    if let Err(e) = tokio::fs::create_dir_all(captures_dir).await {
        return CaptureResult::failure(
            "dir_creation_failed",
            format!("Failed to create captures directory: {}", e),
        );
    }

    // Generate filename with timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let filename = format!("{}_{}.png", prefix, timestamp);
    let file_path = captures_dir.join(&filename);
    let file_path_str = file_path.to_string_lossy().to_string();

    // Capture screenshot using screenshots crate
    match screenshots::Screen::all() {
        Ok(screens) => {
            // Get primary screen (first one)
            match screens.into_iter().next() {
                Some(screen) => {
                    match screen.capture() {
                        Ok(image) => {
                            // Get dimensions
                            let width = image.width();
                            let height = image.height();
                            let rgba_data = image.as_raw().to_vec();

                            // Write directly to file using png crate
                            let write_result = tokio::task::spawn_blocking({
                                let file_path = file_path.clone();
                                let rgba_data = rgba_data.clone();
                                move || {
                                    write_png_to_file(&file_path, &rgba_data, width, height)
                                }
                            }).await;

                            match write_result {
                                Ok(Ok(())) => {
                                    // Calculate file hash
                                    let hash = calculate_file_hash(&file_path).unwrap_or_default();
                                    
                                    // Get file size
                                    let file_size = tokio::fs::metadata(&file_path).await
                                        .map(|m| m.len())
                                        .unwrap_or(0);

                                    CaptureResult::success(
                                        file_path_str,
                                        width,
                                        height,
                                        file_size,
                                        hash,
                                    )
                                }
                                Ok(Err(e)) => {
                                    CaptureResult::failure(
                                        "png_encoding_failed",
                                        format!("Failed to encode PNG: {}", e),
                                    )
                                }
                                Err(e) => {
                                    CaptureResult::failure(
                                        "task_join_failed",
                                        format!("Failed to complete PNG encoding: {}", e),
                                    )
                                }
                            }
                        }
                        Err(e) => {
                            CaptureResult::failure(
                                "capture_failed",
                                format!("Failed to capture screen: {}. Please ensure screen capture permissions are granted.", e),
                            )
                        }
                    }
                }
                None => {
                    CaptureResult::failure(
                        "no_screen_available",
                        "No screen available for capture. This may occur in headless environments or when no display is connected.",
                    )
                }
            }
        }
        Err(e) => {
            CaptureResult::failure(
                "screenshots_init_failed",
                format!("Failed to initialize screenshots library: {}. This may indicate no desktop session is available.", e),
            )
        }
    }
}

/// Write PNG data to file (blocking, runs in spawn_blocking)
fn write_png_to_file(file_path: &Path, rgba_data: &[u8], width: u32, height: u32) -> Result<(), String> {
    use std::fs::File;
    use std::io::BufWriter;
    
    let file = File::create(file_path).map_err(|e| format!("Failed to create file: {}", e))?;
    let mut writer = BufWriter::new(file);

    let mut encoder = png::Encoder::new(&mut writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    
    let mut writer = encoder.write_header().map_err(|e| format!("PNG header error: {}", e))?;
    writer.write_image_data(rgba_data).map_err(|e| format!("PNG data error: {}", e))?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_placeholder() {
        // Placeholder test - actual capture tests require Windows environment
        assert!(true);
    }
}
