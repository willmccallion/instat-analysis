//! Positioned-word extraction from PDF pages, built on `pdf-extract`'s glyph callbacks.

use pdf_extract::{Document, MediaBox, OutputDev, OutputError, Transform};

use crate::error::Error;

/// A run of non-space glyphs on one baseline, in top-left page coordinates (points).
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Word {
    #[must_use]
    pub const fn center_x(&self) -> f64 {
        f64::midpoint(self.x0, self.x1)
    }

    #[must_use]
    pub const fn center_y(&self) -> f64 {
        f64::midpoint(self.top, self.bottom)
    }
}

#[derive(Debug, Clone)]
pub struct Page {
    /// 1-based page number.
    pub number: u32,
    pub words: Vec<Word>,
}

impl Page {
    /// Page text in reading order, one space between words; used for page classification.
    #[must_use]
    pub fn joined_text(&self) -> String {
        let mut text = String::new();
        for word in &self.words {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&word.text);
        }
        text
    }
}

/// Parses the PDF bytes and returns every page's words.
pub fn extract_pages(bytes: &[u8]) -> Result<Vec<Page>, Error> {
    let doc = Document::load_mem(bytes).map_err(|e| Error::Pdf(e.to_string()))?;
    let mut collector = WordCollector::default();
    pdf_extract::output_doc(&doc, &mut collector).map_err(|e| Error::Pdf(format!("{e:?}")))?;
    Ok(collector.pages)
}

#[derive(Debug, Clone, Copy)]
struct Glyph {
    x0: f64,
    x1: f64,
    /// Where the next glyph would start, including character spacing.
    advance_end: f64,
    baseline: f64,
    size: f64,
}

#[derive(Default)]
struct WordCollector {
    pages: Vec<Page>,
    page_height: f64,
    current_words: Vec<Word>,
    pending: Option<(String, Glyph)>,
    /// Set when a new text-showing operator starts; glyphs inside one operator only split on spaces.
    new_text_run: bool,
}

/// Gap (as a fraction of font size) above which two text runs are separate words.
const RUN_GAP_RATIO: f64 = 0.1;
const BASELINE_TOLERANCE: f64 = 0.5;

impl WordCollector {
    fn flush_pending(&mut self) {
        let Some((text, glyph)) = self.pending.take() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        let top = self.page_height - glyph.baseline - glyph.size * 0.8;
        let bottom = self.page_height - glyph.baseline + glyph.size * 0.2;
        self.current_words.push(Word {
            text,
            x0: glyph.x0,
            x1: glyph.x1,
            top,
            bottom,
        });
    }

    fn continues_pending(&self, next: &Glyph) -> bool {
        self.pending.as_ref().is_some_and(|(_, prev)| {
            let same_line = (prev.baseline - next.baseline).abs() <= BASELINE_TOLERANCE;
            if !same_line {
                return false;
            }
            if !self.new_text_run {
                return true;
            }
            let gap = next.x0 - prev.advance_end;
            gap.abs() <= next.size * RUN_GAP_RATIO
        })
    }
}

impl OutputDev for WordCollector {
    fn begin_page(
        &mut self,
        page_num: u32,
        media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.page_height = media_box.ury - media_box.lly;
        self.current_words.clear();
        self.pages.push(Page {
            number: page_num,
            words: Vec::new(),
        });
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), OutputError> {
        self.flush_pending();
        let words = std::mem::take(&mut self.current_words);
        if let Some(page) = self.pages.last_mut() {
            page.words = words;
        }
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        spacing: f64,
        font_size: f64,
        text: &str,
    ) -> Result<(), OutputError> {
        let scale_x = trm.m11.hypot(trm.m12);
        let scale_y = trm.m21.hypot(trm.m22);
        let size = font_size * scale_y;
        let glyph = Glyph {
            x0: trm.m31,
            x1: trm.m31 + width * font_size * scale_x,
            advance_end: trm.m31 + (width * font_size + spacing) * scale_x,
            baseline: trm.m32,
            size,
        };
        if text.chars().all(char::is_whitespace) {
            self.flush_pending();
            self.new_text_run = false;
            return Ok(());
        }
        if self.continues_pending(&glyph) {
            if let Some((word_text, prev)) = self.pending.as_mut() {
                word_text.push_str(text);
                prev.x1 = glyph.x1.max(prev.x1);
                prev.advance_end = glyph.advance_end;
                prev.size = prev.size.max(glyph.size);
            }
        } else {
            self.flush_pending();
            self.pending = Some((text.to_owned(), glyph));
        }
        self.new_text_run = false;
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), OutputError> {
        self.new_text_run = true;
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}
