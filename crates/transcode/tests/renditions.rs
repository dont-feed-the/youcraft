//! Generated videos made into renditions: exact timestamps, aligned keyframes, a PSNR floor,
//! playlists, thumbnails and a storyboard that describe what was written, and sound mixed down
//! to stereo with every speaker where it belongs.

use std::path::{Path, PathBuf};

use filmcraft_media::FrameRequest;
use youcraft_api::records::{Failure, Storyboard};
use youcraft_transcode::picture::{Rgba, psnr};
use youcraft_transcode::synthetic::{self, Synthetic, Tone};
use youcraft_transcode::{Outcome, open, plan, transcode};

const SPEC: Synthetic = Synthetic {
    width: 1280,
    height: 720,
    par: (1, 1),
    fps: (30_000, 1001),
    frames: 140,
    sound: Some(Tone::stereo(440)),
    hue: 120,
};

/// A small, short video: quick to make, and enough to check sizes, rates and sound.
const SMALL: Synthetic = Synthetic {
    width: 320,
    height: 180,
    par: (1, 1),
    fps: (30, 1),
    frames: 60,
    sound: None,
    hue: 40,
};

// Helpers that unwrap are `#[cfg(test)]`, so that clippy's allowances for tests reach them.
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

/// `spec` written to `dir` and made into renditions in `dir/media`.
#[cfg(test)]
fn made(dir: &Path, spec: &Synthetic) -> Outcome {
    let input = dir.join("upload.mp4");
    synthetic::write_mp4(spec, &input).unwrap();
    let output = dir.join("media");
    std::fs::create_dir_all(&output).unwrap();
    let source = open(&input).unwrap();
    let plan = plan(&source).unwrap();
    let last = std::sync::atomic::AtomicU32::new(0);
    let outcome = transcode(&source, &plan, &output, &|done| {
        last.fetch_max(done, std::sync::atomic::Ordering::Relaxed);
    })
    .unwrap();
    assert_eq!(last.into_inner(), 1000);
    outcome
}

/// The video frames of a rendition's track file: decode times, and whether each is a keyframe.
#[cfg(test)]
fn frame_times(file: &Path) -> Vec<(i64, bool)> {
    let bytes = std::fs::read(file).unwrap();
    let file = filmcraft_isobmff::open(bytes.as_slice()).unwrap();
    assert!(file.fragmented);
    let track = &file.tracks[0];
    track.samples.iter().map(|s| (s.dts, s.is_sync)).collect()
}

/// The amplitude of the sine at `hz` in `signal`, sampled at 48 kHz, over a whole number of its
/// periods: one second.
fn amplitude(signal: &[f32], hz: u32) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (n, &x) in signal.iter().enumerate() {
        let angle = std::f64::consts::TAU * f64::from(hz) * n as f64 / 48_000.0;
        re += f64::from(x) * angle.cos();
        im += f64::from(x) * angle.sin();
    }
    2.0 * (re * re + im * im).sqrt() / signal.len() as f64
}

/// The middle second of the rendition's sound, left and right.
#[cfg(test)]
fn middle_second(media: &Path) -> (Vec<f32>, Vec<f32>) {
    let source = open(&media.join("audio.mp4")).unwrap();
    let buffer = source.audio(24_000, 48_000, 48_000).unwrap();
    assert_eq!(buffer.channels.len(), 2);
    (buffer.channels[0].clone(), buffer.channels[1].clone())
}

