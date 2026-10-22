//! `TEXT.RSC` — reading retail text records (M15, REQ-L99-047 · NPC
//! captions).
//!
//! **Scope (hard boundary):** M15 exposed raw record reading
//! (`record_raw`); M16 adds the tokenization of format codes
//! (`0xFB` position, `0xFC` justify-left, `0xFD` justify-center,
//! `0xFF` subrecord separator, `0xF6` end of page), the render with a
//! stateful cursor and the book reader (`BOK*.TXT`, same token stream
//! without a table header). REQ-L99-048.
//!
//! **Token semantics (route A of the M16 §2.1 spec):** retail content is
//! tokenized as-is (`TextFile.ReadTokens`), **never** interpreted as DFU's
//! text quest DSL (`Message.LoadMessage`), which would lose `0xFB`/`0xFF`.
//! `0xFB` and `0xF9` consume **1** parameter byte (form `FB <x>`, measured
//! in 20/20 occurrences of `TEXT.RSC`; UESP's 4 B form does not apply to
//! this file). `0xFA` (FontColor) **is not implemented**: 0 occurrences as
//! a token in `TEXT.RSC` (spec H2).
//!
//! **Render:** justifiers affect the text already emitted since the last
//! cut, so the render is a stateful cursor (`PositionPrefix` fixes the
//! column of the following text) and not a per-line `split`.
//!
//! **Format (executable spec `DFU:Assets/Scripts/API/TextFile.cs`, MIT):**
//!
//! ```text
//! u16 TextRecordHeaderLength                 // TextFile.cs ReadHeader
//! (u16 TextRecordId, u32 Offset) × count     // TextRecordHeaderElement
//! count = TextRecordHeaderLength / 6 - 1     // TextFile.cs RecordCount
//! record = bytes from Offset up to 0xFE      // GetRecordLength (0xFE inclusive)
//! ```
//!
//! Empirical verification against the owner's retail copy (2026-10-20,
//! `DISK:TEXT.RSC 353 393 B sha256=cdbf1d8c…`): `TextRecordHeaderLength =
//! 8454` ⇒ `count = 8454/6 - 1 = 1408`; the table occupies `[2, 8450)` and the
//! first declared offset is `8456`; all offsets are strictly
//! increasing and `< 353 393`; `id 0` returns `STRENGTH`, `id 1`
//! `INTELLIGENCE`, `id 2` `WILLPOWER`, `id 3` `AGILITY` (character sheet).
//! The 4 bytes `FF FF 71 64` of `[8450, 8454)` fall outside both the table and
//! the first record: DFU's spec does not read them (0 invented interpretation).
//!
//! **Custody:** 0 retail bytes in git; the file is read at runtime from the
//! user's ARENA2 (Mode A, INV-L99-6).

/// Header: `u16 TextRecordHeaderLength`.
pub const TEXT_RSC_HEADER_LEN: usize = 2;
/// `TextRecordHeaderElement`: `u16 id` + `u32 offset`.
pub const TEXT_RSC_ENTRY_LEN: usize = 6;
/// `Formatting.EndOfRecord` (`TextFile.cs` enum `Formatting`).
pub const TEXT_RSC_END_OF_RECORD: u8 = 0xFE;
/// `Formatting.EndOfPage` — end of page in books/letters.
pub const TEXT_RSC_END_OF_PAGE: u8 = 0xF6;
/// `Formatting.SubrecordSeparator` — separates variants of the same record.
pub const TEXT_RSC_SUBRECORD_SEPARATOR: u8 = 0xFF;
/// `Formatting.JustifyLeft`.
pub const TEXT_RSC_JUSTIFY_LEFT: u8 = 0xFC;
/// `Formatting.JustifyCenter`.
pub const TEXT_RSC_JUSTIFY_CENTER: u8 = 0xFD;
/// `Formatting.PositionPrefix` (consumes **1** parameter byte).
pub const TEXT_RSC_POSITION_PREFIX: u8 = 0xFB;
/// `Formatting.FontPrefix` (consumes **1** parameter byte).
pub const TEXT_RSC_FONT_PREFIX: u8 = 0xF9;
/// `Formatting.FirstCharacter` — first printable character of `Text`.
pub const TEXT_RSC_FIRST_CHARACTER: u8 = 0x20;
/// `Formatting.LastCharacter` — last printable character of `Text`.
pub const TEXT_RSC_LAST_CHARACTER: u8 = 0x7F;
/// Book title: 64 B NUL-padded field before the token stream.
pub const BOOK_TITLE_LEN: usize = 64;
/// Book author: 64 B NUL-padded field after the title.
pub const BOOK_AUTHOR_LEN: usize = 64;
/// Book header (title + author; the rest are tokenized pages).
pub const BOOK_HEADER_LEN: usize = BOOK_TITLE_LEN + BOOK_AUTHOR_LEN;

