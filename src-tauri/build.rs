fn main() {
    let dir = std::path::Path::new("icons");
    let _ = std::fs::create_dir_all(dir);
    let png: &[u8] = &[
        137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,6,0,0,0,31,21,196,137,
        0,0,0,13,73,68,65,84,8,215,99,96,96,96,248,15,0,1,4,1,0,112,32,101,11,
        0,0,0,0,73,69,78,68,174,66,96,130
    ];
    let _ = std::fs::write(dir.join("icon.png"), png);
    #[cfg(target_os = "windows")]
    {
        // Minimal valid ICO containing a 1x1 32-bit image. Required by tauri-build
        // for the Windows resource even for the portable V0.1 build.
        let ico: &[u8] = &[
            0,0,1,0,1,0,1,1,0,0,1,0,32,0,48,0,0,0,22,0,0,0,
            40,0,0,0,1,0,0,0,2,0,0,0,1,0,32,0,0,0,0,0,4,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
            0,120,220,255,0,0,0,0
        ];
        let _ = std::fs::write(dir.join("icon.ico"), ico);
    }
    tauri_build::build()
}
