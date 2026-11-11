//! Embeds the Windows version information (Properties > Details) and application manifest in
//! firecraft.exe. Executables without them look suspicious to some antivirus heuristics.
//! Compiled with windres (MinGW), rc.exe (MSVC on Windows) or llvm-rc (cargo-xwin).

use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=windows/firecraft.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let manifest = dir.join("windows").join("firecraft.manifest");
    let rc_path = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    let number = |key: &str| env::var(key).ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(0);
    let (major, minor, patch) = (
        number("CARGO_PKG_VERSION_MAJOR"),
        number("CARGO_PKG_VERSION_MINOR"),
        number("CARGO_PKG_VERSION_PATCH"),
    );
    let version = env::var("CARGO_PKG_VERSION")?;
    let description = env::var("CARGO_PKG_DESCRIPTION")?;
    // Numeric constants rather than winver.h, so the script needs no SDK headers:
    // VOS_NT_WINDOWS32 = 0x40004, VFT_APP = 1, RT_MANIFEST = 24.
    let rc = format!(
        r#"1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3F
FILEFLAGS 0x0
FILEOS 0x40004
FILETYPE 0x1
FILESUBTYPE 0x0
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "CompanyName", "The FireCraft authors"
            VALUE "FileDescription", "FireCraft"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "firecraft"
            VALUE "LegalCopyright", "Copyright (c) 2026 the FireCraft authors. MIT or Apache-2.0."
            VALUE "OriginalFilename", "firecraft.exe"
            VALUE "ProductName", "FireCraft"
            VALUE "ProductVersion", "{version}"
            VALUE "Comments", "{description}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END

1 24 "{manifest}"
"#,
        manifest = rc_path(&manifest),
        description = description.replace('"', "'"),
    );
    let out = PathBuf::from(env::var("OUT_DIR")?).join("firecraft.rc");
    fs::write(&out, rc)?;
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .map_err(|e| format!("embedding the Windows resources: {e}"))?;
    Ok(())
}
