//! `youcraft transcode <input> <output> <memory bytes> <longest seconds>`: the child process that
//! makes one upload playable.
//!
//! It limits itself before reading a byte of the file: its address space to `memory bytes`. Once
//! the file says how long it is, it refuses a video longer than `longest seconds`, and limits its
//! CPU time and the size of each file it writes to what a video of that length needs at the
//! highest bitrate its plan allows. A decoder that runs away is stopped by the kernel, and only
//! this process dies. It reports on standard output, one line each: `progress <thousandths>`,
//! then `outcome <JSON>` or `failure <failure> <detail>`, the failure as the studio names it.

use std::io::Write;
use std::path::Path;

use rlimit::Resource;
use youcraft_api::records::Failure;
use youcraft_transcode::{Outcome, Plan};

/// CPU seconds a transcode may spend per second of video, besides [`CPU_SECONDS_BASE`].
const CPU_SECONDS_PER_SECOND: u64 = 120;
const CPU_SECONDS_BASE: u64 = 600;
/// How many times the highest rendition's bitrate cap one output file may reach, and the bytes
/// it may hold besides, for its boxes and for the stills.
const FILE_MARGIN: u64 = 2;
const FILE_BYTES_BASE: u64 = 64 << 20;

/// One line the child writes.
#[derive(Debug, PartialEq, Eq)]
pub enum Report {
    /// Thousandths done.
    Progress(u32),
    /// What was made.
    Outcome(Box<Outcome>),
    /// Why nothing was, and detail for the log.
    Failure(Failure, String),
}

impl Report {
    /// The report a line of the child's output holds.
    pub fn parse(line: &str) -> Option<Report> {
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "progress" => rest.trim().parse().ok().map(Report::Progress),
            "outcome" => serde_json::from_str(rest)
                .ok()
                .map(|o| Report::Outcome(Box::new(o))),
            "failure" => {
                let (name, detail) = rest.split_once(' ').unwrap_or((rest, ""));
                let failure = Failure::from_name(name).unwrap_or(Failure::Internal);
                Some(Report::Failure(failure, detail.to_owned()))
            }
            _ => None,
        }
    }

    /// The line that says this report.
    fn line(&self) -> Result<String, String> {
        Ok(match self {
            Report::Progress(done) => format!("progress {done}"),
            Report::Outcome(outcome) => {
                format!(
                    "outcome {}",
                    serde_json::to_string(outcome).map_err(|e| e.to_string())?
                )
            }
            // one line: what could break it becomes a space
            Report::Failure(failure, detail) => {
                format!(
                    "failure {} {}",
                    failure.as_str(),
                    detail.replace(['\n', '\r'], " ")
                )
            }
        })
    }
}

fn limit(resource: Resource, value: u64) -> Result<(), String> {
    rlimit::setrlimit(resource, value, value).map_err(|e| format!("limit {resource:?}: {e}"))
}