/// Parse/read error (0 panic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextRscError {
    /// File shorter than `u16` + one entry.
    TooShort {
        /// Received bytes.
        len: usize,
        /// Minimum bytes.
        need: usize,
    },
    /// `TextRecordHeaderLength` is not a multiple of 6 (formula `count = L/6-1`).
    HeaderLengthNotMultiple {
        /// Declared length.
        len: u16,
    },
    /// The header table runs past the end of the file.
    HeaderOutOfBounds {
        /// Declared records.
        count: usize,
        /// Required bytes.
        need: usize,
        /// File bytes.
        len: usize,
    },
    /// A record `Offset` runs past the end of the file.
    RecordOutOfBounds {
        /// Index in the table.
        index: usize,
        /// Declared offset.
        offset: u32,
        /// File bytes.
        len: usize,
    },
    /// A record has no `0xFE` before the end of the file.
    MissingTerminator {
        /// Index in the table.
        index: usize,
    },
    /// `id` does not exist in the file table.
    UnknownRecord {
        /// `TextRecordId` looked up.
        id: u16,
    },
    /// Table index out of range.
    UnknownRecordIndex {
        /// Requested index.
        index: usize,
        /// Records in the table.
        count: usize,
    },
    /// Tokenization start position outside the buffer.
    StartOutOfBounds {
        /// Requested position.
        start: usize,
        /// Buffer bytes.
        len: usize,
    },
    /// A prefix with a parameter was left without its parameter byte at the end of the buffer.
    MissingParameter {
        /// Byte of the token that required the parameter.
        token: u8,
        /// Offset where the parameter was missing.
        offset: usize,
    },
}

impl std::fmt::Display for TextRscError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need } => {
                write!(f, "TEXT.RSC too short ({len} < {need} bytes)")
            }
            Self::HeaderLengthNotMultiple { len } => write!(
                f,
                "TextRecordHeaderLength {len} is not a multiple of 6 (count = L/6 - 1)"
            ),
            Self::HeaderOutOfBounds { count, need, len } => write!(
                f,
                "table of {count} headers needs {need} B and the file has {len} B"
            ),
            Self::RecordOutOfBounds {
                index,
                offset,
                len,
            } => write!(
                f,
                "record {index}: offset {offset} outside the file of {len} B"
            ),
            Self::MissingTerminator { index } => {
                write!(f, "record {index}: no 0xFE terminator before EOF")
            }
            Self::UnknownRecord { id } => write!(f, "record id {id} does not exist"),
            Self::UnknownRecordIndex { index, count } => {
                write!(f, "record index {index} outside the table ({count} records)")
            }
            Self::StartOutOfBounds { start, len } => {
                write!(f, "tokenization position {start} outside the buffer of {len} B")
            }
            Self::MissingParameter { token, offset } => write!(
                f,
                "token 0x{token:02X} at {offset}: missing parameter byte (EOF)"
            ),
        }
    }
}

impl std::error::Error for TextRscError {}

/// Header of a record (`TextRecordHeaderElement`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRecordHeader {
    /// `TextRecordId`.
    pub id: u16,
    /// Absolute `Offset` of the record.
    pub offset: u32,
}

/// Parsed `TEXT.RSC` (headers; the bytes are retained by the caller, same
/// as `TextureArchive` in `texture.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRsc {
    /// Declared `TextRecordHeaderLength`.
    pub header_length: u16,
    /// Headers in file order.
    pub records: Vec<TextRecordHeader>,
}

