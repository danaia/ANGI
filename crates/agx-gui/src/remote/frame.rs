//! Captured window pixels and change detection.

use slint::{Rgba8Pixel, SharedPixelBuffer};

/// An RGBA8 (4 bytes per pixel, row-major, no padding) image of the whole window.
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pixels: SharedPixelBuffer<Rgba8Pixel>,
}

impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.pixels.as_bytes() == other.pixels.as_bytes()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Frame {
    pub fn from_snapshot(mut pixels: SharedPixelBuffer<Rgba8Pixel>) -> Self {
        // The software display has no GPU colour pass, so night light scales the captured pixels.
        if crate::night_light::frame_gain().is_some() {
            crate::night_light::filter_pixels(pixels.make_mut_bytes());
        }
        Self {
            width: pixels.width() as usize,
            height: pixels.height() as usize,
            pixels,
        }
    }

    pub fn bounds(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: self.width,
            h: self.height,
        }
    }

    pub fn row(&self, y: usize) -> &[u8] {
        let stride = self.width * 4;
        &self.pixels.as_bytes()[y * stride..(y + 1) * stride]
    }

    #[cfg(test)]
    pub fn from_rgba(width: u32, height: u32, bytes: &[u8]) -> Self {
        Self::from_snapshot(SharedPixelBuffer::clone_from_slice(bytes, width, height))
    }

    #[cfg(test)]
    pub fn rgba_mut(&mut self) -> &mut [u8] {
        self.pixels.make_mut_bytes()
    }
}

/// Smallest rectangle containing every pixel that differs, or `None` when the frames
/// are identical. Both frames must have the same size.
pub fn changed_bounds(old: &Frame, new: &Frame) -> Option<Rect> {
    debug_assert_eq!((old.width, old.height), (new.width, new.height));
    let (mut top, mut bottom) = (None, 0);
    let (mut left, mut right) = (usize::MAX, 0);
    for y in 0..new.height {
        let (a, b) = (old.row(y), new.row(y));
        if a == b {
            continue;
        }
        top.get_or_insert(y);
        bottom = y;
        let first = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .position(|(p, q)| p != q)
            .unwrap_or(0);
        let last = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .rposition(|(p, q)| p != q)
            .unwrap_or(0);
        left = left.min(first);
        right = right.max(last);
    }
    top.map(|top| Rect {
        x: left,
        y: top,
        w: right - left + 1,
        h: bottom - top + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(width: u32, height: u32) -> Frame {
        Frame::from_snapshot(SharedPixelBuffer::new(width, height))
    }

    fn paint(frame: &mut Frame, x: usize, y: usize) {
        let index = (y * frame.width + x) * 4;
        frame.rgba_mut()[index] = 255;
    }

    #[test]
    fn identical_frames_have_no_changes() {
        assert_eq!(changed_bounds(&blank(8, 4), &blank(8, 4)), None);
    }

    #[test]
    fn bounds_cover_every_changed_pixel() {
        let old = blank(10, 6);
        let mut new = old.clone();
        paint(&mut new, 7, 1);
        paint(&mut new, 2, 4);
        assert_eq!(
            changed_bounds(&old, &new),
            Some(Rect {
                x: 2,
                y: 1,
                w: 6,
                h: 4
            })
        );
    }

    #[test]
    fn a_single_pixel_is_a_one_pixel_rectangle() {
        let old = blank(5, 5);
        let mut new = old.clone();
        paint(&mut new, 0, 0);
        assert_eq!(
            changed_bounds(&old, &new),
            Some(Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1
            })
        );
    }
}