/// Write `report` to standard output; the parent that reads it may be gone, which changes
/// nothing.
fn say(report: &Report) {
    if let Ok(line) = report.line() {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

/// The most bytes one file made of `plan` may hold, for a video of `seconds` seconds: its
/// highest bitrate cap for every second begun, with a margin.
fn file_bytes(plan: &Plan, seconds: u64) -> u64 {
    let cap = plan.rungs.iter().map(|r| r.max_kbps).max().unwrap_or(0);
    let per_second = u64::from(cap).max(u64::from(youcraft_transcode::audio::BITRATE / 1000)) * 125;
    FILE_BYTES_BASE.saturating_add(
        per_second
            .saturating_mul(seconds.saturating_add(1))
            .saturating_mul(FILE_MARGIN),
    )
}

/// The limits a transcode lives within, from the command line's numbers.
fn numbers(memory: &str, longest: &str) -> Result<(u64, u64), String> {
    let number = |text: &str, what: &str| {
        text.parse::<u64>()
            .map_err(|e| format!("{what} {text}: {e}"))
    };
    Ok((
        number(memory, "memory bytes")?,
        number(longest, "longest seconds")?,
    ))
}

/// Make `input` into `output` within the address space `memory_bytes`, refusing a video longer
/// than `longest_seconds`.
fn made(
    input: &Path,
    output: &Path,
    memory_bytes: u64,
    longest_seconds: u64,
) -> Result<Outcome, (Failure, String)> {
    let refused = |error: youcraft_transcode::Error| (error.failure(), error.to_string());
    let internal = |detail: String| (Failure::Internal, detail);
    limit(Resource::AS, memory_bytes).map_err(internal)?;
    let source = youcraft_transcode::open(input).map_err(refused)?;
    let plan = youcraft_transcode::plan(&source).map_err(refused)?;
    let seconds = plan.timing.seconds();
    if seconds > longest_seconds {
        return Err((
            Failure::TooLong,
            format!("{seconds} s, more than {longest_seconds} s"),
        ));
    }
    let cpu = CPU_SECONDS_BASE.saturating_add(seconds.saturating_mul(CPU_SECONDS_PER_SECOND));
    limit(Resource::CPU, cpu).map_err(internal)?;
    limit(Resource::FSIZE, file_bytes(&plan, seconds)).map_err(internal)?;
    let last = std::sync::atomic::AtomicU32::new(0);
    let progress = |done: u32| {
        // one line per percent is plenty
        if done / 10 > last.fetch_max(done / 10, std::sync::atomic::Ordering::Relaxed) {
            say(&Report::Progress(done));
        }
    };
    youcraft_transcode::transcode(&source, &plan, output, &progress).map_err(refused)
}

/// Run the transcode of `input` into `output` under the limits of `memory` bytes and `longest`
/// seconds, reporting on standard output.
pub fn run(input: &Path, output: &Path, memory: &str, longest: &str) -> Result<(), String> {
    let (memory_bytes, longest_seconds) = numbers(memory, longest)?;
    match made(input, output, memory_bytes, longest_seconds) {
        Ok(outcome) => {
            say(&Report::Outcome(Box::new(outcome)));
            Ok(())
        }
        Err((failure, detail)) => {
            let message = format!("transcode: {}: {detail}", failure.as_str());
            say(&Report::Failure(failure, detail));
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn reports_read_back_from_their_lines() {
        assert_eq!(Report::parse("progress 420"), Some(Report::Progress(420)));
        let failure = Report::Failure(
            Failure::Unsupported,
            "unsupported: the file has no video".to_owned(),
        );
        assert_eq!(Report::parse(&failure.line().unwrap()), Some(failure));
        let split = Report::Failure(Failure::Decode, "line one\nline two".to_owned());
        assert_eq!(
            Report::parse(&split.line().unwrap()),
            Some(Report::Failure(
                Failure::Decode,
                "line one line two".to_owned()
            ))
        );
        assert_eq!(
            Report::parse("failure nonsense"),
            Some(Report::Failure(Failure::Internal, String::new()))
        );
        assert_eq!(Report::parse("outcome {not json"), None);
        assert_eq!(Report::parse("anything else"), None);
        let outcome = Outcome {
            duration: youcraft_api::time::MediaDuration {
                ticks: 3003,
                timescale: NonZeroU32::new(90_000).unwrap(),
            },
            width: 640,
            height: 360,
            renditions: Vec::new(),
        };
        let report = Report::Outcome(Box::new(outcome));
        assert_eq!(Report::parse(&report.line().unwrap()), Some(report));
    }

    #[test]
    fn a_file_may_hold_twice_its_highest_bitrate() {
        let source_of = |width, height, fps| {
            let dir = std::env::temp_dir().join(format!(
                "youcraft-limits-{width}-{fps}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("in.mp4");
            let spec = youcraft_transcode::synthetic::Synthetic {
                width,
                height,
                par: (1, 1),
                fps: (fps, 1),
                frames: 2,
                sound: None,
                hue: 0,
            };
            youcraft_transcode::synthetic::write_mp4(&spec, &path).unwrap();
            let plan = youcraft_transcode::plan(&youcraft_transcode::open(&path).unwrap()).unwrap();
            let _ = std::fs::remove_dir_all(&dir);
            plan
        };
        // a 32:9 picture at 60 frames per second: 3840×1080 aims at 19.9 Mbit/s, capped at 29.9
        let wide = source_of(3840, 1080, 60);
        let cap = wide.rungs.iter().map(|r| r.max_kbps).max().unwrap();
        assert!(cap > 29_000, "{cap}");
        let hour = file_bytes(&wide, 3600);
        assert!(hour >= u64::from(cap) * 125 * 3600 * 2, "{hour}");
        // a tiny one still has room for its stills
        assert!(file_bytes(&source_of(64, 36, 30), 0) >= FILE_BYTES_BASE);
    }

    #[test]
    fn limits_come_as_numbers() {
        assert_eq!(numbers("8589934592", "43200"), Ok((8 << 30, 43_200)));
        assert!(numbers("lots", "43200").is_err());
    }
}
