fn main() {
    // Load the Client ID from `.env` (repo root, or src-tauri/) and expose it to
    // the crate as a compile-time env var, so `option_env!("LUME_SPOTIFY_CLIENT_ID")`
    // in auth.rs picks it up. Not a secret, but per-user (each dev's own Spotify app).
    let _ = dotenvy::from_filename("../.env").or_else(|_| dotenvy::from_filename(".env"));
    if let Ok(id) = std::env::var("LUME_SPOTIFY_CLIENT_ID") {
        println!("cargo:rustc-env=LUME_SPOTIFY_CLIENT_ID={id}");
    }
    // Rebuild when the .env or the var changes.
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-env-changed=LUME_SPOTIFY_CLIENT_ID");

    tauri_build::build()
}
