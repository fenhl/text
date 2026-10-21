use {
    std::{
        borrow::Cow,
        collections::hash_map::{
            self,
            HashMap,
        },
        sync::Arc,
    },
    fontdue::{
        Font,
        layout::{
            GlyphRasterConfig,
            HorizontalAlign,
            Layout,
            LayoutSettings,
            TextStyle,
            VerticalAlign,
        },
    },
    itertools::Itertools as _,
    noisy_float::prelude::*,
    nonempty_collections::NEVec,
    tiny_skia::*,
    crate::*,
};

#[must_use]
pub struct Builder<B: Bounds> {
    fonts: NEVec<Arc<Font>>,
    text: Cow<'static, str>,
    bounds: B,
    color: ColorU8,
    size: f32,
    halign: HorizontalAlign,
    valign: VerticalAlign,
}

impl Builder<DefaultBounds> {
    pub fn new(font: Arc<Font>, text: impl Into<Cow<'static, str>>) -> Self {
        Self {
            fonts: NEVec::new(font),
            text: text.into(),
            bounds: DefaultBounds,
            color: Color::WHITE.to_color_u8(),
            size: DEFAULT_SIZE,
            halign: HorizontalAlign::Center,
            valign: VerticalAlign::Middle,
        }
    }

    pub fn bounds_inner(self, bounds: Rect) -> Builder<InnerBounds> {
        Builder {
            fonts: self.fonts,
            text: self.text,
            color: self.color,
            size: self.size,
            halign: self.halign,
            valign: self.valign,
            bounds: InnerBounds(bounds),
        }
    }

    pub fn bounds_outer(self, bounds: Rect) -> Builder<OuterBounds> {
        Builder {
            fonts: self.fonts,
            text: self.text,
            color: self.color,
            size: self.size,
            halign: self.halign,
            valign: self.valign,
            bounds: OuterBounds(bounds),
        }
    }

    pub fn build(self, layout: Layout, [canvas_width, canvas_height]: [f32; 2]) -> Result<TextBox, Error> {
        let inner_bounds = Rect::from_xywh(0.0, 0.0, canvas_width, canvas_height).ok_or(Error::Rect)?.inset(self.size / 2.0, self.size / 2.0).ok_or(Error::Inset)?;
        Ok(self.bounds_inner(inner_bounds).build(layout))
    }
}

impl<B: Bounds> Builder<B> {
    pub fn fallback_font(mut self, font: Arc<Font>) -> Self {
        self.fonts.push(font);
        self
    }

    pub fn color(self, color: impl IntoColor) -> Self {
        Self {
            color: color.into_color_u8(),
            ..self
        }
    }

    pub fn size(self, size: f32) -> Self {
        Self { size, ..self }
    }

    pub fn halign(self, halign: HorizontalAlign) -> Self {
        Self { halign, ..self }
    }

    pub fn valign(self, valign: VerticalAlign) -> Self {
        Self { valign, ..self }
    }
}

impl Builder<InnerBounds> {
    pub fn build(self, mut layout: Layout) -> TextBox {
        layout.reset(&LayoutSettings {
            x: self.bounds.0.x(),
            y: self.bounds.0.y(),
            max_width: Some(self.bounds.0.width()),
            max_height: Some(self.bounds.0.height()),
            horizontal_align: self.halign,
            vertical_align: self.valign,
            ..LayoutSettings::default()
        });
        for (font_idx, segment) in &self.text.chars().chunk_by(|c| self.fonts.iter().position(|font| font.has_glyph(*c)).unwrap_or_default()) {
            layout.append(self.fonts.as_ref(), &TextStyle::new(&segment.collect::<String>(), self.size, font_idx));
        }
        TextBox {
            fonts: self.fonts,
            color: self.color,
            size: self.size,
            halign: self.halign,
            valign: self.valign,
            inner_bounds: self.bounds.0,
            layout,
        }
    }
}

