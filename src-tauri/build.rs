fn main() {
    // Tauri's compile-time context expects an icon even when bundling is disabled.
    // Generate a tiny valid PNG for CI/dev; branded release assets come later.
    let dir = std::path::Path::new("icons");
    let _ = std::fs::create_dir_all(dir);
    let png: &[u8] = &[
        137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,6,0,0,0,31,21,196,137,
        0,0,0,13,73,68,65,84,8,215,99,96,96,96,248,15,0,1,4,1,0,112,32,101,11,
        0,0,0,0,73,69,78,68,174,66,96,130
    ];
    let _ = std::fs::write(dir.join("icon.png"), png);
    tauri_build::build()
}
