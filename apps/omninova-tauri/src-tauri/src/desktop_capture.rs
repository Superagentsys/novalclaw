use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use omninova_core::computer_use::{DesktopDriver, OsDesktopDriver};
use serde::Serialize;
use std::io::Cursor;
use std::path::PathBuf;

const DEFAULT_MAX_DIMENSION_PX: u32 = 1280;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopScreenshotPayload {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

fn resize_max_dimension(image: DynamicImage, max_dimension_px: u32) -> DynamicImage {
    let max_dimension_px = max_dimension_px.max(320);
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_dimension_px {
        return image;
    }
    let scale = max_dimension_px as f32 / longest as f32;
    let target_w = ((width as f32) * scale).round().max(1.0) as u32;
    let target_h = ((height as f32) * scale).round().max(1.0) as u32;
    image.resize(target_w, target_h, FilterType::Triangle)
}

fn encode_jpeg_data_url(image: DynamicImage) -> Result<String, String> {
    // JPEG 不支持 alpha 通道；屏幕截图常为 RGBA8，需先丢弃 alpha 转 RGB8。
    let rgb = DynamicImage::ImageRgb8(image.to_rgb8());
    let mut buffer = Vec::new();
    rgb
        .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Jpeg)
        .map_err(|e| format!("JPEG 编码失败: {e}"))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(buffer)
    ))
}

fn load_png_bytes(bytes: &[u8]) -> Result<DynamicImage, String> {
    image::load_from_memory(bytes).map_err(|e| format!("解析截图失败: {e}"))
}

/// Screen capture is delegated to the same driver `computer_use` uses, so the
/// desktop-vision preview and the agent's own screenshots cannot diverge per
/// platform. That driver tries the cross-platform capture first and falls back
/// to OS tooling, which is what gives this command Windows support.
async fn capture_screen_png_bytes() -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(|| {
        let path: PathBuf = std::env::temp_dir().join(format!(
            "omninova-screen-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        let captured = OsDesktopDriver
            .capture_png(&path)
            .and_then(|_| std::fs::read(&path).map_err(|e| format!("读取截图文件失败: {e}")));
        let _ = std::fs::remove_file(&path);
        let bytes = captured?;
        if bytes.is_empty() {
            return Err("截图为空，请检查屏幕录制权限".to_string());
        }
        Ok(bytes)
    })
    .await
    .map_err(|e| format!("截图任务未能完成: {e}"))?
}

/// 截取主显示器画面，缩放后返回 JPEG data URL。
#[tauri::command]
pub async fn capture_desktop_screenshot(
    max_dimension_px: Option<u32>,
) -> Result<DesktopScreenshotPayload, String> {
    let max_dimension_px = max_dimension_px.unwrap_or(DEFAULT_MAX_DIMENSION_PX);
    let png_bytes = capture_screen_png_bytes().await?;
    let image = load_png_bytes(&png_bytes)?;
    let resized = resize_max_dimension(image, max_dimension_px);
    let (width, height) = resized.dimensions();
    let data_url = encode_jpeg_data_url(resized)?;

    Ok(DesktopScreenshotPayload {
        data_url,
        width,
        height,
    })
}
