//! Register `.housecraft` files to open in HouseCraft, for the current user.
//!
//! - Windows: per-user ProgID under `HKCU\Software\Classes` (no admin rights).
//! - Linux: shared-mime-info package + `.desktop` entry under `~/.local/share`.
//! - macOS: file types are declared in the app bundle's `Info.plist` at packaging
//!   time (see `[package.metadata.packager]`); nothing to do at runtime.

use std::path::Path;

pub const EXTENSION: &str = housecraft_io_native::EXTENSION;
pub const LEGACY_EXTENSION: &str = housecraft_io_native::LEGACY_EXTENSION;
pub const PROG_ID: &str = "HouseCraft.Plan";
pub const MIME_TYPE: &str = housecraft_io_native::MIME_TYPE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// `.housecraft` opens with this executable.
    Registered,
    /// Registered, but to a different HouseCraft executable (e.g. an older build).
    OtherExecutable,
    NotRegistered,
    /// Handled by the app bundle on this platform (macOS, web).
    #[cfg_attr(
        not(any(target_os = "macos", target_arch = "wasm32")),
        allow(dead_code)
    )]
    ByPackage,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    const CLASSES: &str = r"Software\Classes";

    fn command(exe: &Path) -> String {
        format!("\"{}\" \"%1\"", exe.display())
    }

    fn notify_shell() {
        // Tell Explorer that associations changed so icons and verbs refresh.
        unsafe {
            windows_sys::Win32::UI::Shell::SHChangeNotify(
                windows_sys::Win32::UI::Shell::SHCNE_ASSOCCHANGED as i32,
                windows_sys::Win32::UI::Shell::SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }

    pub fn register_under(base: &str, exe: &Path) -> std::io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (prog, _) = hkcu.create_subkey(format!(r"{base}\{PROG_ID}"))?;
        prog.set_value("", &"HouseCraft Plan")?;
        let (icon, _) = prog.create_subkey("DefaultIcon")?;
        icon.set_value("", &format!("\"{}\",0", exe.display()))?;
        let (cmd, _) = prog.create_subkey(r"shell\open\command")?;
        cmd.set_value("", &command(exe))?;
        for ext in [EXTENSION, LEGACY_EXTENSION] {
            let (key, _) = hkcu.create_subkey(format!(r"{base}\.{ext}"))?;
            key.set_value("", &PROG_ID)?;
            key.set_value("Content Type", &MIME_TYPE)?;
            let (open_with, _) = key.create_subkey("OpenWithProgids")?;
            open_with.set_value(PROG_ID, &"")?;
        }
        Ok(())
    }

    pub fn unregister_under(base: &str) -> std::io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(format!(r"{base}\{PROG_ID}"));
        for ext in [EXTENSION, LEGACY_EXTENSION] {
            if let Ok(key) = hkcu.open_subkey_with_flags(
                format!(r"{base}\.{ext}"),
                KEY_READ | winreg::enums::KEY_WRITE,
            ) {
                let current: String = key.get_value("").unwrap_or_default();
                if current == PROG_ID {
                    let _ = hkcu.delete_subkey_all(format!(r"{base}\.{ext}"));
                }
            }
        }
        Ok(())
    }

    pub fn status_under(base: &str, exe: &Path) -> Status {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let ext_ok = hkcu
            .open_subkey(format!(r"{base}\.{EXTENSION}"))
            .and_then(|k| k.get_value::<String, _>(""))
            .is_ok_and(|v| v == PROG_ID);
        let cmd = hkcu
            .open_subkey(format!(r"{base}\{PROG_ID}\shell\open\command"))
            .and_then(|k| k.get_value::<String, _>(""));
        match (ext_ok, cmd) {
            (true, Ok(c)) if c.eq_ignore_ascii_case(&command(exe)) => Status::Registered,
            (true, Ok(_)) => Status::OtherExecutable,
            _ => Status::NotRegistered,
        }
    }

    pub fn register(exe: &Path) -> std::io::Result<()> {
        register_under(CLASSES, exe)?;
        notify_shell();
        Ok(())
    }

    pub fn unregister() -> std::io::Result<()> {
        unregister_under(CLASSES)?;
        notify_shell();
        Ok(())
    }

    pub fn status(exe: &Path) -> Status {
        status_under(CLASSES, exe)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;
    use std::path::PathBuf;

    fn data_home() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }

    fn files() -> Option<(PathBuf, PathBuf)> {
        let base = data_home()?;
        Some((
            base.join("mime/packages/housecraft.xml"),
            base.join("applications/housecraft.desktop"),
        ))
    }

    fn run(cmd: &str, args: &[&str]) {
        let _ = std::process::Command::new(cmd).args(args).status();
    }

    pub fn register(exe: &Path) -> std::io::Result<()> {
        let (mime, desktop) = files().ok_or_else(|| std::io::Error::other("no HOME"))?;
        std::fs::create_dir_all(mime.parent().expect("has parent"))?;
        std::fs::create_dir_all(desktop.parent().expect("has parent"))?;
        std::fs::write(
            &mime,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="{MIME_TYPE}">
    <comment>HouseCraft plan</comment>
    <glob pattern="*.{EXTENSION}"/>
    <glob pattern="*.{LEGACY_EXTENSION}"/>
  </mime-type>
</mime-info>
"#
            ),
        )?;
        std::fs::write(
            &desktop,
            format!(
                "[Desktop Entry]\nType=Application\nName=HouseCraft\nComment=Residential design\nExec=\"{}\" %f\nMimeType={MIME_TYPE};\nCategories=Graphics;Engineering;\nTerminal=false\n",
                exe.display()
            ),
        )?;
        let base = data_home().expect("checked");
        run(
            "update-mime-database",
            &[&base.join("mime").to_string_lossy()],
        );
        run(
            "update-desktop-database",
            &[&base.join("applications").to_string_lossy()],
        );
        run("xdg-mime", &["default", "housecraft.desktop", MIME_TYPE]);
        Ok(())
    }

    pub fn unregister() -> std::io::Result<()> {
        if let Some((mime, desktop)) = files() {
            let _ = std::fs::remove_file(mime);
            let _ = std::fs::remove_file(desktop);
            let base = data_home().expect("checked");
            run(
                "update-mime-database",
                &[&base.join("mime").to_string_lossy()],
            );
            run(
                "update-desktop-database",
                &[&base.join("applications").to_string_lossy()],
            );
        }
        Ok(())
    }

    pub fn status(exe: &Path) -> Status {
        match files().and_then(|(_, d)| std::fs::read_to_string(d).ok()) {
            Some(text) if text.contains(&exe.display().to_string()) => Status::Registered,
            Some(_) => Status::OtherExecutable,
            None => Status::NotRegistered,
        }
    }
}