#[test]
fn renditions_cut_at_the_same_exact_ticks() {
    let dir = scratch("ticks");
    let outcome = made(&dir, &SPEC);
    let media = dir.join("media");
    // 29.97 frames per second: 90 000 ticks per second, 3003 per frame, 60 frames per segment
    assert_eq!(outcome.duration.timescale.get(), 90_000);
    assert_eq!(outcome.duration.ticks, 140 * 3003);
    let labels: Vec<u32> = outcome.renditions.iter().map(|r| r.label).collect();
    assert_eq!(labels, [360, 720]);
    let mut lists = Vec::new();
    for rendition in &outcome.renditions {
        let frames = frame_times(&media.join(format!("{}p.mp4", rendition.label)));
        assert_eq!(frames.len(), 140);
        for (k, &(dts, sync)) in frames.iter().enumerate() {
            assert_eq!(dts, k as i64 * 3003, "frame {k} of {}p", rendition.label);
            assert_eq!(sync, k % 60 == 0, "frame {k} of {}p", rendition.label);
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
        assert!(rendition.bandwidth >= rendition.average_bandwidth);
    }
    let master = std::fs::read_to_string(media.join("master.m3u8")).unwrap();
    assert!(master.contains("RESOLUTION=640x360,FRAME-RATE=29.970,"));
    assert!(master.contains("RESOLUTION=1280x720,"));
    assert!(master.find("360p.m3u8").unwrap() < master.find("720p.m3u8").unwrap());
    // what the database keeps is what the master playlist says
    for rendition in &outcome.renditions {
        assert!(master.contains(&format!(
            "BANDWIDTH={},AVERAGE-BANDWIDTH={},RESOLUTION={}x{}",
            rendition.bandwidth, rendition.average_bandwidth, rendition.width, rendition.height
        )));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_frame_survives_with_its_own_number_and_its_sound() {
    let dir = scratch("frames");
    let outcome = made(&dir, &SPEC);
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
    // the video's duration of sound, rounded up to whole AAC frames, after the priming frame
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
    let failure = open(&input).and_then(|source| plan(&source)).err().unwrap();
    assert_eq!(failure.failure(), Failure::Unsupported, "{failure}");
    std::fs::write(&input, [0x47u8; 4096]).unwrap();
    assert!(open(&input).and_then(|source| plan(&source)).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_portrait_video_is_named_by_its_shorter_side() {
    let dir = scratch("portrait");
    let spec = Synthetic {
        width: 720,
        height: 1280,
        frames: 12,
        ..SMALL
    };
    let outcome = made(&dir, &spec);
    let sizes: Vec<_> = outcome
        .renditions
        .iter()
        .map(|r| (r.label, r.width, r.height))
        .collect();
    assert_eq!(sizes, [(360, 360, 640), (720, 720, 1280)]);
    assert_eq!((outcome.width, outcome.height), (720, 1280));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn anamorphic_pixels_are_made_square() {
    let dir = scratch("anamorphic");
    // 640×360 stored, every pixel twice as wide as tall: shown at 1280×360
    let spec = Synthetic {
        width: 640,
        height: 360,
        par: (2, 1),
        frames: 12,
        ..SMALL
    };
    let outcome = made(&dir, &spec);
    assert_eq!((outcome.width, outcome.height), (1280, 360));
    let sizes: Vec<_> = outcome
        .renditions
        .iter()
        .map(|r| (r.label, r.width, r.height))
        .collect();
    assert_eq!(sizes, [(360, 1280, 360)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fast_video_keeps_every_other_frame() {
    let dir = scratch("fast");
    // 120 frames per second for one second: 60 kept, frame k showing source frame 2k
    let spec = Synthetic {
        fps: (120, 1),
        frames: 120,
        ..SMALL
    };
    let outcome = made(&dir, &spec);
    let media = dir.join("media");
    assert_eq!(outcome.duration.timescale.get(), 90_000);
    assert_eq!(outcome.duration.ticks, 60 * 1500);
    let frames = frame_times(&media.join("180p.mp4"));
    assert_eq!(frames.len(), 60);
    assert!(
        frames
            .iter()
            .enumerate()
            .all(|(k, &(dts, _))| dts == k as i64 * 1500)
    );
    let source = open(&media.join("180p.mp4")).unwrap();
    for k in [0u32, 1, 29, 59] {
        let time = filmcraft_time::Tick::from_rational(2 * i64::from(k) + 1, 1, 120);
        let frame = source.video_frame(FrameRequest::full(time)).unwrap();
        let luma = Rgba {
            width: frame.width,
            height: frame.height,
            pixels: frame.to_rgba8(),
        }
        .to_yuv420()
        .y;
        assert_eq!(synthetic::read_code(&luma, 320, 180), 2 * k);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_video_shorter_than_a_segment_still_has_its_stills() {
    let dir = scratch("short");
    let spec = Synthetic { frames: 7, ..SMALL };
    let outcome = made(&dir, &spec);
    let media = dir.join("media");
    assert_eq!(outcome.duration.ticks, 7 * 3000);
    let playlist = std::fs::read_to_string(media.join("180p.m3u8")).unwrap();
    assert_eq!(extinf(&playlist), ["#EXTINF:0.233333,"]);
    assert!(playlist.contains("#EXT-X-TARGETDURATION:1\n"));
    assert!(media.join("thumbnail/168.jpg").exists());
    assert!(media.join("storyboard/0.jpg").exists());
    // a silent video has no sound to list
    let master = std::fs::read_to_string(media.join("master.m3u8")).unwrap();
    assert!(!master.contains("EXT-X-MEDIA") && !master.contains("mp4a"));
    assert!(!media.join("audio.mp4").exists());
    assert!(
        outcome.renditions[0].codecs.starts_with("avc1.")
            && !outcome.renditions[0].codecs.contains(',')
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mono_sound_plays_on_both_sides() {
    let dir = scratch("mono");
    let spec = Synthetic {
        sound: Some(Tone {
            hz: 440,
            channels: 1,
        }),
        ..SMALL
    };
    made(&dir, &spec);
    let (left, right) = middle_second(&dir.join("media"));
    for side in [&left, &right] {
        let level = amplitude(side, 440);
        assert!((level - 0.25).abs() < 0.03, "{level}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn five_one_sound_keeps_its_dialogue() {
    let dir = scratch("surround");
    // L 440, R 880, C 1320, LFE 1760, Ls 2200, Rs 2640 Hz, each at 0.25
    let tone = Tone {
        hz: 440,
        channels: 6,
    };
    let spec = Synthetic {
        sound: Some(tone),
        ..SMALL
    };
    made(&dir, &spec);
    let (left, right) = middle_second(&dir.join("media"));
    let side = std::f64::consts::FRAC_1_SQRT_2;
    let (front, other) = (0.25 / (1.0 + 2.0 * side), 0.25 * side / (1.0 + 2.0 * side));
    let near = |level: f64, expected: f64| (level - expected).abs() < 0.15 * expected + 0.004;
    let [l, r, c, lfe, ls, rs] = [0, 1, 2, 3, 4, 5].map(|channel| tone.hz_of(channel));
    assert!(
        near(amplitude(&left, l), front),
        "L {}",
        amplitude(&left, l)
    );
    assert!(
        near(amplitude(&right, r), front),
        "R {}",
        amplitude(&right, r)
    );
    for (name, signal) in [("left", &left), ("right", &right)] {
        assert!(
            near(amplitude(signal, c), other),
            "C on the {name}: {}",
            amplitude(signal, c)
        );
        assert!(amplitude(signal, lfe) < 0.01, "LFE on the {name}");
    }
    assert!(
        near(amplitude(&left, ls), other) && amplitude(&right, ls) < 0.01,
        "Ls"
    );
    assert!(
        near(amplitude(&right, rs), other) && amplitude(&left, rs) < 0.01,
        "Rs"
    );
    assert!(
        amplitude(&left, r) < 0.01 && amplitude(&right, l) < 0.01,
        "the fronts stay apart"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