impl TextRsc {
    /// Parses the header and validates that every record has a `0xFE` terminator.
    pub fn parse(file: &[u8]) -> Result<Self, TextRscError> {
        if file.len() < TEXT_RSC_HEADER_LEN + TEXT_RSC_ENTRY_LEN {
            return Err(TextRscError::TooShort {
                len: file.len(),
                need: TEXT_RSC_HEADER_LEN + TEXT_RSC_ENTRY_LEN,
            });
        }
        let header_length = u16::from_le_bytes([file[0], file[1]]);
        if !(header_length as usize).is_multiple_of(TEXT_RSC_ENTRY_LEN) {
            return Err(TextRscError::HeaderLengthNotMultiple {
                len: header_length,
            });
        }
        let count = header_length as usize / TEXT_RSC_ENTRY_LEN - 1;
        let need = TEXT_RSC_HEADER_LEN + count * TEXT_RSC_ENTRY_LEN;
        if need > file.len() {
            return Err(TextRscError::HeaderOutOfBounds {
                count,
                need,
                len: file.len(),
            });
        }

        let mut records = Vec::with_capacity(count);
        for index in 0..count {
            let at = TEXT_RSC_HEADER_LEN + index * TEXT_RSC_ENTRY_LEN;
            let id = u16::from_le_bytes([file[at], file[at + 1]]);
            let offset = u32::from_le_bytes([file[at + 2], file[at + 3], file[at + 4], file[at + 5]]);
            if offset as usize >= file.len() {
                return Err(TextRscError::RecordOutOfBounds {
                    index,
                    offset,
                    len: file.len(),
                });
            }
            if !file[offset as usize..].contains(&TEXT_RSC_END_OF_RECORD) {
                return Err(TextRscError::MissingTerminator { index });
            }
            records.push(TextRecordHeader { id, offset });
        }
        Ok(Self {
            header_length,
            records,
        })
    }

    /// Number of records (DFU's `RecordCount`).
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Index of the record with `id`, if it exists (`IdToIndex`).
    pub fn index_of(&self, id: u16) -> Option<usize> {
        self.records.iter().position(|record| record.id == id)
    }

    /// Raw bytes of a record, **with** the trailing `0xFE`
    /// (`GetBytesById` — "all special bytes intact").
    pub fn record_raw<'a>(&self, file: &'a [u8], id: u16) -> Option<&'a [u8]> {
        self.index_of(id)
            .and_then(|index| self.record_raw_by_index(file, index))
            .filter(|bytes| !bytes.is_empty())
    }

    /// Raw bytes by table index.
    pub fn record_raw_by_index<'a>(&self, file: &'a [u8], index: usize) -> Option<&'a [u8]> {
        let start = self.records.get(index)?.offset as usize;
        if start >= file.len() {
            return None;
        }
        let tail = &file[start..];
        let end = tail.iter().position(|&b| b == TEXT_RSC_END_OF_RECORD)?;
        Some(&tail[..=end])
    }

    /// Plain text of a record: printable bytes up to the trailing `0xFE`.
    ///
    /// Returns `None` if the record does not exist or if it contains format
    /// codes (tokenization is M16): that way a caption can never come out
    /// polluted with control bytes.
    pub fn record_ascii<'a>(&self, file: &'a [u8], id: u16) -> Option<&'a str> {
        let raw = self.record_raw(file, id)?;
        let body = raw.strip_suffix(&[TEXT_RSC_END_OF_RECORD])?;
        if body.is_empty() || !body.iter().all(|b| (0x20..=0x7E).contains(b)) {
            return None;
        }
        std::str::from_utf8(body).ok()
    }
}

