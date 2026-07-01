fn main() {
    // Config (Client ID / proxy) is loaded at *runtime* from lume.env — see
    // src/config.rs. Nothing is baked into the binary.
    tauri_build::build()
}
