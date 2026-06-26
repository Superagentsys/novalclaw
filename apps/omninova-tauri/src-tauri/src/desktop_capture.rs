use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
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

#[cfg(target_os = "macos")]
async fn capture_screen_png_bytes() -> Result<Vec<u8>, String> {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "omninova-screen-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    let path_str = path
        .to_str()
        .ok_or_else(|| "无法创建临时截图路径".to_string())?;

    let status = tokio::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-C", "-t", "png", path_str])
        .status()
        .await
        .map_err(|e| format!("无法调用 screencapture: {e}"))?;

    if !status.success() {
        return Err(
            "屏幕截取失败（macOS 需在 系统设置 → 隐私与安全性 → 屏幕录制 中授权 OmniNova Claw）"
                .into(),
        );
    }

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("读取截图文件失败: {e}"))?;
    let _ = tokio::fs::remove_file(&path).await;
    if bytes.is_empty() {
        return Err("截图为空，请检查屏幕录制权限".into());
    }
    Ok(bytes)
}

#[cfg(target_os = "linux")]
async fn capture_screen_png_bytes() -> Result<Vec<u8>, String> {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "omninova-screen-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    let path_str = path
        .to_str()
        .ok_or_else(|| "无法创建临时截图路径".to_string())?;

    let mut last_error = String::from("未找到可用的截图工具");

    for (bin, args) in [
        ("gnome-screenshot", vec!["-f", path_str]),
        ("import", vec!["-window", "root", path_str]),
        ("scrot", vec![path_str]),
    ] {
        let status = tokio::process::Command::new(bin)
            .args(&args)
            .status()
            .await;
        match status {
            Ok(s) if s.success() => {
                let bytes = tokio::fs::read(&path)
                    .await
                    .map_err(|e| format!("读取截图文件失败: {e}"))?;
                let _ = tokio::fs::remove_file(&path).await;
                if !bytes.is_empty() {
                    return Ok(bytes);
                }
                last_error = format!("{bin} 产出空文件");
            }
            Ok(_) => last_error = format!("{bin} 退出码非 0"),
            Err(e) => last_error = format!("无法调用 {bin}: {e}"),
        }
    }

    Err(last_error)
}

#[cfg(target_os = "windows")]
async fn capture_screen_png_bytes() -> Result<Vec<u8>, String> {
    let _ = ();
    Err("Windows 桌面截图暂未实现，请使用 macOS 或 Linux".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
async fn capture_screen_png_bytes() -> Result<Vec<u8>, String> {
    Err("当前平台不支持桌面截图".into())
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
