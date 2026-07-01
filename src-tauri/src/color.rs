//! Phase 3 — the signature: sample a vivid accent color from the album art.
//!
//! Decoding happens in Rust (via `image`) so we sidestep the webview's canvas
//! CORS taint on Spotify's image CDN, and so the corporate proxy is honored.
//! The result is one hex color the frontend drops into `--accent`; a blurred
//! copy of the art blooms behind the card and the whole player glows that color.

use image::GenericImageView;

const LILAC_FALLBACK: &str = "#c8b6ff";

/// Fetch the cover and return a vivid `#rrggbb` accent. Falls back to lilac for
/// missing or near-greyscale art so the player never looks broken.
#[tauri::command]
pub async fn get_accent(art_url: String) -> Result<String, String> {
    if art_url.is_empty() {
        return Ok(LILAC_FALLBACK.to_string());
    }
    let bytes = crate::http::client()
        .get(&art_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    // Image decode is CPU-bound — keep it off the async runtime threads.
    tokio::task::spawn_blocking(move || accent_from_bytes(&bytes))
        .await
        .map_err(|e| e.to_string())?
}

fn accent_from_bytes(bytes: &[u8]) -> Result<String, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    // Downscale — we only need the gist of the palette, not every pixel.
    let small = img.resize(64, 64, image::imageops::FilterType::Triangle);

    // Histogram of hue (36 bins), weighted toward vivid mid-luminance pixels.
    let mut weight = [0f64; 36];
    let mut sat_sum = [0f64; 36];
    let mut lum_sum = [0f64; 36];
    let mut count = [0u32; 36];
    let mut any = false;

    for (_, _, px) in small.pixels() {
        let [r, g, b, a] = px.0;
        if a < 128 {
            continue;
        }
        let (h, s, l) = rgb_to_hsl(r, g, b);
        // Skip greys and near black/white — they aren't "the light".
        if s < 0.18 || l < 0.12 || l > 0.92 {
            continue;
        }
        any = true;
        let bin = ((h / 10.0) as usize).min(35);
        // Favor saturated pixels near mid-luminance.
        weight[bin] += s * (1.0 - (l - 0.5).abs());
        sat_sum[bin] += s;
        lum_sum[bin] += l;
        count[bin] += 1;
    }

    if !any {
        return Ok(LILAC_FALLBACK.to_string());
    }

    let bin = (0..36)
        .max_by(|&a, &b| weight[a].partial_cmp(&weight[b]).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap();
    let n = count[bin].max(1) as f64;
    let hue = bin as f64 * 10.0 + 5.0;
    // Boost saturation and clamp luminance into a glowy band (the adaptive
    // guardrail: muted covers still produce a bright, legible accent).
    let sat = (sat_sum[bin] / n).clamp(0.55, 0.95);
    let lum = (lum_sum[bin] / n).clamp(0.52, 0.68);

    let (r, g, b) = hsl_to_rgb(hue, sat, lum);
    Ok(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d.abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    (h.rem_euclid(360.0), s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}
