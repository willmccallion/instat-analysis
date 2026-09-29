//! Typed interpretation of InStat table cells such as `"6 / 4 67%"`, `"24:24"` or `"1—0"`.

use serde::Serialize;

/// A parsed table cell. InStat prints "—" (or "-") for zero/none.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t", content = "v")]
pub enum Cell {
    Empty,
    Int(i64),
    Decimal(f64),
    Percent(f64),
    /// Seconds, from `mm:ss`.
    Clock(u32),
    /// `a / b`, optionally followed by a percentage that InStat derives from them.
    Ratio(u32, u32),
    /// `a / b / c`, e.g. shots / on goal / goals.
    Triple(u32, u32, u32),
    /// A count with its share, e.g. `"33 40%"`.
    CountShare(u32, f64),
    /// A duration with its share, e.g. `"07:33 44%"`.
    ClockShare(u32, f64),
    /// `a—b`, e.g. own goals — opponent goals, or won — lost.
    Pair(u32, u32),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Atom {
    Int(i64),
    Decimal(f64),
    Percent(f64),
    Clock(u32),
    Dash,
    Slash,
    Text(String),
}

const DASHES: [char; 3] = ['—', '–', '-'];

fn atoms(text: &str) -> Vec<Atom> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        split_token(token, &mut out);
    }
    out
}

fn split_token(token: &str, out: &mut Vec<Atom>) {
    if token.is_empty() {
        return;
    }
    if let Some(atom) = numeric_atom(token) {
        out.push(atom);
        return;
    }
    if let Some(pos) = token.find(['—', '–', '/']) {
        let (head, rest) = token.split_at(pos);
        let mut chars = rest.chars();
        let separator = chars.next();
        split_token(head, out);
        out.push(if separator == Some('/') {
            Atom::Slash
        } else {
            Atom::Dash
        });
        split_token(chars.as_str(), out);
        return;
    }
    if token.chars().all(|c| DASHES.contains(&c)) {
        out.push(Atom::Dash);
        return;
    }
    out.push(Atom::Text(token.to_owned()));
}

fn numeric_atom(token: &str) -> Option<Atom> {
    if let Some(number) = token.strip_suffix('%') {
        return number.parse::<f64>().ok().map(Atom::Percent);
    }
    if let Some((minutes, seconds)) = token.split_once(':') {
        let minutes: u32 = minutes.parse().ok()?;
        let seconds: u32 = seconds.parse().ok()?;
        return (seconds < 60).then_some(Atom::Clock(minutes * 60 + seconds));
    }
    if token.starts_with('+')
        || token.starts_with('-')
        || token.starts_with(|c: char| c.is_ascii_digit())
    {
        let unsigned = token.strip_prefix('+').unwrap_or(token);
        if let Ok(value) = unsigned.parse::<i64>() {
            return Some(Atom::Int(value));
        }
        if unsigned.contains('.')
            && let Ok(value) = unsigned.parse::<f64>()
        {
            return Some(Atom::Decimal(value));
        }
    }
    None
}

fn non_negative(value: i64) -> Option<u32> {
    u32::try_from(value).ok()
}

