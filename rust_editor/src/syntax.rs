
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SyntaxStyle {
    Default = 0,
    Keyword = 1,
    Comment = 2,
    String = 3,
    Number = 4,
    Type = 5,
}

impl SyntaxStyle {
    pub fn from_u32(id: u32) -> Self {
        match id {
            1 => Self::Keyword,
            2 => Self::Comment,
            3 => Self::String,
            4 => Self::Number,
            5 => Self::Type,
            _ => Self::Default,
        }
    }
}
