//! YouCraft's default algorithm, an ordinary implementation of contract v1.
//!
//! Each candidate scores its popularity, the log of its views plus its likes, weighed by a
//! half-life on its age, then multiplied by what the viewer is to its channel: subscribed,
//! recently watched, or (beside a video) the same channel. A video already watched falls back,
//! the one being watched never appears, and no channel takes more than two places in a row
//! while another can fill one. Ties break by id, so the order is a function of the request.

use std::cmp::Ordering;

use crate::v1::{Algorithm, Candidate, Ranking, Request, Surface};

/// The default algorithm.
#[derive(Clone, Copy, Debug, Default)]
pub struct Default;

/// The age at which a video's score halves: one week.
const HALF_LIFE_SECONDS: f64 = 7.0 * 24.0 * 3600.0;

impl Algorithm for Default {
    fn rank(&self, request: &Request) -> Ranking {
        let watching = match &request.surface {
            Surface::Home => None,
            Surface::UpNext { watching, channel } => Some((watching.as_str(), channel.as_str())),
        };
        let mut scored: Vec<(f64, &Candidate)> = request
            .candidates
            .iter()
            .filter(|c| watching.is_none_or(|(video, _)| c.video != video))
            .map(|c| (score(request, watching.map(|(_, channel)| channel), c), c))
            .collect();
        scored.sort_by(|(a, x), (b, y)| {
            b.partial_cmp(a)
                .unwrap_or(Ordering::Equal)
                .then_with(|| x.video.cmp(&y.video))
        });
        let order = spread_channels(scored.into_iter().map(|(_, c)| c).collect());
        Ranking {
            videos: order.into_iter().map(|c| c.video.clone()).collect(),
        }
        .bounded_by(request)
    }
}

fn score(request: &Request, beside: Option<&str>, candidate: &Candidate) -> f64 {
    let views = (candidate.views as f64).ln_1p();
    let likes = (candidate.likes as f64).ln_1p();
    let age = (request.now.saturating_sub(candidate.published_at)).max(0) as f64;
    let freshness = (-age / HALF_LIFE_SECONDS * std::f64::consts::LN_2).exp();
    let mut weight = 1.0;
    let viewer = &request.viewer;
    if viewer
        .subscribed
        .iter()
        .any(|channel| channel == &candidate.channel)
    {
        weight *= 2.0;
    }
    if viewer
        .watched
        .iter()
        .take(50)
        .any(|w| w.channel == candidate.channel)
    {
        weight *= 1.5;
    }
    if beside == Some(candidate.channel.as_str()) {
        weight *= 3.0;
    }
    if viewer.watched.iter().any(|w| w.video == candidate.video) {
        weight *= 0.2;
    }
    (1.0 + views + 0.5 * likes) * (0.25 + freshness) * weight
}

/// Reorder so that no channel holds more than two places in a row while another channel's video
/// is still waiting; otherwise keep the order.
fn spread_channels(mut waiting: Vec<&Candidate>) -> Vec<&Candidate> {
    let mut order: Vec<&Candidate> = Vec::with_capacity(waiting.len());
    while !waiting.is_empty() {
        let crowded = match order.as_slice() {
            [.., a, b] if a.channel == b.channel => Some(b.channel.as_str()),
            _ => None,
        };
        let next = waiting
            .iter()
            .position(|c| crowded != Some(c.channel.as_str()))
            .unwrap_or(0);
        order.push(waiting.remove(next));
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::{Viewer, Watched};

    fn candidate(video: &str, channel: &str, views: u64, published_at: i64) -> Candidate {
        Candidate {
            video: video.into(),
            channel: channel.into(),
            title: format!("{video} by {channel}"),
            published_at,
            duration_ms: 60_000,
            views,
            likes: views / 10,
            dislikes: 0,
            comments: 0,
        }
    }

    fn request(
        surface: Surface,
        viewer: Viewer,
        candidates: Vec<Candidate>,
        limit: u32,
    ) -> Request {
        Request {
            surface,
            now: 1_000_000_000,
            viewer,
            candidates,
            limit,
        }
    }

    #[test]
    fn the_ranking_holds_candidates_only_and_respects_the_limit() {
        let all: Vec<_> = (0..20)
            .map(|i| {
                candidate(
                    &format!("v{i:02}"),
                    &format!("c{}", i % 5),
                    i * 100,
                    999_000_000,
                )
            })
            .collect();
        let ranking = Default.rank(&request(Surface::Home, Viewer::default(), all.clone(), 7));
        assert_eq!(ranking.videos.len(), 7);
        assert!(
            ranking
                .videos
                .iter()
                .all(|v| all.iter().any(|c| &c.video == v))
        );
        assert_eq!(
            ranking,
            Default.rank(&request(Surface::Home, Viewer::default(), all, 7)),
            "the same request, the same order"
        );
    }

    #[test]
    fn up_next_never_shows_the_video_being_watched_and_favours_its_channel() {
        let all = vec![
            candidate("a", "x", 10, 999_999_000),
            candidate("b", "y", 10, 999_999_000),
            candidate("c", "z", 10, 999_999_000),
        ];
        let surface = Surface::UpNext {
            watching: "a".into(),
            channel: "z".into(),
        };
        let ranking = Default.rank(&request(surface, Viewer::default(), all, 10));
        assert_eq!(ranking.videos, ["c", "b"]);
    }

    #[test]
    fn subscriptions_rise_and_watched_videos_fall() {
        let all = vec![
            candidate("a", "x", 100, 999_999_000),
            candidate("b", "y", 100, 999_999_000),
        ];
        let viewer = Viewer {
            signed_in: true,
            subscribed: vec!["y".into()],
            ..Viewer::default()
        };
        assert_eq!(
            Default
                .rank(&request(Surface::Home, viewer, all.clone(), 10))
                .videos,
            ["b", "a"]
        );
        let viewer = Viewer {
            signed_in: true,
            watched: vec![Watched {
                video: "a".into(),
                channel: "z".into(),
                at: 0,
            }],
            ..Viewer::default()
        };
        assert_eq!(
            Default
                .rank(&request(Surface::Home, viewer, all, 10))
                .videos,
            ["b", "a"]
        );
    }

    #[test]
    fn no_channel_takes_three_places_in_a_row_while_another_waits() {
        let mut all: Vec<_> = (0..5)
            .map(|i| candidate(&format!("x{i}"), "x", 1000, 999_999_000))
            .collect();
        all.push(candidate("y0", "y", 1, 999_999_000));
        let ranking = Default.rank(&request(Surface::Home, Viewer::default(), all, 10));
        let channels: Vec<char> = ranking
            .videos
            .iter()
            .filter_map(|v| v.chars().next())
            .collect();
        assert_eq!(channels, ['x', 'x', 'y', 'x', 'x', 'x']);
    }

    #[test]
    fn the_contract_travels_as_json() {
        let request = request(
            Surface::UpNext {
                watching: "a".into(),
                channel: "x".into(),
            },
            Viewer::default(),
            vec![candidate("b", "x", 1, 0)],
            3,
        );
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["surface"], "up_next");
        assert_eq!(json["watching"], "a");
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request);
    }
}
