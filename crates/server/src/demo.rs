//! Example channels and videos for an empty database, made in code, for local development.
//!
//! Each video is generated (a moving pattern that spells its frame numbers, and a tone), written
//! as an MP4 file as a camera would hand it over, uploaded as a creator would, and made playable
//! by the same job worker and child process as any upload; then it is dated as if published over
//! the last months.

use youcraft_api::records::Visibility;
use youcraft_api::{AccountId, ChannelId, Role, VideoId};
use youcraft_store::accounts::NewAccount;
use youcraft_store::uploads::NewUpload;
use youcraft_transcode::synthetic::{self, Synthetic};

use crate::site::{Site, now};

const CHANNELS: [(&str, &str); 3] = [
    ("Ondes & Lumière", "ondes"),
    ("Atelier Bois", "atelierbois"),
    ("Pixel Garden", "pixelgarden"),
];

const TOPICS: [&str; 12] = [
    "Why light seems to slow down in glass",
    "A dovetail joint by hand, start to finish",
    "Growing a pixel-art forest in one hour",
    "Standing waves on a guitar string, filmed at 2000 fps",
    "Sharpening a chisel: the only three steps you need",
    "Palette tricks for night scenes",
    "The ringing of a slab after the pulse has passed",
    "Building a workbench from one sheet of plywood",
    "Animating rain with eight frames",
    "Snell's law without equations",
    "Finishing oak with oil and wax",
    "A tiny town, tile by tile",
];

/// The frame rates the demo's videos come in, as cameras record them.
const RATES: [(u32, u32); 3] = [(30, 1), (25, 1), (30_000, 1001)];

/// Three channels and their videos; refuses a database that already has the first demo account.
pub async fn fill(site: &Site) -> Result<(), String> {
    let now = now();
    let store = site.store();
    for (index, (name, handle)) in CHANNELS.iter().enumerate() {
        let n = index as u64 + 1;
        store
            .create_account(&NewAccount {
                id: AccountId::from_bits(0xdead_0000 + n),
                channel: ChannelId::from_bits(0xc4a0_0000 + n),
                google_subject: &format!("demo-{n}"),
                email: &format!("demo{n}@example.invalid"),
                name,
                handle,
                role: Role::Creator,
                now,
            })
            .map_err(|e| format!("demo: {e}"))?;
    }
    let mut videos = Vec::new();
    for ((n, title), &fps) in (0u32..).zip(TOPICS).zip(RATES.iter().cycle()) {
        let video = VideoId::from_bits(0x5eed_0000_0000 + u64::from(n) * 7919);
        let spec = Synthetic {
            width: 1280,
            height: 720,
            fps,
            frames: (10 + 2 * n) * fps.0 / fps.1,
            tone_hz: Some(220 + 55 * n),
            hue: u8::try_from(n * 21 % 256).unwrap_or(0),
        };
        let path = site.config().originals().join(video.to_string());
        let written =
            tokio::task::spawn_blocking(move || synthetic::write_mp4(&spec, &path).map(|()| path))
                .await;
        let path = written
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("demo video: {e}"))?;
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        let upload = NewUpload {
            video,
            channel: ChannelId::from_bits(0xc4a0_0001 + u64::from(n % 3)),
            title,
            description: "An example video, generated for local development.",
            visibility: Visibility::Public,
            size,
            now,
        };
        let queued = store
            .create_upload(&upload)
            .and_then(|()| store.set_received(video, size))
            .and_then(|()| store.finish_upload(video, now));
        queued.map_err(|e| format!("demo: {e}"))?;
        videos.push(video);
        tracing::info!("demo: generated {} of {}", videos.len(), TOPICS.len());
    }
    crate::jobs::drain(site).await;
    for (n, video) in (1i64..).zip(videos) {
        store
            .set_published_at(video, now - n * n * 86_400 / 2)
            .map_err(|e| format!("demo: {e}"))?;
    }
    Ok(())
}
