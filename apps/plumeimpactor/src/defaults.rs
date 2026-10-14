use std::{
    env, fs,
    path::{Path, PathBuf},
};

use iced::window;

use crate::appearance;

pub(crate) fn default_settings() -> iced::Settings {
    iced::Settings {
        default_font: appearance::p_font(),
        default_text_size: appearance::THEME_FONT_SIZE.into(),
        fonts: appearance::load_fonts(),
        ..Default::default()
    }
}

pub(crate) fn default_window_settings() -> window::Settings {
    #[cfg(target_os = "macos")]
    let platform_specific = window::settings::PlatformSpecific {
        titlebar_transparent: true,
        title_hidden: true,
        fullsize_content_view: true,
        ..Default::default()
    };

    #[cfg(target_os = "linux")]
    let platform_specific = window::settings::PlatformSpecific {
        application_id: String::from("dev.khcrysalis.PlumeImpactor"),
        ..Default::default()
    };

    #[cfg(target_os = "windows")]
    let platform_specific = window::settings::PlatformSpecific::default();

    window::Settings {
        // Sized to fit the installer screen (the tallest) without scrolling.
        size: iced::Size::new(575.0, 475.0),
        position: window::Position::Centered,
        exit_on_close_request: false,
        resizable: false,
        icon: Some(load_window_icon()),
        platform_specific: platform_specific,
        ..Default::default()
    }
}

fn load_window_icon() -> window::Icon {
    let bytes = include_bytes!(
        "../../../package/linux/icons/hicolor/64x64/apps/dev.khcrysalis.PlumeImpactor.png"
    );
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("Failed to load icon bytes")
        .to_rgba8();
    let (width, height) = image.dimensions();
    window::icon::from_rgba(image.into_raw(), width, height).unwrap()
}

pub fn get_data_path() -> PathBuf {
    let base = if cfg!(windows) {
        env::var("APPDATA").unwrap()
    } else {
        env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            env::var("HOME").unwrap() + "/.config"
        })
    };

    let dir = Path::new(&base).join("PlumeImpactor");

    fs::create_dir_all(&dir).ok();

    dir
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdiReset {
    Cleared,
    AlreadyClear,
}

/// Drop the saved anisette provision and keep the device id.
///
/// Remote anisette stores the blob as `adi_pb` in `state.plist`. A local
/// Android library stores it as `adi.pb` in the same directory. The keychain
/// identifier stays, so the next sign-in re-provisions this device.
pub(crate) fn reset_adi_pb() -> Result<AdiReset, String> {
    reset_adi_pb_in(&get_data_path())
}

pub(crate) fn reset_adi_pb_in(dir: &Path) -> Result<AdiReset, String> {
    let mut cleared = false;

    let state_path = dir.join("state.plist");
    if state_path.is_file() {
        let mut value: plist::Value = plist::from_file(&state_path).map_err(|e| e.to_string())?;
        let dict = value
            .as_dictionary_mut()
            .ok_or_else(|| "state.plist is not a dictionary".to_string())?;
        if dict.remove("adi_pb").is_some() {
            plist::to_file_xml(&state_path, &value).map_err(|e| e.to_string())?;
            cleared = true;
        }
    }

    let adi_file = dir.join("adi.pb");
    if adi_file.is_file() {
        fs::remove_file(&adi_file).map_err(|e| e.to_string())?;
        cleared = true;
    }

    // Written by the one-shot client-info migration. They are not the provision.
    for name in ["v3_client_info", "adi_client_info"] {
        let path = dir.join(name);
        if path.is_file() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }

    Ok(if cleared {
        AdiReset::Cleared
    } else {
        AdiReset::AlreadyClear
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_drops_the_provision_and_keeps_the_device_id() {
        let dir = env::temp_dir().join(format!("impactor-adi-reset-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut dict = plist::Dictionary::new();
        dict.insert(
            "keychain_identifier".into(),
            plist::Value::Data(vec![9; 16]),
        );
        dict.insert("adi_pb".into(), plist::Value::Data(vec![1, 2, 3, 4]));
        plist::Value::Dictionary(dict)
            .to_file_xml(dir.join("state.plist"))
            .unwrap();
        fs::write(dir.join("adi.pb"), b"local").unwrap();
        fs::write(dir.join("v3_client_info"), b"marker").unwrap();

        assert_eq!(reset_adi_pb_in(&dir).unwrap(), AdiReset::Cleared);

        let value: plist::Value = plist::from_file(dir.join("state.plist")).unwrap();
        let dict = value.as_dictionary().unwrap();
        assert!(dict.get("adi_pb").is_none());
        assert_eq!(
            dict.get("keychain_identifier").and_then(|v| v.as_data()),
            Some([9u8; 16].as_slice())
        );
        assert!(!dir.join("adi.pb").exists());
        assert!(!dir.join("v3_client_info").exists());

        assert_eq!(reset_adi_pb_in(&dir).unwrap(), AdiReset::AlreadyClear);
        let value: plist::Value = plist::from_file(dir.join("state.plist")).unwrap();
        assert_eq!(
            value
                .as_dictionary()
                .unwrap()
                .get("keychain_identifier")
                .and_then(|v| v.as_data()),
            Some([9u8; 16].as_slice())
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