#[cfg(any(target_os = "macos", target_arch = "wasm32"))]
mod imp {
    use super::*;

    pub fn register(_exe: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "on this platform file types are registered by the installed app bundle",
        ))
    }
    pub fn unregister() -> std::io::Result<()> {
        Ok(())
    }
    pub fn status(_exe: &Path) -> Status {
        Status::ByPackage
    }
}

/// The running executable.
pub fn current_exe() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()
}

pub fn register() -> std::io::Result<()> {
    let exe = current_exe()
        .ok_or_else(|| std::io::Error::other("cannot locate the HouseCraft executable"))?;
    imp::register(&exe)
}

pub fn unregister() -> std::io::Result<()> {
    imp::unregister()
}

pub fn status() -> Status {
    current_exe().map_or(Status::NotRegistered, |exe| imp::status(&exe))
}

#[cfg(all(test, windows))]
mod tests {
    use super::imp::*;
    use super::*;

    #[test]
    fn registers_and_unregisters_under_a_test_key() {
        // A scratch key, so the test never touches the real file associations.
        let base = format!(r"Software\HouseCraftTest-{}\Classes", std::process::id());
        let exe = Path::new(r"C:\Program Files\HouseCraft\housecraft-app.exe");
        assert_eq!(status_under(&base, exe), Status::NotRegistered);
        register_under(&base, exe).unwrap();
        assert_eq!(status_under(&base, exe), Status::Registered);
        assert_eq!(
            status_under(&base, Path::new(r"C:\other.exe")),
            Status::OtherExecutable
        );
        unregister_under(&base).unwrap();
        assert_eq!(status_under(&base, exe), Status::NotRegistered);
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let root = base.trim_end_matches(r"\Classes");
        hkcu.delete_subkey_all(root).unwrap();
    }
}
