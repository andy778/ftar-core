//! Drawing preferences (OpenFTA's Edit > Preferences): pen, background and highlight
//! colours, a fill colour per event/gate type, and a monochrome mode for printing.
//!
//! Stored as a small `key = #rrggbb` text file; unknown or malformed lines are ignored,
//! so a damaged file never breaks start-up.

use std::path::{Path, PathBuf};

use ftar_core::EventType;

/// An sRGB colour.
pub type Rgb = [u8; 3];

/// Symbol groups that each have their own fill colour (OpenFTA's list, plus dormant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    Intermediate,
    Basic,
    Dormant,
    Undeveloped,
    External,
    Conditioning,
    And,
    Or,
    PriorityAnd,
    Xor,
    Inhibit,
    TransferIn,
    TransferOut,
}

impl Symbol {
    pub const ALL: [Symbol; 13] = [
        Symbol::Intermediate,
        Symbol::Basic,
        Symbol::Dormant,
        Symbol::Undeveloped,
        Symbol::External,
        Symbol::Conditioning,
        Symbol::And,
        Symbol::Or,
        Symbol::PriorityAnd,
        Symbol::Xor,
        Symbol::Inhibit,
        Symbol::TransferIn,
        Symbol::TransferOut,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Symbol::Intermediate => "intermediate",
            Symbol::Basic => "basic",
            Symbol::Dormant => "dormant",
            Symbol::Undeveloped => "undeveloped",
            Symbol::External => "external",
            Symbol::Conditioning => "conditioning",
            Symbol::And => "and",
            Symbol::Or => "or",
            Symbol::PriorityAnd => "priority_and",
            Symbol::Xor => "exclusive_or",
            Symbol::Inhibit => "inhibit",
            Symbol::TransferIn => "transfer_in",
            Symbol::TransferOut => "transfer_out",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Symbol::Intermediate => "Intermediate event",
            Symbol::Basic => "Basic event",
            Symbol::Dormant => "Dormant event",
            Symbol::Undeveloped => "Undeveloped event",
            Symbol::External => "External event",
            Symbol::Conditioning => "Conditioning event",
            Symbol::And => "AND gate",
            Symbol::Or => "OR gate",
            Symbol::PriorityAnd => "Priority AND gate",
            Symbol::Xor => "Exclusive OR gate",
            Symbol::Inhibit => "Inhibit gate",
            Symbol::TransferIn => "Transfer in",
            Symbol::TransferOut => "Transfer out",
        }
    }

    /// The group an event type is coloured by (`None`: not a drawn symbol).
    pub fn of(kind: EventType) -> Option<Symbol> {
        use EventType::*;
        Some(match kind {
            Basic => Symbol::Basic,
            Dormant => Symbol::Dormant,
            Undeveloped => Symbol::Undeveloped,
            External => Symbol::External,
            And => Symbol::And,
            Or => Symbol::Or,
            Xor => Symbol::Xor,
            PriorityAnd => Symbol::PriorityAnd,
            Inhibit => Symbol::Inhibit,
            Intermediate => Symbol::Intermediate,
            ConditionAnalysed | ConditionNotAnalysed => Symbol::Conditioning,
            TransferIn => Symbol::TransferIn,
            TransferOut => Symbol::TransferOut,
            Unknown => return None,
        })
    }

    /// OpenFTA's defaults (`Preferences.java`). OpenFTA has no colour for
    /// dormant events (it draws them as basic events), so they share the basic one.
    fn default_fill(self) -> Rgb {
        match self {
            Symbol::Intermediate => [250, 250, 200],
            Symbol::Basic | Symbol::Dormant | Symbol::Undeveloped | Symbol::External => {
                [210, 250, 200]
            }
            Symbol::Conditioning => [200, 245, 250],
            Symbol::And | Symbol::Or | Symbol::PriorityAnd | Symbol::Xor | Symbol::Inhibit => {
                [250, 200, 200]
            }
            Symbol::TransferIn | Symbol::TransferOut => [220, 200, 170],
        }
    }

    fn index(self) -> usize {
        Symbol::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// Colours and mode used by the canvas, image export and print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    /// Draw in black on white (fills white) regardless of the colours below.
    pub monochrome: bool,
    pub pen: Rgb,
    pub background: Rgb,
    pub highlight: Rgb,
    /// Fill per [`Symbol`], in [`Symbol::ALL`] order (prefer `fill_of` / `set_fill`).
    pub fills: [Rgb; 13],
}

impl Default for Preferences {
    fn default() -> Self {
        let mut fills = [[0u8; 3]; 13];
        for s in Symbol::ALL {
            fills[s.index()] = s.default_fill();
        }
        Self {
            monochrome: false,
            pen: [0, 0, 0],
            background: [0xff, 0xff, 0xff],
            highlight: [255, 50, 50],
            fills,
        }
    }
}

const WHITE: Rgb = [255, 255, 255];
const BLACK: Rgb = [0, 0, 0];

impl Preferences {
    /// Configured fill for a symbol group (ignores monochrome).
    pub fn fill_of(&self, s: Symbol) -> Rgb {
        self.fills[s.index()]
    }

