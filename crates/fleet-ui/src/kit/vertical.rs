use std::fmt::Write as _;

use gpui::{
    App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px, svg,
};
use ttf_parser::{Face, OutlineBuilder};

use crate::assets::PLEX_SANS_SEMIBOLD;
use crate::theme::{ActiveTheme, LABEL_TRACKING, TEXT_LABEL};

#[derive(IntoElement)]
pub struct VerticalLabel {
    text: SharedString,
}

impl VerticalLabel {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl RenderOnce for VerticalLabel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = cx.palette().mute;
        let size = TEXT_LABEL.as_f32();
        let text = self.text.to_uppercase();
        let rotated = Face::parse(PLEX_SANS_SEMIBOLD, 0)
            .ok()
            .map(|face| rotate(&face, &text, size, size * LABEL_TRACKING));
        match rotated {
            Some(label) => svg()
                .data(label.svg.as_bytes())
                .flex_none()
                .w(px(label.width))
                .h(px(label.height))
                .text_color(color)
                .into_any_element(),
            None => div().text_color(color).child(text).into_any_element(),
        }
    }
}

#[derive(Debug)]
struct Rotated {
    svg: String,
    width: f32,
    height: f32,
}

struct PathData {
    d: String,
    scale: f32,
    origin: f32,
    baseline: f32,
}

impl PathData {
    fn point(&mut self, x: f32, y: f32) {
        let _ = write!(
            self.d,
            "{} {} ",
            self.origin + x * self.scale,
            self.baseline - y * self.scale
        );
    }
}

impl OutlineBuilder for PathData {
    fn move_to(&mut self, x: f32, y: f32) {
        self.d.push('M');
        self.point(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.d.push('L');
        self.point(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.d.push('Q');
        self.point(x1, y1);
        self.point(x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.d.push('C');
        self.point(x1, y1);
        self.point(x2, y2);
        self.point(x, y);
    }

    fn close(&mut self) {
        self.d.push('Z');
    }
}

fn rotate(face: &Face, text: &str, size: f32, tracking: f32) -> Rotated {
    let scale = size / f32::from(face.units_per_em());
    let ascent = f32::from(face.ascender()) * scale;
    let line = f32::from(face.ascender()).abs() * scale + f32::from(face.descender()).abs() * scale;
    let mut path = PathData {
        d: String::new(),
        scale,
        origin: 0.,
        baseline: ascent,
    };
    let mut pen = 0.;
    let mut glyphs = 0usize;
    for glyph in text.chars().filter_map(|c| face.glyph_index(c)) {
        path.origin = pen;
        face.outline_glyph(glyph, &mut path);
        let advance = face
            .glyph_hor_advance(glyph)
            .map_or(0., |a| f32::from(a) * scale);
        pen += advance + tracking;
        glyphs += 1;
    }
    let length = if glyphs == 0 { 0. } else { pen - tracking };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{line}" height="{length}" viewBox="0 0 {line} {length}"><path transform="translate({line} 0) rotate(90)" d="{}"/></svg>"#,
        path.d
    );
    Rotated {
        svg,
        width: line,
        height: length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> Face<'static> {
        Face::parse(PLEX_SANS_SEMIBOLD, 0).unwrap_or_else(|error| panic!("bundled font: {error}"))
    }

    #[test]
    fn labels_run_down_the_strip_as_one_rotated_line() {
        let label = rotate(&face(), "CLUSTERS", 11., 0.77);
        assert!(label.height > label.width * 3.);
        assert!(label.width < 16.);
        assert!(label.svg.contains("rotate(90)"));
        assert!(label.svg.contains('M'));
    }

    #[test]
    fn tracking_adds_space_between_letters_only() {
        let tight = rotate(&face(), "FLEET", 11., 0.);
        let tracked = rotate(&face(), "FLEET", 11., 1.);
        assert!((tracked.height - tight.height - 4.).abs() < 0.01);
        assert!(rotate(&face(), "", 11., 1.).height.abs() < f32::EPSILON);
    }
}
