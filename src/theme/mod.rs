//! Semantic themes and centralized component styling.

mod definition;
pub(crate) mod styles;

pub(crate) use definition::{
    Preset, ResolvedTheme, Tokens, Typography, WritingFont, WritingMeasure,
};

pub(crate) const BUNDLED_WRITING_FONTS: [&[u8]; 8] = [
    include_bytes!("../../assets/fonts/Newsreader-Regular.ttf"),
    include_bytes!("../../assets/fonts/Newsreader-Bold.ttf"),
    include_bytes!("../../assets/fonts/Newsreader-Italic.ttf"),
    include_bytes!("../../assets/fonts/Newsreader-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/source-sans-3/SourceSans3-Regular.ttf"),
    include_bytes!("../../assets/fonts/source-sans-3/SourceSans3-Bold.ttf"),
    include_bytes!("../../assets/fonts/source-sans-3/SourceSans3-It.ttf"),
    include_bytes!("../../assets/fonts/source-sans-3/SourceSans3-BoldIt.ttf"),
];
