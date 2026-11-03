//! The keyboard shortcuts, as YouTube's player answers them.
//!
//! `k` or space plays and pauses, `j` and `l` jump ten seconds, the arrows five seconds or five
//! points of volume, `0`–`9` jump to a tenth of the video, Home and End to its ends, `<` and `>`
//! change the speed, `,` and `.` step a frame while paused, `m` mutes, `f` fills the screen and
//! `t` toggles theatre mode. Time runs left to right in every language, so the arrows do too.

/// What a key asks of the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Play if paused, pause if playing.
    TogglePlay,
    /// Move the playhead by this many seconds.
    Jump(i32),
    /// Move the playhead to this many tenths of the video.
    JumpToTenth(u8),
    /// Move the playhead to the start.
    JumpToStart,
    /// Move the playhead to the end.
    JumpToEnd,
    /// Change the volume by this many points out of 100.
    Volume(i32),
    /// Mute or unmute.
    ToggleMute,
    /// Enter or leave full screen.
    ToggleFullScreen,
    /// Enter or leave theatre mode.
    ToggleTheatre,
    /// Step the speed one choice up (1) or down (-1).
    Speed(i32),
    /// Step one frame forward (1) or back (-1), while paused.
    Frame(i32),
}

/// The action of the key `key` (a `KeyboardEvent.key`), pressed with Control, Alt or Meta held
/// or not; `None` leaves the key to the page.
pub fn action(key: &str, modified: bool) -> Option<Action> {
    if modified {
        return None;
    }
    let action = match key {
        " " | "k" | "K" => Action::TogglePlay,
        "j" | "J" => Action::Jump(-10),
        "l" | "L" => Action::Jump(10),
        "ArrowLeft" => Action::Jump(-5),
        "ArrowRight" => Action::Jump(5),
        "ArrowUp" => Action::Volume(5),
        "ArrowDown" => Action::Volume(-5),
        "Home" => Action::JumpToStart,
        "End" => Action::JumpToEnd,
        "m" | "M" => Action::ToggleMute,
        "f" | "F" => Action::ToggleFullScreen,
        "t" | "T" => Action::ToggleTheatre,
        "<" => Action::Speed(-1),
        ">" => Action::Speed(1),
        "," => Action::Frame(-1),
        "." => Action::Frame(1),
        digit => {
            let mut chars = digit.chars();
            let tenth = chars.next()?.to_digit(10)?;
            if chars.next().is_some() {
                return None;
            }
            Action::JumpToTenth(u8::try_from(tenth).ok()?)
        }
    };
    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_do_what_they_do_on_youtube() {
        assert_eq!(action("k", false), Some(Action::TogglePlay));
        assert_eq!(action(" ", false), Some(Action::TogglePlay));
        assert_eq!(action("j", false), Some(Action::Jump(-10)));
        assert_eq!(action("L", false), Some(Action::Jump(10)));
        assert_eq!(action("ArrowLeft", false), Some(Action::Jump(-5)));
        assert_eq!(action("ArrowUp", false), Some(Action::Volume(5)));
        assert_eq!(action("7", false), Some(Action::JumpToTenth(7)));
        assert_eq!(action(">", false), Some(Action::Speed(1)));
        assert_eq!(action(",", false), Some(Action::Frame(-1)));
        assert_eq!(action("t", false), Some(Action::ToggleTheatre));
    }

    #[test]
    fn other_keys_and_chords_are_left_to_the_page() {
        assert_eq!(action("k", true), None);
        assert_eq!(action("q", false), None);
        assert_eq!(action("12", false), None);
        assert_eq!(action("٣", false), None);
        assert_eq!(action("", false), None);
    }
}