    pub fn set_fill(&mut self, s: Symbol, c: Rgb) {
        self.fills[s.index()] = c;
    }

    /// Fill actually drawn for `kind`.
    pub fn fill(&self, kind: EventType) -> Rgb {
        if self.monochrome {
            return WHITE;
        }
        match Symbol::of(kind) {
            Some(s) => self.fills[s.index()],
            None => [0xdd, 0xdd, 0xdd],
        }
    }

    pub fn pen_colour(&self) -> Rgb {
        if self.monochrome {
            BLACK
        } else {
            self.pen
        }
    }

    pub fn background_colour(&self) -> Rgb {
        if self.monochrome {
            WHITE
        } else {
            self.background
        }
    }

    pub fn highlight_colour(&self) -> Rgb {
        if self.monochrome {
            BLACK
        } else {
            self.highlight
        }
    }

    /// Serialise as `key = value` lines.
    pub fn to_text(&self) -> String {
        let mut s = String::from("# ftar drawing preferences\n");
        s.push_str(&format!("monochrome = {}\n", self.monochrome));
        s.push_str(&format!("pen = {}\n", hex(self.pen)));
        s.push_str(&format!("background = {}\n", hex(self.background)));
        s.push_str(&format!("highlight = {}\n", hex(self.highlight)));
        for sym in Symbol::ALL {
            s.push_str(&format!(
                "fill.{} = {}\n",
                sym.key(),
                hex(self.fill_of(sym))
            ));
        }
        s
    }

    /// Parse; anything unreadable keeps its default.
    pub fn from_text(text: &str) -> Self {
        let mut p = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "monochrome" => {
                    if let Ok(b) = v.parse::<bool>() {
                        p.monochrome = b;
                    }
                }
                "pen" => set(&mut p.pen, v),
                "background" => set(&mut p.background, v),
                "highlight" => set(&mut p.highlight, v),
                _ => {
                    if let Some(sym) = k
                        .strip_prefix("fill.")
                        .and_then(|n| Symbol::ALL.into_iter().find(|s| s.key() == n))
                    {
                        let mut c = p.fill_of(sym);
                        set(&mut c, v);
                        p.set_fill(sym, c);
                    }
                }
            }
        }
        p
    }

    /// Read from `path`; a missing or unreadable file gives the defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::from_text(&t))
            .unwrap_or_default()
    }

    /// Write to `path`, creating parent folders.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_text())
    }

    /// Per-user location: `$XDG_CONFIG_HOME/ftar/preferences.txt`, `%APPDATA%\ftar\...`
    /// or `~/.config/ftar/...`. `None` when no home can be found.
    pub fn default_path() -> Option<PathBuf> {
        let nonempty = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
        let base = nonempty("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| nonempty("APPDATA").map(PathBuf::from))
            .or_else(|| nonempty("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("ftar").join("preferences.txt"))
    }
}

/// `#rrggbb` text of a colour.
pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Parse `#rrggbb` (the `#` is optional).
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

fn set(slot: &mut Rgb, v: &str) {
    if let Some(c) = parse_hex(v) {
        *slot = c;
    }
}

#[cfg(test)]
mod tests {
    /// OpenFTA's defaults, from `Preferences.java` (#69).
    #[test]
    fn defaults_are_openftas() {
        use EventType::*;
        let p = Preferences::default();
        assert_eq!(p.pen, [0, 0, 0]);
        assert_eq!(p.background, [255, 255, 255]);
        assert_eq!(p.highlight, [255, 50, 50]);
        assert_eq!(p.fill(Intermediate), [250, 250, 200]);
        for k in [Basic, Undeveloped, External, Dormant] {
            assert_eq!(p.fill(k), [210, 250, 200], "{k:?}");
        }
        for k in [ConditionAnalysed, ConditionNotAnalysed] {
            assert_eq!(p.fill(k), [200, 245, 250], "{k:?}");
        }
        for k in [And, Or, PriorityAnd, Xor, Inhibit] {
            assert_eq!(p.fill(k), [250, 200, 200], "{k:?}");
        }
        for k in [TransferIn, TransferOut] {
            assert_eq!(p.fill(k), [220, 200, 170], "{k:?}");
        }
    }

    use super::*;

    #[test]
    fn roundtrip_and_garbage() {
        let mut p = Preferences {
            monochrome: true,
            pen: [1, 2, 3],
            ..Preferences::default()
        };
        p.set_fill(Symbol::And, [9, 8, 7]);
        assert_eq!(Preferences::from_text(&p.to_text()), p);
        let d = Preferences::from_text(
            "pen = zzz\nfill.and = #12\n\u{e9}=\u{e9}\nfill.nope=#000000\n=\nfill.or=\u{e9}\u{e9}\u{e9}",
        );
        assert_eq!(d, Preferences::default());
        assert_eq!(parse_hex("\u{e9}\u{e9}\u{e9}"), None);
    }

    #[test]
    fn monochrome_overrides() {
        let p = Preferences {
            monochrome: true,
            ..Preferences::default()
        };
        assert_eq!(p.fill(EventType::Basic), WHITE);
        assert_eq!(p.pen_colour(), BLACK);
    }
}