/// Format token of a retail text stream (`TextFile.Token`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenFormat {
    /// `Formatting.Text` — printable ASCII text (0x20…=0x7F).
    Text,
    /// `Formatting.NewLineOffset` (0x00) — line break.
    NewLine,
    /// `Formatting.SameLineOffset` (0x01).
    SameLine,
    /// `Formatting.PullPreceeding` (0x02).
    PullPreceding,
    /// `Formatting.EndOfPage` (0xF6) — end of page in books.
    EndOfPage,
    /// `Formatting.InputCursorPositioner` (0xF8).
    InputCursorPositioner,
    /// `Formatting.FontPrefix` (0xF9) — 1 parameter byte.
    FontPrefix,
    /// `Formatting.PositionPrefix` (0xFB) — 1 parameter byte (X column).
    PositionPrefix,
    /// `Formatting.JustifyLeft` (0xFC) — justifies the preceding text.
    JustifyLeft,
    /// `Formatting.JustifyCenter` (0xFD) — centres the preceding text.
    JustifyCenter,
    /// `Formatting.SubrecordSeparator` (0xFF) — separates variants.
    SubrecordSeparator,
    /// `Formatting.EndOfRecord` (0xFE).
    EndOfRecord,
    /// Control byte with no semantics declared in DFU (`0xFA` included).
    Other(u8),
}

impl TokenFormat {
    /// Maps a control byte to its token (DFU `Formatting` identity).
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => Self::NewLine,
            0x01 => Self::SameLine,
            0x02 => Self::PullPreceding,
            TEXT_RSC_END_OF_PAGE => Self::EndOfPage,
            0xF8 => Self::InputCursorPositioner,
            TEXT_RSC_FONT_PREFIX => Self::FontPrefix,
            TEXT_RSC_POSITION_PREFIX => Self::PositionPrefix,
            TEXT_RSC_JUSTIFY_LEFT => Self::JustifyLeft,
            TEXT_RSC_JUSTIFY_CENTER => Self::JustifyCenter,
            TEXT_RSC_SUBRECORD_SEPARATOR => Self::SubrecordSeparator,
            TEXT_RSC_END_OF_RECORD => Self::EndOfRecord,
            other => Self::Other(other),
        }
    }

    /// Is this the text token?
    pub fn is_text(self) -> bool {
        matches!(self, Self::Text)
    }
}

/// Token of a retail text stream: format + text + parameter (`x`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Token semantics.
    pub format: TokenFormat,
    /// Text (only for [`TokenFormat::Text`]).
    pub text: String,
    /// 1 B parameter (`FontPrefix`/`PositionPrefix`).
    pub parameter: Option<u8>,
}

impl Token {
    /// Text token.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            format: TokenFormat::Text,
            text: text.into(),
            parameter: None,
        }
    }

    /// Format token without parameter.
    pub fn formatting(format: TokenFormat) -> Self {
        Self {
            format,
            text: String::new(),
            parameter: None,
        }
    }

    /// Format token with a 1 B parameter.
    pub fn with_parameter(format: TokenFormat, parameter: u8) -> Self {
        Self {
            format,
            text: String::new(),
            parameter: Some(parameter),
        }
    }
}

/// Alignment of a rendered line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    /// `JustifyLeft` (or text without an explicit justifier).
    Left,
    /// `JustifyCenter`.
    Center,
}

/// Rendered line: full text (positions already materialized) + alignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLine {
    /// Line text, with the column padding of `PositionPrefix`.
    pub text: String,
    /// Alignment of the block preceding the justifier.
    pub justify: Justify,
}

/// Tokenizes a text stream up to `end_token` (exclusive).
///
/// Replica of `TextFile.ReadTokens` (`DFU:API/TextFile.cs:421-443`):
/// `Text` groups runs of `0x20..=0x7F`; `FontPrefix`/`PositionPrefix`
/// consume exactly 1 parameter B; the remaining control bytes are
/// format tokens without a parameter (`0xFA` included, no semantics).
pub fn read_tokens(
    file: &[u8],
    start: usize,
    end_token: u8,
) -> Result<Vec<Token>, TextRscError> {
    Ok(read_tokens_until(file, start, end_token)?.0)
}

