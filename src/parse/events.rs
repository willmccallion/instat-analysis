//! InStat's event export (CSV): one row per action with its video time, position on the ice
//! and player.
//!
//! A game comes as two files: the players file (every player action and every shift) and
//! the team file (team-level actions such as scoring chances, power-play periods and time in
//! each zone). Only the file name carries the date and the final score.

use crate::error::Error;
use crate::model::{Date, Seconds, TeamName};
use crate::parse::match_report::Title;

const SECTION: &str = "event export";
const HEADER: &str = "ID,start,end,duration,pos_x,pos_y,player,team,action,half";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFileKind {
    /// Player actions and shifts.
    Players,
    /// Team actions: scoring chances, power-play periods, zone time.
    Team,
}

/// A moment in the game video, to the millisecond. The rows describing one action (say
/// "Shots", "Shots on goal" and "Goals") share it exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VideoTime(u32);

impl VideoTime {
    pub const END: Self = Self(u32::MAX);

    #[must_use]
    pub fn seconds(self) -> f64 {
        f64::from(self.0) / 1000.0
    }

    const fn midpoint(self, later: Self) -> Self {
        Self(self.0 + (later.0 - self.0) / 2)
    }

    /// Parses the export's non-negative decimal seconds, e.g. `"125"` or `"125.4"`.
    pub(crate) fn parse(text: &str, what: &str) -> Result<Self, Error> {
        let bad = || Error::parse(SECTION, format!("{what} \"{text}\" is not a time in seconds"));
        let trimmed = text.trim();
        let (whole, fraction) = trimmed.split_once('.').unwrap_or((trimmed, ""));
        if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let whole: u32 = whole.parse().map_err(|_| bad())?;
        let millis: u32 = format!("{fraction:0<3}").parse().map_err(|_| bad())?;
        whole.checked_mul(1000).and_then(|w| w.checked_add(millis)).map(Self).ok_or_else(bad)
    }
}

/// Something a player or team did at one moment.
///
/// The export lists each action as a 12-second video clip; the action is at its middle.
/// Positions are metres on a 60.96 × 25.92 m rink, seen from the acting team's attack
/// (their target net at large x, y toward their left), except faceoffs, whose rows for
/// both teams share one position in the winning team's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub at: VideoTime,
    pub position: Option<(f64, f64)>,
    pub player: Option<String>,
    pub team: TeamName,
    pub name: String,
    pub period: u32,
}

/// A stretch of play: a shift, a power play, a controlled breakout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: VideoTime,
    pub end: VideoTime,
    pub player: Option<String>,
    pub team: TeamName,
    pub name: String,
    pub period: u32,
}

