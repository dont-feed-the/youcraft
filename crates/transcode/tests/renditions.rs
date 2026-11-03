//! A generated video made into renditions: exact timestamps, aligned keyframes, a PSNR floor,
//! and playlists, thumbnails and a storyboard that describe what was written.

use std::path::{Path, PathBuf};

use filmcraft_media::FrameRequest;
use youcraft_api::records::Storyboard;
use youcraft_transcode::picture::{Rgba, psnr};
use youcraft_transcode::synthetic::{self, Synthetic};
use youcraft_transcode::{open, transcode};

const SPEC: Synthetic = Synthetic {
    width: 1280,
    height: 720,
    fps: (30_000, 1001),
    frames: 140,
    tone_hz: Some(440),
    hue: 120,
};

#[cfg(test)]
fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("youcraft-transcode-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn extinf(playlist: &str) -> Vec<&str> {
    playlist
        .lines()
        .filter(|l| l.starts_with("#EXTINF:"))
        .collect()
}

#[cfg(test)]
fn made(dir: &Path) -> youcraft_transcode::Outcome {
    let input = dir.join("upload.mp4");
    synthetic::write_mp4(&SPEC, &input).unwrap();
    let output = dir.join("media");
    std::fs::create_dir_all(&output).unwrap();
    let last = std::sync::atomic::AtomicU32::new(0);
    let outcome = transcode(&input, &output, &|done| {
        last.fetch_max(done, std::sync::atomic::Ordering::Relaxed);
    })
    .unwrap();
    assert_eq!(last.into_inner(), 1000);
    outcome
}

#[test]
fn renditions_cut_at_the_same_exact_ticks() {
    let dir = scratch("ticks");
    let outcome = made(&dir);
    let media = dir.join("media");
    // 29.97 frames per second: 90 000 ticks per second, 3003 per frame, 60 frames per segment
    assert_eq!(outcome.duration.timescale, 90_000);
    assert_eq!(outcome.duration.ticks, 140 * 3003);
    let labels: Vec<u32> = outcome.renditions.iter().map(|r| r.label).collect();
    assert_eq!(labels, [360, 720]);
    let mut lists = Vec::new();
    for rendition in &outcome.renditions {
        let bytes = std::fs::read(media.join(format!("{}p.mp4", rendition.label))).unwrap();
        let file = filmcraft_isobmff::open(bytes.as_slice()).unwrap();
        assert!(file.fragmented);
        let track = &file.tracks[0];
        assert_eq!(track.timescale, 90_000);
        assert_eq!(track.samples.len(), 140);
        for (k, sample) in track.samples.iter().enumerate() {
            assert_eq!(
                sample.dts,
                k as i64 * 3003,
                "frame {k} of {}p",
                rendition.label
            );
            assert_eq!(sample.pts, sample.dts);
            assert_eq!(
                sample.is_sync,
                k % 60 == 0,
                "frame {k} of {}p",
                rendition.label
            );
        }
        let playlist =
            std::fs::read_to_string(media.join(format!("{}p.m3u8", rendition.label))).unwrap();
        assert_eq!(
            extinf(&playlist),
            ["#EXTINF:2.002,", "#EXTINF:2.002,", "#EXTINF:0.667333,"]
        );
        lists.push(playlist.replace(&format!("{}p.mp4", rendition.label), "file"));
        assert!(
            rendition.codecs.starts_with("avc1.64") && rendition.codecs.ends_with(",mp4a.40.2")
        );
        assert!(rendition.bandwidth > 100_000);
    }
    let master = std::fs::read_to_string(media.join("master.m3u8")).unwrap();
    assert!(master.contains("RESOLUTION=640x360,FRAME-RATE=29.970,"));
    assert!(master.contains("RESOLUTION=1280x720,"));
    assert!(master.find("360p.m3u8").unwrap() < master.find("720p.m3u8").unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_frame_survives_with_its_own_number_and_its_sound() {
    let dir = scratch("frames");
    let outcome = made(&dir);
    let media = dir.join("media");
    let source = open(&media.join("360p.mp4")).unwrap();
    let mut worst = f64::INFINITY;
    for k in 0..140u32 {
        let time = filmcraft_time::Tick::from_rational(2 * i64::from(k) + 1, 1001, 60_000);
        let frame = source.video_frame(FrameRequest::full(time)).unwrap();
        let decoded = Rgba {
            width: frame.width,
            height: frame.height,
            pixels: frame.to_rgba8(),
        };
        let luma = decoded.to_yuv420().y;
        assert_eq!(
            synthetic::read_code(&luma, 640, 360),
            k,
            "frame {k} is in its place"
        );
        let expected = synthetic::frame(&SPEC, k).resized(640, 360).to_yuv420().y;
        worst = worst.min(psnr(&expected, &luma));
    }
    assert!(worst > 30.0, "luma PSNR {worst:.1} dB");

    let audio = std::fs::read(media.join("audio.mp4")).unwrap();
    let file = filmcraft_isobmff::open(audio.as_slice()).unwrap();
    let track = &file.tracks[0];
    assert_eq!(track.timescale, 48_000);
    assert_eq!(track.edits.first().map(|e| e.media_time), Some(1024));
    let samples: u64 = 140 * 1001 * 48_000 / 30_000;
    let ticks: u64 = track.samples.iter().map(|s| u64::from(s.duration)).sum();
    assert_eq!(ticks, samples.div_ceil(1024) * 1024 + 1024);
    assert_eq!(outcome.renditions.len(), 2);

    for width in youcraft_api::routes::THUMBNAIL_WIDTHS {
        let jpeg = std::fs::read(media.join(format!("thumbnail/{width}.jpg"))).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    }
    let storyboard: Storyboard =
        serde_json::from_slice(&std::fs::read(media.join("storyboard.json")).unwrap()).unwrap();
    // a tile every 30 frames of 3003 ticks: frames 0, 30, 60, 90 and 120
    assert_eq!(storyboard.interval.ticks, 30 * 3003);
    assert_eq!(storyboard.sheets, 1);
    assert!(media.join("storyboard/0.jpg").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_is_no_video_fails_as_unsupported() {
    let dir = scratch("garbage");
    let input = dir.join("upload.mp4");
    std::fs::write(&input, b"\x00\x00\x00\x18ftypisom but nothing after it").unwrap();
    let error = transcode(&input, &dir, &|_| {}).unwrap_err();
    assert_eq!(error.kind(), "unsupported", "{error}");
    std::fs::write(&input, [0x47u8; 4096]).unwrap();
    assert!(transcode(&input, &dir, &|_| {}).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