/// Like [`read_tokens`] but also returns the position of the terminator
/// (or `file.len()` if the stream ends at EOF).
pub fn read_tokens_until(
    file: &[u8],
    start: usize,
    end_token: u8,
) -> Result<(Vec<Token>, usize), TextRscError> {
    if start > file.len() {
        return Err(TextRscError::StartOutOfBounds {
            start,
            len: file.len(),
        });
    }
    let mut tokens = Vec::new();
    let mut index = start;
    while index < file.len() {
        let byte = file[index];
        if byte == end_token {
            return Ok((tokens, index));
        }
        if (TEXT_RSC_FIRST_CHARACTER..=TEXT_RSC_LAST_CHARACTER).contains(&byte) {
            let begin = index;
            while index < file.len()
                && (TEXT_RSC_FIRST_CHARACTER..=TEXT_RSC_LAST_CHARACTER).contains(&file[index])
            {
                index += 1;
            }
            tokens.push(Token {
                format: TokenFormat::Text,
                text: String::from_utf8_lossy(&file[begin..index]).into_owned(),
                parameter: None,
            });
            continue;
        }
        let format = TokenFormat::from_byte(byte);
        index += 1;
        let parameter = match format {
            TokenFormat::FontPrefix => {
                let value = *file
                    .get(index)
                    .ok_or(TextRscError::MissingParameter {
                        token: byte,
                        offset: index,
                    })?;
                index += 1;
                Some(value)
            }
            TokenFormat::PositionPrefix => {
                // DFU's `PeekByte` returns null at the end of the buffer ⇒ x = 0.
                let value = file.get(index).copied();
                if value.is_some() {
                    index += 1;
                }
                value
            }
            _ => None,
        };
        tokens.push(Token {
            format,
            text: String::new(),
            parameter,
        });
    }
    Ok((tokens, index))
}

