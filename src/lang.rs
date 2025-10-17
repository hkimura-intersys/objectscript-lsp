// use std::collections::BTreeMap;
//
// /// Represents a position in LSP (Language Server Protocol) format
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub struct LspPosition {
//     pub line: u32,
//     pub character: u32, // UTF-16 code units
// }
//
// /// Represents a byte range in tree-sitter format
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub struct ByteRange {
//     pub start: usize,
//     pub end: usize,
// }
//
// /// Converts between UTF-8 byte offsets (tree-sitter) and UTF-16 code unit positions (LSP)
// pub struct PositionConverter {
//     text: String,
//     line_offsets: Vec<LineOffset>,
// }
//
// #[derive(Debug, Clone)]
// struct LineOffset {
//     utf8_start: usize,
//     utf16_line_start: usize,
//     // Map from UTF-8 byte offset (relative to line start) to UTF-16 code units
//     utf8_to_utf16: BTreeMap<usize, usize>,
//     // Map from UTF-16 code units (relative to line start) to UTF-8 bytes
//     utf16_to_utf8: BTreeMap<usize, usize>,
// }
// impl PositionConverter {
//     /// Creates a new position converter for the given text
//     pub fn new(text: String) -> Self {
//         let mut line_offsets = Vec::new();
//         let mut utf8_pos = 0;
//         let mut utf16_total = 0;
//
//         for line in text.split_inclusive('\n') {
//             let mut utf8_to_utf16 = BTreeMap::new();
//             let mut utf16_to_utf8 = BTreeMap::new();
//
//             let mut line_utf8 = 0;
//             let mut line_utf16 = 0;
//
//             // Build character mappings for this line
//             for ch in line.chars() {
//                 utf8_to_utf16.insert(line_utf8, line_utf16);
//                 utf16_to_utf8.insert(line_utf16, line_utf8);
//
//                 line_utf8 += ch.len_utf8();
//                 line_utf16 += ch.len_utf16();
//             }
//
//             // Add end position
//             utf8_to_utf16.insert(line_utf8, line_utf16);
//             utf16_to_utf8.insert(line_utf16, line_utf8);
//
//             line_offsets.push(LineOffset {
//                 utf8_start: utf8_pos,
//                 utf16_line_start: utf16_total,
//                 utf8_to_utf16,
//                 utf16_to_utf8,
//             });
//
//             utf8_pos += line.len();
//             utf16_total += line_utf16;
//         }
//
//         // Handle case where text doesn't end with newline
//         if !text.ends_with('\n') && !text.is_empty() {
//             // The last "line" was already processed above
//         } else if text.is_empty() {
//             // Empty document should have at least one line
//             line_offsets.push(LineOffset {
//                 utf8_start: 0,
//                 utf16_line_start: 0,
//                 utf8_to_utf16: BTreeMap::from([(0, 0)]),
//                 utf16_to_utf8: BTreeMap::from([(0, 0)]),
//             });
//         }
//
//         Self { text, line_offsets }
//     }
//
//     /// Converts a tree-sitter byte offset to an LSP position
//     pub fn byte_to_lsp(&self, byte_offset: usize) -> LspPosition {
//         // Find the line containing this byte offset
//         let line_idx = self.line_offsets
//             .binary_search_by_key(&byte_offset, |line| line.utf8_start)
//             .unwrap_or_else(|i| i.saturating_sub(1));
//
//         let line_offset = &self.line_offsets[line_idx];
//         let byte_in_line = byte_offset.saturating_sub(line_offset.utf8_start);
//
//         // Convert byte offset within line to UTF-16 code units
//         let character = line_offset.utf8_to_utf16
//             .range(..=byte_in_line)
//             .next_back()
//             .map(|(_, &utf16)| utf16)
//             .unwrap_or(0);
//
//         LspPosition {
//             line: line_idx as u32,
//             character: character as u32,
//         }
//     }
//
//     /// Converts an LSP position to a tree-sitter byte offset
//     pub fn lsp_to_byte(&self, position: LspPosition) -> usize {
//         let line_idx = position.line as usize;
//
//         if line_idx >= self.line_offsets.len() {
//             // Position is beyond the document
//             return self.text.len();
//         }
//
//         let line_offset = &self.line_offsets[line_idx];
//
//         // Convert UTF-16 code units to bytes within the line
//         let byte_in_line = line_offset.utf16_to_utf8
//             .range(..=position.character as usize)
//             .next_back()
//             .map(|(_, &utf8)| utf8)
//             .unwrap_or(0);
//
//         line_offset.utf8_start + byte_in_line
//     }
//
//     /// Converts an LSP range to a tree-sitter byte range
//     pub fn lsp_range_to_byte_range(&self, start: LspPosition, end: LspPosition) -> ByteRange {
//         ByteRange {
//             start: self.lsp_to_byte(start),
//             end: self.lsp_to_byte(end),
//         }
//     }
//
//     /// Converts a tree-sitter byte range to an LSP range
//     pub fn byte_range_to_lsp_range(&self, range: ByteRange) -> (LspPosition, LspPosition) {
//         (
//             self.byte_to_lsp(range.start),
//             self.byte_to_lsp(range.end),
//         )
//     }
//
//     /// Gets the total number of lines in the document
//     pub fn line_count(&self) -> usize {
//         self.line_offsets.len()
//     }
//
//     /// Gets the text of a specific line (0-indexed)
//     pub fn line_text(&self, line: usize) -> Option<&str> {
//         if line >= self.line_offsets.len() {
//             return None;
//         }
//
//         let start = self.line_offsets[line].utf8_start;
//         let end = if line + 1 < self.line_offsets.len() {
//             self.line_offsets[line + 1].utf8_start
//         } else {
//             self.text.len()
//         };
//
//         Some(&self.text[start..end])
//     }
// }
//
// /// Incremental position converter that can be updated efficiently
// pub struct IncrementalPositionConverter {
//     converter: PositionConverter,
// }
//
// impl IncrementalPositionConverter {
//     pub fn new(text: String) -> Self {
//         Self {
//             converter: PositionConverter::new(text),
//         }
//     }
//
//     /// Updates the converter with a text change
//     pub fn update(&mut self, start: LspPosition, end: LspPosition, new_text: String) {
//         // For simplicity, rebuild the entire converter
//         // In a production implementation, you'd want to update only affected lines
//         let start_byte = self.converter.lsp_to_byte(start);
//         let end_byte = self.converter.lsp_to_byte(end);
//
//         let mut text = self.converter.text.clone();
//         text.replace_range(start_byte..end_byte, &new_text);
//
//         self.converter = PositionConverter::new(text);
//     }
//
//     pub fn byte_to_lsp(&self, byte_offset: usize) -> LspPosition {
//         self.converter.byte_to_lsp(byte_offset)
//     }
//
//     pub fn lsp_to_byte(&self, position: LspPosition) -> usize {
//         self.converter.lsp_to_byte(position)
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_ascii_only() {
//         let text = "hello\nworld".to_string();
//         let converter = PositionConverter::new(text);
//
//         // First line
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 0 }), 0);
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 5 }), 5);
//
//         // Second line
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 1, character: 0 }), 6);
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 1, character: 5 }), 11);
//
//         // Reverse conversion
//         assert_eq!(converter.byte_to_lsp(0), LspPosition { line: 0, character: 0 });
//         assert_eq!(converter.byte_to_lsp(5), LspPosition { line: 0, character: 5 });
//         assert_eq!(converter.byte_to_lsp(6), LspPosition { line: 1, character: 0 });
//         assert_eq!(converter.byte_to_lsp(11), LspPosition { line: 1, character: 5 });
//     }
//
//     #[test]
//     fn test_emoji() {
//         let text = "Hello 😀 World".to_string();
//         let converter = PositionConverter::new(text);
//
//         // Before emoji
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 6 }), 6);
//
//         // After emoji (emoji is 2 UTF-16 code units, 4 UTF-8 bytes)
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 8 }), 10);
//
//         // End of string
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 14 }), 16);
//
//         // Reverse
//         assert_eq!(converter.byte_to_lsp(6), LspPosition { line: 0, character: 6 });
//         assert_eq!(converter.byte_to_lsp(10), LspPosition { line: 0, character: 8 });
//         assert_eq!(converter.byte_to_lsp(16), LspPosition { line: 0, character: 14 });
//     }
//
//     #[test]
//     fn test_mixed_unicode() {
//         // Mix of ASCII, 2-byte UTF-8 (é), 3-byte UTF-8 (中), and 4-byte UTF-8 (😀)
//         let text = "Café 中文 😀".to_string();
//         let converter = PositionConverter::new(text);
//
//         // "Café" - é is 2 bytes in UTF-8, 1 code unit in UTF-16
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 4 }), 5); // After 'é'
//
//         // Space after "Café"
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 5 }), 6);
//
//         // "中文" - each character is 3 bytes in UTF-8, 1 code unit in UTF-16
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 7 }), 12); // After '中文'
//
//         // Emoji - 4 bytes in UTF-8, 2 code units in UTF-16
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 10 }), 17); // After emoji
//     }
//
//     #[test]
//     fn test_empty_lines() {
//         let text = "line1\n\nline3".to_string();
//         let converter = PositionConverter::new(text);
//
//         // First line
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 0, character: 0 }), 0);
//
//         // Empty second line
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 1, character: 0 }), 6);
//
//         // Third line
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 2, character: 0 }), 7);
//         assert_eq!(converter.lsp_to_byte(LspPosition { line: 2, character: 5 }), 12);
//     }
//
//     #[test]
//     fn test_range_conversion() {
//         let text = "Hello 😀\nWorld".to_string();
//         let converter = PositionConverter::new(text);
//
//         let start = LspPosition { line: 0, character: 6 };
//         let end = LspPosition { line: 1, character: 5 };
//
//         let byte_range = converter.lsp_range_to_byte_range(start, end);
//         assert_eq!(byte_range.start, 6);  // After "Hello "
//         assert_eq!(byte_range.end, 16);   // End of "World"
//
//         let (lsp_start, lsp_end) = converter.byte_range_to_lsp_range(byte_range);
//         assert_eq!(lsp_start, start);
//         assert_eq!(lsp_end, end);
//     }
// }