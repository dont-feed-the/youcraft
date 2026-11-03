//! Example channels and videos for an empty database, written in code, for local development.

use youcraft_api::time::MediaDuration;
use youcraft_api::{AccountId, ChannelId, Role, VideoId};
use youcraft_store::accounts::NewAccount;
use youcraft_store::videos::{NewVideo, Visibility};
use youcraft_store::{Store, StoreError};

use crate::site::now;

/// Three channels and their videos, published over the last months; refuses a database that
/// already has the first demo account.
pub fn fill(store: &Store) -> Result<(), StoreError> {
    let channels = [
        ("Ondes & Lumière", "ondes"),
        ("Atelier Bois", "atelierbois"),
        ("Pixel Garden", "pixelgarden"),
    ];
    let topics = [
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
    let now = now();
    for (index, (name, handle)) in channels.iter().enumerate() {
        let n = index as u64 + 1;
        store.create_account(&NewAccount {
            id: AccountId::from_bits(0xdead_0000 + n),
            channel: ChannelId::from_bits(0xc4a0_0000 + n),
            google_subject: &format!("demo-{n}"),
            email: &format!("demo{n}@example.invalid"),
            name,
            handle,
            role: Role::Creator,
            now,
        })?;
    }
    for (index, title) in topics.iter().enumerate() {
        let n = index as u64;
        let channel = ChannelId::from_bits(0xc4a0_0001 + n % 3);
        store.insert_ready_video(&NewVideo {
            id: VideoId::from_bits(0x5eed_0000_0000 + n * 7919),
            channel,
            title,
            description: "An example video, generated for local development.",
            visibility: Visibility::Public,
            duration: MediaDuration::from_millis(45_000 + n * 97_000),
            width: 1920,
            height: 1080,
            published_at: now - (n as i64 + 1) * (n as i64 + 1) * 86_400 / 2,
        })?;
    }
    Ok(())
}