impl Cell {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let parsed = atoms(text);
        Self::from_atoms(&parsed).unwrap_or_else(|| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Self::Empty
            } else {
                Self::Text(trimmed.to_owned())
            }
        })
    }

    fn from_atoms(atoms: &[Atom]) -> Option<Self> {
        use Atom as A;
        let cell = match atoms {
            [] | [A::Dash] => Self::Empty,
            [A::Int(n)] => Self::Int(*n),
            [A::Decimal(d)] => Self::Decimal(*d),
            [A::Percent(p)] => Self::Percent(*p),
            [A::Clock(s)] => Self::Clock(*s),
            [A::Int(a), A::Slash, A::Int(b)] | [A::Int(a), A::Slash, A::Int(b), A::Percent(_)] => {
                Self::Ratio(non_negative(*a)?, non_negative(*b)?)
            }
            [A::Int(a), A::Slash, A::Int(b), A::Slash, A::Int(c)] => {
                Self::Triple(non_negative(*a)?, non_negative(*b)?, non_negative(*c)?)
            }
            [A::Int(n), A::Percent(p)] => Self::CountShare(non_negative(*n)?, *p),
            [A::Clock(s), A::Percent(p)] => Self::ClockShare(*s, *p),
            [A::Int(a), A::Dash, A::Int(b)] => Self::Pair(non_negative(*a)?, non_negative(*b)?),
            _ => return None,
        };
        Some(cell)
    }

    /// Treats "—" as zero and plain counts as themselves.
    #[must_use]
    pub fn count(&self) -> Option<u32> {
        match *self {
            Self::Empty => Some(0),
            Self::Int(n) => u32::try_from(n).ok(),
            Self::CountShare(n, _) => Some(n),
            _ => None,
        }
    }

    /// Signed integer such as +/- or Corsi differential.
    #[must_use]
    pub const fn signed(&self) -> Option<i64> {
        match *self {
            Self::Empty => Some(0),
            Self::Int(n) => Some(n),
            _ => None,
        }
    }

    #[must_use]
    pub const fn seconds(&self) -> Option<u32> {
        match *self {
            Self::Empty => Some(0),
            Self::Clock(s) | Self::ClockShare(s, _) => Some(s),
            _ => None,
        }
    }

    /// `(a, b)` from `a / b` (e.g. attempts / successes); "—" is `(0, 0)`.
    #[must_use]
    pub const fn ratio(&self) -> Option<(u32, u32)> {
        match *self {
            Self::Empty => Some((0, 0)),
            Self::Ratio(a, b) => Some((a, b)),
            _ => None,
        }
    }

    /// `(a, b)` from `a—b`; "—" is `(0, 0)`.
    #[must_use]
    pub const fn pair(&self) -> Option<(u32, u32)> {
        match *self {
            Self::Empty => Some((0, 0)),
            Self::Pair(a, b) => Some((a, b)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn decimal(&self) -> Option<f64> {
        match *self {
            Self::Empty => Some(0.0),
            Self::Decimal(d) => Some(d),
            Self::Int(n) => Some(n as f64),
            _ => None,
        }
    }

    /// A percentage (0–100); `None` for "—", since InStat uses it when the base is zero.
    #[must_use]
    pub const fn percent(&self) -> Option<f64> {
        match *self {
            Self::Percent(p) | Self::CountShare(_, p) | Self::ClockShare(_, p) => Some(p),
            _ => None,
        }
    }

    #[must_use]
    pub const fn triple(&self) -> Option<(u32, u32, u32)> {
        match *self {
            Self::Empty => Some((0, 0, 0)),
            Self::Triple(a, b, c) => Some((a, b, c)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cell;

    #[test]
    fn dash_means_empty() {
        assert_eq!(Cell::parse("—"), Cell::Empty);
        assert_eq!(Cell::parse(""), Cell::Empty);
    }

    #[test]
    fn signed_integers_keep_their_sign() {
        assert_eq!(Cell::parse("+2"), Cell::Int(2));
        assert_eq!(Cell::parse("-19"), Cell::Int(-19));
    }

    #[test]
    fn clock_is_seconds() {
        assert_eq!(Cell::parse("27:46"), Cell::Clock(27 * 60 + 46));
    }

    #[test]
    fn ratio_ignores_trailing_percentage() {
        assert_eq!(Cell::parse("3 / 1 33%"), Cell::Ratio(3, 1));
        assert_eq!(Cell::parse("1/0"), Cell::Ratio(1, 0));
        assert_eq!(Cell::parse("5/ 2"), Cell::Ratio(5, 2));
    }

    #[test]
    fn em_dash_between_numbers_is_a_pair() {
        assert_eq!(Cell::parse("1 — 0"), Cell::Pair(1, 0));
        assert_eq!(Cell::parse("11—4"), Cell::Pair(11, 4));
    }

    #[test]
    fn triple_and_shares() {
        assert_eq!(Cell::parse("44 / 29 / 4"), Cell::Triple(44, 29, 4));
        assert_eq!(Cell::parse("33 40%"), Cell::CountShare(33, 40.0));
        assert_eq!(Cell::parse("07:33 44%"), Cell::ClockShare(453, 44.0));
    }

    #[test]
    fn decimals_parse() {
        assert_eq!(Cell::parse("0.14"), Cell::Decimal(0.14));
    }

    #[test]
    fn unknown_text_is_kept() {
        assert_eq!(Cell::parse("MLAC Traxx"), Cell::Text("MLAC Traxx".into()));
    }
}