/// Renders tokens into lines with a stateful cursor.
///
/// `JustifyLeft`/`JustifyCenter` close the line with the text already emitted
/// (semantics of `Quest.cs:789-810`); `PositionPrefix(x)` pads with
/// spaces up to column `x` before the following text; `0xFF` closes the
/// line and separates a variant.
pub fn render_tokens(tokens: &[Token]) -> Vec<RenderedLine> {
    fn flush(lines: &mut Vec<RenderedLine>, line: &mut String, justify: Justify) {
        if !line.is_empty() {
            lines.push(RenderedLine {
                text: std::mem::take(line),
                justify,
            });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();
    let mut column: Option<u8> = None;
    for token in tokens {
        match token.format {
            TokenFormat::Text => {
                if let Some(target) = column.take() {
                    let width = line.chars().count();
                    let target = target as usize;
                    if target > width {
                        line.extend(std::iter::repeat_n(' ', target - width));
                    }
                }
                line.push_str(&token.text);
            }
            TokenFormat::PositionPrefix => column = token.parameter,
            TokenFormat::JustifyLeft => flush(&mut lines, &mut line, Justify::Left),
            TokenFormat::JustifyCenter => flush(&mut lines, &mut line, Justify::Center),
            TokenFormat::NewLine => flush(&mut lines, &mut line, Justify::Left),
            TokenFormat::SubrecordSeparator => flush(&mut lines, &mut line, Justify::Left),
            TokenFormat::EndOfRecord | TokenFormat::EndOfPage => {
                flush(&mut lines, &mut line, Justify::Left);
            }
            TokenFormat::SameLine
            | TokenFormat::PullPreceding
            | TokenFormat::InputCursorPositioner
            | TokenFormat::FontPrefix
            | TokenFormat::Other(_) => {}
        }
    }
    flush(&mut lines, &mut line, Justify::Left);
    lines
}

impl TextRsc {
    /// `0xFFFF` after the header table (terminator of the header-list).
    pub fn header_terminator(&self, file: &[u8]) -> Option<u16> {
        let at = TEXT_RSC_HEADER_LEN + self.records.len() * TEXT_RSC_ENTRY_LEN;
        let bytes = file.get(at..at + 2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// `u32` after the terminator: declared file size (measured, not
    /// documented in UESP nor in DFU).
    pub fn declared_file_size(&self, file: &[u8]) -> Option<u32> {
        let at = TEXT_RSC_HEADER_LEN + self.records.len() * TEXT_RSC_ENTRY_LEN + 2;
        let bytes = file.get(at..at + 4)?;
        Some(u32::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))
    }

    /// Length of record `index`, `0xFE` inclusive (scan, **not** an
    /// offset difference: 12 pairs of consecutive ids share an offset).
    pub fn record_len(&self, file: &[u8], index: usize) -> Option<usize> {
        let start = self.records.get(index)?.offset as usize;
        let tail = file.get(start..)?;
        tail.iter()
            .position(|&b| b == TEXT_RSC_END_OF_RECORD)
            .map(|end| end + 1)
    }

    /// Tokens of a record by table index.
    pub fn record_tokens_by_index(
        &self,
        file: &[u8],
        index: usize,
    ) -> Result<Vec<Token>, TextRscError> {
        let record = self
            .records
            .get(index)
            .ok_or(TextRscError::UnknownRecordIndex {
                index,
                count: self.records.len(),
            })?;
        read_tokens(file, record.offset as usize, TEXT_RSC_END_OF_RECORD)
    }

    /// Tokens of a record by `TextRecordId`.
    pub fn record_tokens(&self, file: &[u8], id: u16) -> Result<Vec<Token>, TextRscError> {
        let index = self.index_of(id).ok_or(TextRscError::UnknownRecord { id })?;
        self.record_tokens_by_index(file, index)
    }

    /// Variants of the record split by `0xFF` (always ≥1).
    pub fn record_variants(&self, file: &[u8], id: u16) -> Result<Vec<Vec<Token>>, TextRscError> {
        let tokens = self.record_tokens(file, id)?;
        let mut variants = vec![Vec::new()];
        for token in tokens {
            if token.format == TokenFormat::SubrecordSeparator {
                variants.push(Vec::new());
            } else if let Some(current) = variants.last_mut() {
                current.push(token);
            }
        }
        Ok(variants)
    }

    /// Rendered lines of a record (format semantics preserved).
    pub fn record_lines(
        &self,
        file: &[u8],
        id: u16,
    ) -> Result<Vec<RenderedLine>, TextRscError> {
        Ok(render_tokens(&self.record_tokens(file, id)?))
    }

    /// Plain text (route A): rendered lines joined by `\n`.
    pub fn record_text(&self, file: &[u8], id: u16) -> Result<String, TextRscError> {
        let index = self.index_of(id).ok_or(TextRscError::UnknownRecord { id })?;
        self.record_text_by_index(file, index)
    }

    /// Plain text by table index (needed when the table repeats ids,
    /// as in 17 retail `.QRC` files).
    pub fn record_text_by_index(
        &self,
        file: &[u8],
        index: usize,
    ) -> Result<String, TextRscError> {
        let lines = render_tokens(&self.record_tokens_by_index(file, index)?);
        Ok(lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

/// Retail book (`BOK*.TXT`): title + 64 B NUL-padded author and tokenized
/// pages separated by `0xF6` (`Formatting.EndOfPage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    /// Title (64 B field, up to the first NUL).
    pub title: String,
    /// Author (64 B field, up to the first NUL).
    pub author: String,
    /// Tokenized pages in file order.
    pub pages: Vec<Vec<Token>>,
}

impl Book {
    /// Parses a book: 128 B header + token stream with end of page.
    pub fn parse(file: &[u8]) -> Result<Self, TextRscError> {
        if file.len() < BOOK_HEADER_LEN {
            return Err(TextRscError::TooShort {
                len: file.len(),
                need: BOOK_HEADER_LEN,
            });
        }
        let title = c_string(&file[..BOOK_TITLE_LEN]);
        let author = c_string(&file[BOOK_TITLE_LEN..BOOK_HEADER_LEN]);
        let mut pages = Vec::new();
        let mut position = BOOK_HEADER_LEN;
        while position < file.len() {
            let (tokens, stop) = read_tokens_until(file, position, TEXT_RSC_END_OF_PAGE)?;
            pages.push(tokens);
            if stop >= file.len() {
                break;
            }
            position = stop + 1;
        }
        Ok(Self {
            title,
            author,
            pages,
        })
    }

    /// Rendered lines of a page.
    pub fn page_lines(&self, index: usize) -> Option<Vec<RenderedLine>> {
        self.pages.get(index).map(|tokens| render_tokens(tokens))
    }

    /// Plain text of a page.
    pub fn page_text(&self, index: usize) -> Option<String> {
        self.page_lines(index).map(|lines| {
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    /// Plain text of the whole book (pages joined by `\n\n`).
    pub fn plain_text(&self) -> String {
        (0..self.pages.len())
            .filter_map(|index| self.page_text(index))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// NUL-terminated string from a fixed-size field.
fn c_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic `TEXT.RSC` from the `(id, text)` records.
    fn synthetic(records: &[(u16, &[u8])]) -> Vec<u8> {
        let count = records.len();
        let header_length = ((count + 1) * TEXT_RSC_ENTRY_LEN) as u16;
        let mut file = Vec::new();
        file.extend_from_slice(&header_length.to_le_bytes());
        let data_start = TEXT_RSC_HEADER_LEN + count * TEXT_RSC_ENTRY_LEN;
        let mut cursor = data_start;
        for (id, text) in records {
            file.extend_from_slice(&id.to_le_bytes());
            file.extend_from_slice(&(cursor as u32).to_le_bytes());
            cursor += text.len() + 1;
        }
        for (_, text) in records {
            file.extend_from_slice(text);
            file.push(TEXT_RSC_END_OF_RECORD);
        }
        file
    }

    /// Table + `record_raw` (0xFE inclusive) + `record_ascii`.
    #[test]
    fn parses_table_and_returns_raw_records() {
        let file = synthetic(&[(0, b"STRENGTH"), (1, b"INTELLIGENCE")]);
        let rsc = TextRsc::parse(&file).expect("valid fixture");
        assert_eq!(rsc.record_count(), 2);
        assert_eq!(rsc.header_length as usize, 3 * TEXT_RSC_ENTRY_LEN);
        assert_eq!(rsc.record_raw(&file, 0), Some(&b"STRENGTH\xfe"[..]));
        assert_eq!(rsc.record_ascii(&file, 1), Some("INTELLIGENCE"));
        assert_eq!(rsc.record_raw(&file, 99), None);
    }

    /// A record with control bytes does not produce `record_ascii` (M16
    /// tokenizes them); the raw bytes are still preserved byte by byte.
    #[test]
    fn control_bytes_are_not_ascii_captions() {
        let file = synthetic(&[(4653, b"mage"), (7, b"a\xfdb")]);
        let rsc = TextRsc::parse(&file).unwrap();
        assert_eq!(rsc.record_ascii(&file, 4653), Some("mage"));
        assert_eq!(rsc.record_ascii(&file, 7), None);
        assert_eq!(rsc.record_raw(&file, 7), Some(&b"a\xfdb\xfe"[..]));
    }

    /// Invalid headers → typed errors (0 panic).
    #[test]
    fn malformed_headers_are_rejected() {
        assert!(matches!(
            TextRsc::parse(&[0u8; 4]),
            Err(TextRscError::TooShort { .. })
        ));
        let mut bad = synthetic(&[(1, b"x")]);
        bad[0] = 7;
        assert_eq!(
            TextRsc::parse(&bad),
            Err(TextRscError::HeaderLengthNotMultiple { len: 7 })
        );
        let mut short = synthetic(&[(1, b"x"), (2, b"y")]);
        short.truncate(TEXT_RSC_HEADER_LEN + TEXT_RSC_ENTRY_LEN);
        assert!(matches!(
            TextRsc::parse(&short),
            Err(TextRscError::HeaderOutOfBounds { .. })
        ));
    }

    /// A record without `0xFE` is an error, not a silently truncated text.
    #[test]
    fn missing_terminator_is_reported() {
        let mut file = synthetic(&[(1, b"x")]);
        let last = file.len() - 1;
        file[last] = b'?';
        assert_eq!(
            TextRsc::parse(&file),
            Err(TextRscError::MissingTerminator { index: 0 })
        );
    }

    /// `0xFB` consumes **1** parameter byte (retail form `FB <x>`).
    #[test]
    fn position_prefix_consumes_exactly_one_byte() {
        let file = synthetic(&[(1, b" \xfb\x14%dam \xfb\x28modifier")]);
        let rsc = TextRsc::parse(&file).unwrap();
        let tokens = rsc.record_tokens(&file, 1).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::text(" "),
                Token::with_parameter(TokenFormat::PositionPrefix, 20),
                Token::text("%dam "),
                Token::with_parameter(TokenFormat::PositionPrefix, 40),
                Token::text("modifier"),
            ]
        );
    }

    /// `0xF9` consumes 1 B and a prefix without a parameter at EOF is a typed error.
    #[test]
    fn font_prefix_requires_its_parameter() {
        let tokens = read_tokens(b"a\xf9\x02b", 0, TEXT_RSC_END_OF_RECORD).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::text("a"),
                Token::with_parameter(TokenFormat::FontPrefix, 2),
                Token::text("b"),
            ]
        );
        assert_eq!(
            read_tokens(b"a\xf9", 0, TEXT_RSC_END_OF_RECORD),
            Err(TextRscError::MissingParameter {
                token: 0xF9,
                offset: 2
            })
        );
    }

    /// `0xFA` is not implemented: it stays as `Other` without consuming a parameter.
    #[test]
    fn font_colour_is_not_interpreted() {
        let tokens = read_tokens(b"\xfa\x03x", 0, TEXT_RSC_END_OF_RECORD).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::formatting(TokenFormat::Other(0xFA)),
                Token::formatting(TokenFormat::Other(0x03)),
                Token::text("x"),
            ]
        );
    }

    /// Variants separated by `0xFF`, with the separator excluded.
    #[test]
    fn subrecord_separator_splits_variants() {
        let file = synthetic(&[(9, b"one\xfftwo\xffthree")]);
        let rsc = TextRsc::parse(&file).unwrap();
        let variants = rsc.record_variants(&file, 9).unwrap();
        assert_eq!(variants.len(), 3);
        assert_eq!(variants[0], vec![Token::text("one")]);
        assert_eq!(variants[2], vec![Token::text("three")]);
    }

    /// Render with cursor: the justifier closes the preceding line and
    /// `PositionPrefix` pads up to the column.
    #[test]
    fn render_applies_justify_and_position() {
        let file = synthetic(&[(
            5,
            b"STRENGTH\xfd Strength governs\xfc a\xfb\x06b\xfc",
        )]);
        let rsc = TextRsc::parse(&file).unwrap();
        let lines = rsc.record_lines(&file, 5).unwrap();
        assert_eq!(
            lines,
            vec![
                RenderedLine {
                    text: "STRENGTH".to_string(),
                    justify: Justify::Center,
                },
                RenderedLine {
                    text: " Strength governs".to_string(),
                    justify: Justify::Left,
                },
                RenderedLine {
                    text: " a    b".to_string(),
                    justify: Justify::Left,
                },
            ]
        );
    }

    /// Book: 64 B title + 64 B author + pages terminated by `0xF6`.
    #[test]
    fn book_parses_title_author_and_pages() {
        let mut file = Vec::new();
        let mut title = [0u8; BOOK_TITLE_LEN];
        title[..5].copy_from_slice(b"Title");
        let mut author = [0u8; BOOK_AUTHOR_LEN];
        author[..6].copy_from_slice(b"Author");
        file.extend_from_slice(&title);
        file.extend_from_slice(&author);
        file.extend_from_slice(b"page one\xf6page two\xf6");
        let book = Book::parse(&file).unwrap();
        assert_eq!(book.title, "Title");
        assert_eq!(book.author, "Author");
        assert_eq!(book.pages.len(), 2);
        assert_eq!(book.page_text(0).unwrap(), "page one");
        assert_eq!(book.page_text(1).unwrap(), "page two");
        assert_eq!(book.plain_text(), "page one\n\npage two");
        assert!(matches!(
            Book::parse(&file[..BOOK_HEADER_LEN - 1]),
            Err(TextRscError::TooShort { .. })
        ));
    }

    /// Header: `0xFFFF` and declared size after the table.
    #[test]
    fn header_terminator_and_declared_size() {
        // Real layout: H (12 = 2 entries) in 2 B, table (1 record) in 2..8,
        // `0xFFFF` in 8, declared size in 10 and data in 14.
        let mut file = Vec::new();
        file.extend_from_slice(&12u16.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&14u32.to_le_bytes());
        file.extend_from_slice(&0xFFFFu16.to_le_bytes());
        file.extend_from_slice(&16u32.to_le_bytes());
        file.push(b'x');
        file.push(TEXT_RSC_END_OF_RECORD);
        let rsc = TextRsc::parse(&file).unwrap();
        assert_eq!(rsc.header_terminator(&file), Some(0xFFFF));
        assert_eq!(rsc.declared_file_size(&file), Some(file.len() as u32));
    }
}
