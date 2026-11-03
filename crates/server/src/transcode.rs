//! `youcraft transcode <input> <output>`: the child process that makes one upload playable.
//!
//! It limits itself before reading a byte of the file: its address space to the configured
//! memory, then, once the file says how long it is, its CPU time and the size of each file it
//! writes to what a video of that length needs. A decoder that runs away is stopped by the
//! kernel, and only this process dies. It reports on standard output, one line each:
//! `progress <thousandths>`, then `outcome <JSON>` or `failure <kind> <message>`.

use std::io::Write;
use std::path::Path;

use rlimit::Resource;
use youcraft_transcode::Outcome;

use crate::config::Config;

/// CPU seconds a transcode may spend per second of video, besides [`CPU_SECONDS_BASE`].
const CPU_SECONDS_PER_SECOND: u64 = 120;
const CPU_SECONDS_BASE: u64 = 600;
/// Bytes per second of video one output file may hold: 16 Mbit/s, twice the highest rendition's
/// bitrate cap.
const FILE_BYTES_PER_SECOND: u64 = 2 << 20;
const FILE_BYTES_BASE: u64 = 64 << 20;

/// One line the child writes.
#[derive(Debug, PartialEq, Eq)]
pub enum Report {
    /// Thousandths done.
    Progress(u32),
    /// What was made.
    Outcome(Box<Outcome>),
    /// Why nothing was: a kind the studio names (`unsupported`, `decode`, `internal`), and detail
    /// for the log.
    Failure(String),
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
            "failure" => Some(Report::Failure(
                rest.split(' ').next().unwrap_or("internal").to_owned(),
            )),
            _ => None,
        }
    }
}

fn limit(resource: Resource, value: u64) -> Result<(), String> {
    rlimit::setrlimit(resource, value, value).map_err(|e| format!("limit {resource:?}: {e}"))
}

/// Write `line` to standard output; the parent that reads it may be gone, which changes nothing.
fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Run the transcode of `input` into `output` under limits, reporting on standard output.
pub fn run(input: &Path, output: &Path) -> Result<(), String> {
    let config = Config::from_env()?;
    let result = limit(Resource::AS, config.transcode.memory_bytes)
        .map_err(youcraft_transcode::Error::Io)
        .and_then(|()| {
            let source = youcraft_transcode::open(input)?;
            let plan = youcraft_transcode::plan(&source)?;
            let seconds = plan
                .timing
                .frames
                .saturating_mul(u64::from(plan.timing.frame))
                / u64::from(plan.timing.timescale.max(1));
            let cpu =
                CPU_SECONDS_BASE.saturating_add(seconds.saturating_mul(CPU_SECONDS_PER_SECOND));
            let file =
                FILE_BYTES_BASE.saturating_add(seconds.saturating_mul(FILE_BYTES_PER_SECOND));
            limit(Resource::CPU, cpu).map_err(youcraft_transcode::Error::Io)?;
            limit(Resource::FSIZE, file).map_err(youcraft_transcode::Error::Io)?;
            drop(source);
            let last = std::sync::atomic::AtomicU32::new(0);
            let progress = |done: u32| {
                // one line per percent is plenty
                if done / 10 > last.fetch_max(done / 10, std::sync::atomic::Ordering::Relaxed) {
                    say(&format!("progress {done}"));
                }
            };
            youcraft_transcode::transcode(input, output, &progress)
        });
    match result {
        Ok(outcome) => {
            let json = serde_json::to_string(&outcome).map_err(|e| e.to_string())?;
            say(&format!("outcome {json}"));
            Ok(())
        }
        Err(error) => {
            say(&format!("failure {} {error}", error.kind()));
            Err(format!("transcode: {error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_read_back_from_their_lines() {
        assert_eq!(Report::parse("progress 420"), Some(Report::Progress(420)));
        assert_eq!(
            Report::parse("failure unsupported unsupported: the file has no video"),
            Some(Report::Failure("unsupported".to_owned()))
        );
        assert_eq!(Report::parse("outcome {not json"), None);
        assert_eq!(Report::parse("anything else"), None);
        let outcome = Outcome {
            duration: youcraft_api::time::MediaDuration {
                ticks: 3003,
                timescale: 90_000,
            },
            width: 640,
            height: 360,
            renditions: Vec::new(),
        };
        let line = format!("outcome {}", serde_json::to_string(&outcome).unwrap());
        assert_eq!(
            Report::parse(&line),
            Some(Report::Outcome(Box::new(outcome)))
        );
    }
}
