use crate::config::schema::BorderStyle;

/// ui box drawing glyph set.
#[derive(Debug, Clone, Copy)]
pub struct Glyphs {
    pub h: char,
    pub v: char,
    pub top_left: char,
    pub top_right: char,
    pub bottom_left: char,
    pub bottom_right: char,
    pub ellipsis: char,
    pub block: char,
}

impl Glyphs {
    pub const UNICODE: Self = Self {
        h: '─',
        v: '│',
        top_left: '╭',
        top_right: '╮',
        bottom_left: '╰',
        bottom_right: '╯',
        ellipsis: '…',
        block: '█',
    };

    pub const PLAIN: Self = Self {
        h: '─',
        v: '│',
        top_left: '┌',
        top_right: '┐',
        bottom_left: '└',
        bottom_right: '┘',
        ellipsis: '…',
        block: '█',
    };

    pub const THICK: Self = Self {
        h: '━',
        v: '┃',
        top_left: '┏',
        top_right: '┓',
        bottom_left: '┗',
        bottom_right: '┛',
        ellipsis: '…',
        block: '█',
    };

    pub const DOUBLE: Self = Self {
        h: '═',
        v: '║',
        top_left: '╔',
        top_right: '╗',
        bottom_left: '╚',
        bottom_right: '╝',
        ellipsis: '…',
        block: '█',
    };

    pub const ASCII: Self = Self {
        h: '-',
        v: '|',
        top_left: '+',
        top_right: '+',
        bottom_left: '+',
        bottom_right: '+',
        ellipsis: '.',
        block: '#',
    };

    /// returns the glyph set for a configured border style.
    pub fn styled(ascii: bool, style: BorderStyle) -> Self {
        if ascii {
            return Self::ASCII;
        }
        match style {
            BorderStyle::Plain => Self::PLAIN,
            BorderStyle::Thick => Self::THICK,
            BorderStyle::Double => Self::DOUBLE,
            BorderStyle::Rounded | BorderStyle::None => Self::UNICODE,
        }
    }
}