impl Span {
    #[must_use]
    pub fn duration(&self) -> Seconds {
        Seconds(self.end.seconds() - self.start.seconds())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventFile {
    pub kind: EventFileKind,
    pub title: Title,
    pub actions: Vec<Action>,
    pub spans: Vec<Span>,
}

enum Row {
    Action(Action),
    Span(Span),
}

fn is_span(name: &str) -> bool {
    name.ends_with(" shifts") || matches!(name, "Power play" | "Short-handed" | "Controlled breakouts")
}

/// Whether `bytes` look like an InStat event export.
#[must_use]
pub fn is_event_export(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    text.starts_with(HEADER.as_bytes())
}

fn number(text: &str, what: &str) -> Result<f64, Error> {
    text.trim().parse().map_err(|_| Error::parse(SECTION, format!("{what} \"{text}\" is not a number")))
}

fn row(line: &str) -> Result<Row, Error> {
    let fields: Vec<&str> = line.split(',').collect();
    let [_, start, end, _, x, y, player, team, name, period] = fields.as_slice() else {
        return Err(Error::parse(SECTION, format!("expected 10 columns, found {}", fields.len())));
    };
    let position = match (x.trim(), y.trim()) {
        ("", _) | (_, "") => None,
        (x, y) => Some((number(x, "pos_x")?, number(y, "pos_y")?)),
    };
    let period = period
        .trim()
        .parse()
        .map_err(|_| Error::parse(SECTION, format!("period \"{period}\" is not a number")))?;
    let (start, end) = (VideoTime::parse(start, "start")?, VideoTime::parse(end, "end")?);
    if end < start {
        return Err(Error::parse(SECTION, format!("\"{name}\" ends before it starts")));
    }
    let player = Some(player.trim()).filter(|p| !p.is_empty()).map(str::to_owned);
    let (team, name) = (TeamName::new(team), name.trim().to_owned());
    Ok(if is_span(&name) {
        Row::Span(Span { start, end, player, team, name, period })
    } else {
        Row::Action(Action { at: start.midpoint(end), position, player, team, name, period })
    })
}

/// Teams, score and date from the downloaded file name, e.g.
/// `"SSAC Lions U15 AAA 4 _ 5 CAC Canadians U15 AAA 26.09.2026-2.csv"`.
fn title_from_name(file_name: &str, teams: &[TeamName]) -> Result<Title, Error> {
    let keep_name = || {
        Error::parse(
            SECTION,
            format!("\"{file_name}\" isn't named the way InStat downloads it (\"Team A 4 _ 5 Team B 26.09.2026.csv\"); the date and score come from the name"),
        )
    };
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem.strip_suffix(".csv").or_else(|| stem.strip_suffix(".CSV")).unwrap_or(stem);
    let mut words: Vec<&str> = stem.split_whitespace().collect();
    if words.last().is_some_and(|w| w.starts_with('(') && w.ends_with(')')) {
        words.pop();
    }
    let last = words.pop().ok_or_else(keep_name)?;
    let date_text = last.split(['-', '(']).next().unwrap_or(last);
    let date = Date::parse_dotted(date_text).ok_or_else(keep_name)?;
    let middle = words.join(" ");
    let (first_part, second_part) = middle.split_once(" _ ").ok_or_else(keep_name)?;
    let (first_name, first_score) = first_part.rsplit_once(' ').ok_or_else(keep_name)?;
    let (second_score, second_name) = second_part.split_once(' ').ok_or_else(keep_name)?;
    let score = (first_score.parse().map_err(|_| keep_name())?, second_score.parse().map_err(|_| keep_name())?);
    let named = |name: &str| {
        let wanted = TeamName::new(name);
        teams.iter().find(|t| **t == wanted).cloned().ok_or_else(|| {
            Error::parse(SECTION, format!("the file name says \"{name}\" but the file has {}", teams.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(" and ")))
        })
    };
    Ok(Title { teams: [named(first_name)?, named(second_name)?], score, date })
}

pub fn parse(bytes: &[u8], file_name: &str) -> Result<EventFile, Error> {
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
        .map_err(|_| Error::parse(SECTION, "not UTF-8 text"))?;
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some(HEADER) {
        return Err(Error::parse(SECTION, "not an InStat event export"));
    }
    let (mut actions, mut spans) = (Vec::new(), Vec::new());
    for line in lines.filter(|l| !l.trim().is_empty()) {
        match row(line)? {
            Row::Action(action) => actions.push(action),
            Row::Span(span) => spans.push(span),
        }
    }
    let mut teams: Vec<TeamName> = actions.iter().map(|a| a.team.clone()).chain(spans.iter().map(|s| s.team.clone())).collect();
    teams.sort();
    teams.dedup();
    if teams.len() != 2 {
        return Err(Error::parse(SECTION, format!("expected two teams, found {}", teams.len())));
    }
    let kind = if spans.iter().any(|s| s.name == "All shifts") {
        EventFileKind::Players
    } else if actions.iter().any(|a| a.name == "OZ play") {
        EventFileKind::Team
    } else {
        return Err(Error::parse(SECTION, "neither a players file (with shifts) nor a team file (with zone play)"));
    };
    Ok(EventFile { kind, title: title_from_name(file_name, &teams)?, actions, spans })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "ID,start,end,duration,pos_x,pos_y,player,team,action,half\n\
        1,3,45,42,,,Smith John,Team One,All shifts,1\n\
        2,40,52,12,30.48,12.96,Jones Bob,Team One,Shots,1\n\
        3,40,52,12,,,,Team Two,Penalties,1\n";

    #[test]
    fn reads_the_title_from_the_file_name() {
        let file = parse(SAMPLE.as_bytes(), "Team One 4 _ 5 Team Two 26.09.2026-2.csv").unwrap();
        assert_eq!(file.kind, EventFileKind::Players);
        assert_eq!(file.title.teams[0].0, "TEAM ONE");
        assert_eq!(file.title.score, (4, 5));
        assert_eq!(file.title.date, Date { year: 2026, month: 9, day: 26 });
    }

    #[test]
    fn shifts_are_spans_and_other_rows_are_moments_at_the_middle_of_their_clip() {
        let file = parse(SAMPLE.as_bytes(), "Team One 4 _ 5 Team Two 26.09.2026.csv").unwrap();
        assert_eq!(file.spans.len(), 1);
        assert_eq!((file.spans[0].start, file.spans[0].end), (VideoTime(3000), VideoTime(45_000)));
        assert_eq!(file.actions[0].at, VideoTime(46_000));
        assert_eq!(file.actions[0].position, Some((30.48, 12.96)));
        assert_eq!(file.actions[1].player, None);
        assert_eq!(file.actions[1].position, None);
    }

    #[test]
    fn video_times_are_exact_to_the_millisecond() {
        assert_eq!(VideoTime::parse("125", "start").unwrap(), VideoTime(125_000));
        assert_eq!(VideoTime::parse("125.4", "start").unwrap(), VideoTime(125_400));
        assert_eq!(VideoTime::parse("0.125", "start").unwrap(), VideoTime(125));
        assert!(VideoTime::parse("-1", "start").is_err());
        assert!(VideoTime::parse("1.2345", "start").is_err());
    }

    #[test]
    fn a_renamed_file_is_refused_with_a_reason() {
        let err = parse(SAMPLE.as_bytes(), "game.csv").unwrap_err().to_string();
        assert!(err.contains("isn't named the way InStat downloads it"), "{err}");
    }

    #[test]
    fn a_browser_copy_suffix_is_ignored() {
        let file = parse(SAMPLE.as_bytes(), "Team One 4 _ 5 Team Two 26.09.2026 (1).csv").unwrap();
        assert_eq!(file.title.date, Date { year: 2026, month: 9, day: 26 });
    }

    #[test]
    fn a_byte_order_mark_is_accepted() {
        let with_bom = [b"\xEF\xBB\xBF".as_slice(), SAMPLE.as_bytes()].concat();
        assert!(is_event_export(&with_bom));
        assert!(parse(&with_bom, "Team One 4 _ 5 Team Two 26.09.2026.csv").is_ok());
    }
}
