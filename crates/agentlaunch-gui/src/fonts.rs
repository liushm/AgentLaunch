use std::fs;
use std::path::Path;

use egui::{FontData, FontDefinitions, FontFamily};

/// Loads an OS-native CJK font (Windows Microsoft YaHei / macOS PingFang) so Chinese text renders
/// cleanly without bundling multi-megabyte font files into the binary.
pub fn configure_system_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\simhei.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        ]
    };

    for path_str in candidates {
        let path = Path::new(path_str);
        if let Ok(bytes) = fs::read(path) {
            let font_name = "system_cjk".to_string();
            fonts
                .font_data
                .insert(font_name.clone(), FontData::from_owned(bytes).into());

            if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
                family.push(font_name.clone());
            }
            if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
                family.push(font_name);
            }
            break;
        }
    }

    ctx.set_fonts(fonts);
}