impl Builder<OuterBounds> {
    fn bounds_inner(self, bounds: Rect) -> Builder<InnerBounds> {
        Builder {
            fonts: self.fonts,
            text: self.text,
            color: self.color,
            size: self.size,
            halign: self.halign,
            valign: self.valign,
            bounds: InnerBounds(bounds),
        }
    }

    pub fn build(self, layout: Layout) -> Result<TextBox, Error> {
        let inner_bounds = self.bounds.0.inset(self.size / 2.0, self.size / 2.0).ok_or(Error::Inset)?;
        Ok(self.bounds_inner(inner_bounds).build(layout))
    }
}

#[must_use]
pub struct TextBox {
    fonts: NEVec<Arc<Font>>,
    layout: Layout,
    inner_bounds: Rect,
    color: ColorU8,
    size: f32,
    halign: HorizontalAlign,
    valign: VerticalAlign,
}

impl TextBox {
    #[must_use]
    pub fn rect_inner(&self) -> Result<Rect, Error> {
        let width = self.layout.lines()
            .and_then(|lines| lines.iter().map(|line| r32(self.inner_bounds.width() - line.padding)).max())
            .unwrap_or_default()
            .raw();
        let height = self.layout.height();
        Rect::from_xywh(
            self.inner_bounds.x() + match self.halign {
                HorizontalAlign::Left => 0.0,
                HorizontalAlign::Center => (self.inner_bounds.width() - width) / 2.0,
                HorizontalAlign::Right => self.inner_bounds.width() - width,
            },
            self.inner_bounds.y() + match self.valign {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Middle => (self.inner_bounds.height() - height) / 2.0,
                VerticalAlign::Bottom => self.inner_bounds.height() - height,
            },
            width,
            height,
        ).ok_or(Error::Rect)
    }

    #[must_use]
    pub fn rect_outer(&self) -> Result<Rect, Error> {
        self.rect_inner()?.outset(self.size / 2.0, self.size / 2.0).ok_or(Error::Outset)
    }

    pub fn draw_ref(&self, mut canvas: PixmapMut<'_>, glyph_cache: &mut HashMap<(GlyphRasterConfig, [u8; 4]), Pixmap>) -> Result<(), Error> {
        for glyph in self.layout.glyphs() {
            if glyph.width > 0 && glyph.height > 0 {
                match glyph_cache.entry((glyph.key, [self.color.red(), self.color.green(), self.color.blue(), self.color.alpha()])) {
                    hash_map::Entry::Occupied(entry) => canvas.draw_pixmap(0, 0, entry.get().as_ref(), &PixmapPaint::default(), Transform::from_translate(glyph.x, glyph.y), None),
                    hash_map::Entry::Vacant(entry) => {
                        let (_, data) = self.fonts[glyph.font_index].rasterize_config(glyph.key);
                        let mut glyph_canvas = Pixmap::new(glyph.width as u32, glyph.height as u32).ok_or(Error::GlyphPixmap)?;
                        for (alpha, pixel) in data.into_iter().zip_eq(glyph_canvas.pixels_mut()) {
                            *pixel = ColorU8::from_rgba(self.color.red(), self.color.green(), self.color.blue(), (u16::from(self.color.alpha()) * u16::from(alpha) / 255) as u8).premultiply();
                        }
                        canvas.draw_pixmap(0, 0, glyph_canvas.as_ref(), &PixmapPaint::default(), Transform::from_translate(glyph.x, glyph.y), None);
                        entry.insert(glyph_canvas);
                    }
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn draw(self, canvas: PixmapMut<'_>, glyph_cache: &mut HashMap<(GlyphRasterConfig, [u8; 4]), Pixmap>) -> Result<Layout, Error> {
        self.draw_ref(canvas, glyph_cache)?;
        Ok(self.layout)
    }

    #[must_use]
    pub fn into_layout(self) -> Layout {
        self.layout
    }
}
