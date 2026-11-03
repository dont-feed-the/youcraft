//! One CMAF track file: an init segment, then one `moof`+`mdat` fragment per media segment.
//!
//! FilmCraft's fragmented MP4 writer does the boxes; this keeps count of where each part lands,
//! so the HLS playlists can address the init segment and every media segment of the one file by
//! byte range. Each segment records its first decode time and duration in the track's timescale.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use filmcraft_isobmff::{Brand, FragmentedWriter, TrackConfig, WriteSample, WriterOptions};

use crate::Error;

/// A part of the file: `length` bytes from `offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    /// Where it starts.
    pub offset: u64,
    /// How many bytes.
    pub length: u64,
}

/// One media segment of a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    /// Where its fragment lies.
    pub range: Range,
    /// Its first decode time, in the track's timescale.
    pub start: u64,
    /// How long it lasts, in the track's timescale.
    pub duration: u64,
}

/// A finished track file, as its playlist describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    /// Its ticks per second.
    pub timescale: u32,
    /// The init segment.
    pub init: Range,
    /// Its media segments in order.
    pub segments: Vec<Segment>,
}

impl Track {
    /// Its length in ticks.
    pub fn duration(&self) -> u64 {
        self.segments.iter().map(|s| s.duration).sum()
    }

    /// Bits per second of its busiest segment, and of the whole track.
    pub fn bandwidth(&self) -> (u64, u64) {
        let rate = |bytes: u64, ticks: u64| {
            (u128::from(bytes) * 8 * u128::from(self.timescale))
                .checked_div(u128::from(ticks))
                .map_or(0, |bits| u64::try_from(bits).unwrap_or(u64::MAX))
        };
        let peak = self
            .segments
            .iter()
            .map(|s| rate(s.range.length, s.duration))
            .max();
        let bytes = self.segments.iter().map(|s| s.range.length).sum();
        (peak.unwrap_or(0), rate(bytes, self.duration()))
    }
}

/// Counts the bytes that pass, so the writer above knows where it is.
struct Counted {
    inner: BufWriter<File>,
    written: Arc<AtomicU64>,
}

impl Write for Counted {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(bytes)?;
        self.written.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A track file being written.
pub struct TrackWriter {
    writer: FragmentedWriter<Counted>,
    written: Arc<AtomicU64>,
    track: Track,
    /// Decode time of the next sample, and of the open segment's first one.
    next: u64,
    segment_start: u64,
    pending: bool,
}

impl TrackWriter {
    /// Create `path` for one track described by `config`, and write its init segment.
    pub fn create(path: &Path, config: TrackConfig) -> Result<TrackWriter, Error> {
        let file = File::create(path).map_err(|e| Error::io(path, &e))?;
        let written = Arc::new(AtomicU64::new(0));
        let counted = Counted {
            inner: BufWriter::new(file),
            written: Arc::clone(&written),
        };
        let timescale = config.timescale;
        let mut writer = FragmentedWriter::new(counted, WriterOptions::new(Brand::Mp4));
        writer.add_track(config).map_err(Error::mux)?;
        // with nothing buffered, the first flush writes the init segment alone
        writer.flush_fragment().map_err(Error::mux)?;
        let init = Range {
            offset: 0,
            length: written.load(Ordering::Relaxed),
        };
        Ok(TrackWriter {
            writer,
            written,
            track: Track {
                timescale,
                init,
                segments: Vec::new(),
            },
            next: 0,
            segment_start: 0,
            pending: false,
        })
    }

    /// Add one sample, `duration` ticks long, to the open segment.
    pub fn sample(&mut self, data: &[u8], duration: u32, sync: bool) -> Result<(), Error> {
        let sample = WriteSample {
            data,
            duration,
            composition_offset: 0,
            is_sync: sync,
        };
        self.writer.write_sample(0, sample).map_err(Error::mux)?;
        self.next = self.next.saturating_add(u64::from(duration));
        self.pending = true;
        Ok(())
    }

    /// Close the open segment, if it holds any sample.
    pub fn cut(&mut self) -> Result<(), Error> {
        if !self.pending {
            return Ok(());
        }
        let offset = self.written.load(Ordering::Relaxed);
        self.writer.flush_fragment().map_err(Error::mux)?;
        let length = self.written.load(Ordering::Relaxed).saturating_sub(offset);
        self.track.segments.push(Segment {
            range: Range { offset, length },
            start: self.segment_start,
            duration: self.next.saturating_sub(self.segment_start),
        });
        self.segment_start = self.next;
        self.pending = false;
        Ok(())
    }

    /// Close the last segment and the file.
    pub fn finish(mut self) -> Result<Track, Error> {
        self.cut()?;
        let mut counted = self.writer.finish().map_err(Error::mux)?;
        counted.flush().map_err(|e| Error::Io(e.to_string()))?;
        Ok(self.track)
    }
}
