//! Positioned words, filled rectangles and clip rectangles from PDF pages.
//!
//! Words and fills come from `pdf-extract`'s rendering callbacks; clip rectangles (which
//! InStat uses for goal markers and power-play bands) are read from the raw content stream.

use pdf_extract::content::Content;
use pdf_extract::{
    ColorSpace, Document, MediaBox, Object, OutputDev, OutputError, Path, PathOp, Transform,
};

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

/// An axis-aligned rectangle in top-left page coordinates (points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Rect {
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    #[must_use]
    pub const fn height(&self) -> f64 {
        self.bottom - self.top
    }

    #[must_use]
    pub const fn center_x(&self) -> f64 {
        f64::midpoint(self.x0, self.x1)
    }
}

#[derive(Debug, Clone)]
pub struct Page {
    /// 1-based page number.
    pub number: u32,
    pub words: Vec<Word>,
    /// Filled rectangles, e.g. shift bars.
    pub fills: Vec<Rect>,
    /// Clipping rectangles, e.g. goal markers and special-teams bands.
    pub clips: Vec<Rect>,
}

impl Page {
    /// Page text in drawing order, one space between words; used for page classification.
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

/// Parses the PDF bytes and returns every page's words, fills and clips.
pub fn extract_pages(bytes: &[u8]) -> Result<Vec<Page>, Error> {
    let doc = Document::load_mem(bytes).map_err(|e| Error::Pdf(e.to_string()))?;
    let mut collector = WordCollector::default();
    pdf_extract::output_doc(&doc, &mut collector).map_err(|e| Error::Pdf(format!("{e:?}")))?;
    let mut pages = collector.pages;
    for (number, page_id) in doc.get_pages() {
        let content = doc
            .get_page_content(page_id)
            .map_err(|e| Error::Pdf(e.to_string()))?;
        let operations = Content::decode(&content)
            .map_err(|e| Error::Pdf(e.to_string()))?
            .operations;
        let height = page_height(&doc, page_id);
        if let Some(page) = pages.iter_mut().find(|p| p.number == number) {
            page.clips = clip_rects(&operations, height);
        }
    }
    Ok(pages)
}

fn page_height(doc: &Document, page_id: pdf_extract::ObjectId) -> f64 {
    const A4_LANDSCAPE_HEIGHT: f64 = 595.276;
    doc.get_dictionary(page_id)
        .ok()
        .and_then(|dict| dict.get(b"MediaBox").ok())
        .and_then(|obj| obj.as_array().ok())
        .and_then(|values| {
            let lly = values.get(1)?.as_float().ok()?;
            let ury = values.get(3)?.as_float().ok()?;
            Some(f64::from(ury - lly))
        })
        .unwrap_or(A4_LANDSCAPE_HEIGHT)
}

#[derive(Debug, Clone, Copy)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// `self` applied after `inner` (PDF `cm` semantics: new CTM = inner × current).
    fn then(self, inner: Self) -> Self {
        Self {
            a: inner.a * self.a + inner.b * self.c,
            b: inner.a * self.b + inner.b * self.d,
            c: inner.c * self.a + inner.d * self.c,
            d: inner.c * self.b + inner.d * self.d,
            e: inner.e * self.a + inner.f * self.c + self.e,
            f: inner.e * self.b + inner.f * self.d + self.f,
        }
    }

    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

fn operand_floats(operands: &[Object]) -> Option<Vec<f64>> {
    operands
        .iter()
        .map(|o| o.as_float().ok().map(f64::from))
        .collect()
}

fn clip_rects(operations: &[pdf_extract::content::Operation], page_height: f64) -> Vec<Rect> {
    let mut ctm = Matrix::IDENTITY;
    let mut stack = Vec::new();
    let mut path_rects: Vec<Rect> = Vec::new();
    let mut clips = Vec::new();
    for op in operations {
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or(Matrix::IDENTITY),
            "cm" => {
                if let Some([a, b, c, d, e, f]) = operand_floats(&op.operands).as_deref() {
                    let inner = Matrix {
                        a: *a,
                        b: *b,
                        c: *c,
                        d: *d,
                        e: *e,
                        f: *f,
                    };
                    ctm = ctm.then(inner);
                }
            }
            "re" => {
                if let Some([x, y, w, h]) = operand_floats(&op.operands).as_deref() {
                    let p = ctm.apply(*x, *y);
                    let q = ctm.apply(x + w, y + h);
                    path_rects.push(Rect {
                        x0: p.0.min(q.0),
                        x1: p.0.max(q.0),
                        top: page_height - p.1.max(q.1),
                        bottom: page_height - p.1.min(q.1),
                    });
                }
            }
            "W" | "W*" => clips.append(&mut path_rects),
            "n" | "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => path_rects.clear(),
            _ => {}
        }
    }
    clips
}

fn apply(t: &Transform, x: f64, y: f64) -> (f64, f64) {
    (
        t.m11 * x + t.m21 * y + t.m31,
        t.m12 * x + t.m22 * y + t.m32,
    )
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
    current_fills: Vec<Rect>,
    pending: Option<(String, Glyph)>,
    /// Set when a new text-showing operator starts; glyphs inside one operator only split on spaces.
    new_text_run: bool,
}

/// Gap (as a fraction of font size) above which two text runs are separate words.
const RUN_GAP_RATIO: f64 = 0.01;
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
        self.current_fills.clear();
        self.pages.push(Page {
            number: page_num,
            words: Vec::new(),
            fills: Vec::new(),
            clips: Vec::new(),
        });
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), OutputError> {
        self.flush_pending();
        let words = std::mem::take(&mut self.current_words);
        let fills = std::mem::take(&mut self.current_fills);
        if let Some(page) = self.pages.last_mut() {
            page.words = words;
            page.fills = fills;
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

    fn fill(
        &mut self,
        ctm: &Transform,
        _colorspace: &ColorSpace,
        _color: &[f64],
        path: &Path,
    ) -> Result<(), OutputError> {
        for op in &path.ops {
            if let PathOp::Rect(x, y, width, height) = *op {
                let a = apply(ctm, x, y);
                let b = apply(ctm, x + width, y + height);
                self.current_fills.push(Rect {
                    x0: a.0.min(b.0),
                    x1: a.0.max(b.0),
                    top: self.page_height - a.1.max(b.1),
                    bottom: self.page_height - a.1.min(b.1),
                });
            }
        }
        Ok(())
    }
}
