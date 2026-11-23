// AGX addition to the vendored Slint LinuxKMS backend (see AGX-PATCHES.md).
//
// Shows the mouse pointer on the display's hardware cursor plane instead of painting it into
// every frame. The pointer is then moved straight from the libinput event, with no render and
// no wait for the next vblank, which is how a compositor such as Weston keeps it responsive.

use std::cell::RefCell;
use std::collections::HashMap;

use drm::Device as _;
use drm::buffer::DrmFourcc;
use drm::control::{Device, PlaneType, crtc, dumbbuffer::DumbBuffer};
use i_slint_core::cursor::MouseCursorInner;
use i_slint_core::graphics::Image;
use i_slint_core::items::BuiltInMouseCursor;
use i_slint_core::slice::Slice;

use crate::drmoutput::SharedFd;

/// The cursor shapes with their own image. Every other cursor falls back to the arrow.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Shape {
    Arrow,
    Move,
    EastWest,
    NorthSouth,
    NorthWestSouthEast,
    NorthEastSouthWest,
}

impl Shape {
    fn svg(self) -> &'static [u8] {
        match self {
            Shape::Arrow => include_bytes!("mouse-pointer.svg"),
            Shape::Move => include_bytes!("cursors/move.svg"),
            Shape::EastWest => include_bytes!("cursors/ew-resize.svg"),
            Shape::NorthSouth => include_bytes!("cursors/ns-resize.svg"),
            Shape::NorthWestSouthEast => include_bytes!("cursors/nwse-resize.svg"),
            Shape::NorthEastSouthWest => include_bytes!("cursors/nesw-resize.svg"),
        }
    }

    /// The arrow points with its tip; the other shapes are centred on the pointer.
    fn hotspot(self, width: u32, height: u32) -> (u32, u32) {
        match self {
            Shape::Arrow => (0, 0),
            _ => (width / 2, height / 2),
        }
    }

    fn of(cursor: &MouseCursorInner) -> Option<Self> {
        let MouseCursorInner::BuiltIn(cursor) = cursor else {
            return Some(Shape::Arrow);
        };
        Some(match cursor {
            BuiltInMouseCursor::None => return None,
            BuiltInMouseCursor::Move => Shape::Move,
            BuiltInMouseCursor::ColResize
            | BuiltInMouseCursor::EwResize
            | BuiltInMouseCursor::EResize
            | BuiltInMouseCursor::WResize => Shape::EastWest,
            BuiltInMouseCursor::RowResize
            | BuiltInMouseCursor::NsResize
            | BuiltInMouseCursor::NResize
            | BuiltInMouseCursor::SResize => Shape::NorthSouth,
            BuiltInMouseCursor::NwseResize
            | BuiltInMouseCursor::NwResize
            | BuiltInMouseCursor::SeResize => Shape::NorthWestSouthEast,
            BuiltInMouseCursor::NeswResize
            | BuiltInMouseCursor::NeResize
            | BuiltInMouseCursor::SwResize => Shape::NorthEastSouthWest,
            _ => Shape::Arrow,
        })
    }
}

struct Loaded {
    buffer: DumbBuffer,
    hotspot: (i32, i32),
}

struct HardwareCursor {
    device: SharedFd,
    crtc: crtc::Handle,
    /// Width and height of the cursor plane's square buffers.
    size: u32,
    scale: u32,
    shapes: HashMap<Shape, Loaded>,
    shown: Option<Shape>,
    hidden: bool,
    /// The pointer stays hidden until the first movement, not in a corner at startup.
    moved: bool,
    wanted: Shape,
    position: (i32, i32),
    failed: bool,
}

thread_local! {
    // The libinput hook and the window adapter run on the event loop thread that built the backend.
    static CURSOR: RefCell<Option<HardwareCursor>> = const { RefCell::new(None) };
}

/// Prepares the hardware cursor for `crtc` when the display has a cursor plane.
pub(crate) fn install(device: &SharedFd, crtc: crtc::Handle) {
    if std::env::var_os("AGX_SOFTWARE_CURSOR").is_some()
        // Rotated output would need the image and position transformed to match.
        || std::env::var_os("SLINT_KMS_ROTATION").is_some()
    {
        return;
    }
    let has_cursor_plane = (|| {
        device.set_client_capability(drm::ClientCapability::UniversalPlanes, true).ok()?;
        let resources = device.resource_handles().ok()?;
        let index = resources.crtcs().iter().position(|handle| *handle == crtc)?;
        let planes = device.plane_handles().ok()?;
        planes.into_iter().find(|plane| {
            device.get_plane(*plane).is_ok_and(|info| {
                resources.filter_crtcs(info.possible_crtcs()).contains(&resources.crtcs()[index])
            }) && plane_is_cursor(device, *plane)
        })
    })();
    let Some(_) = has_cursor_plane else {
        eprintln!("agx: no hardware cursor plane; drawing the pointer in the frame");
        return;
    };
    let size = [drm::DriverCapability::CursorWidth, drm::DriverCapability::CursorHeight]
        .iter()
        .filter_map(|cap| device.get_driver_capability(*cap).ok())
        .min()
        .unwrap_or(64)
        .clamp(1, 256) as u32;
    let scale = std::env::var("SLINT_SCALE_FACTOR")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .map_or(1, |factor| factor.round().max(1.0) as u32)
        .min(size / 32)
        .max(1);
    eprintln!("agx: hardware cursor plane, {size}x{size} buffers at {scale}x");
    CURSOR.with(|cursor| {
        *cursor.borrow_mut() = Some(HardwareCursor {
            device: device.clone(),
            crtc,
            size,
            scale,
            shapes: HashMap::new(),
            shown: None,
            hidden: false,
            moved: false,
            wanted: Shape::Arrow,
            position: (0, 0),
            failed: false,
        });
    });
}

fn plane_is_cursor(device: &SharedFd, plane: drm::control::plane::Handle) -> bool {
    let Ok(properties) = device.get_properties(plane) else {
        return false;
    };
    let (handles, values) = properties.as_props_and_values();
    handles.iter().zip(values).any(|(handle, value)| {
        device.get_property(*handle).is_ok_and(|info| {
            info.name().to_bytes() == b"type" && *value == PlaneType::Cursor as u64
        })
    })
}

/// Whether the pointer is on the hardware plane, so the frame must not draw one.
pub fn available() -> bool {
    CURSOR.with(|cursor| cursor.borrow().as_ref().is_some_and(|cursor| !cursor.failed))
}

/// Shows `cursor` on the hardware plane. Returns whether the plane now handles it.
pub(crate) fn set(cursor: &MouseCursorInner) -> bool {
    CURSOR.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut().filter(|state| !state.failed) else {
            return false;
        };
        state.show(Shape::of(cursor));
        !state.failed
    })
}

/// Moves the pointer to `x`, `y` in physical output pixels.
pub fn move_to(x: i32, y: i32) {
    CURSOR.with(|state| {
        if let Some(state) = state.borrow_mut().as_mut().filter(|state| !state.failed) {
            state.move_to(x, y);
        }
    });
}

impl HardwareCursor {
    fn show(&mut self, shape: Option<Shape>) {
        let Some(shape) = shape else {
            self.hidden = true;
            if self.shown.take().is_some() {
                self.apply(None);
            }
            return;
        };
        self.hidden = false;
        self.wanted = shape;
        if !self.moved || self.shown == Some(shape) {
            return;
        }
        self.apply(Some(shape));
    }

    fn apply(&mut self, shape: Option<Shape>) {
        let result = match shape {
            None => self.device.set_cursor2::<DumbBuffer>(self.crtc, None, (0, 0)),
            Some(shape) => match self.load(shape) {
                Ok(()) => {
                    let loaded = &self.shapes[&shape];
                    self.device.set_cursor2(self.crtc, Some(&loaded.buffer), loaded.hotspot)
                }
                Err(error) => Err(error),
            },
        };
        match result {
            Ok(()) => {
                self.shown = shape;
                // The plane position is reset by the legacy ioctl on some drivers.
                self.update_position();
            }
            Err(error) => {
                eprintln!("agx: hardware cursor failed ({error}); drawing the pointer in the frame");
                self.failed = true;
            }
        }
    }

    fn load(&mut self, shape: Shape) -> std::io::Result<()> {
        if !self.shapes.contains_key(&shape) {
            let loaded = self.render(shape)?;
            self.shapes.insert(shape, loaded);
        }
        Ok(())
    }

    fn render(&self, shape: Shape) -> std::io::Result<Loaded> {
        let invalid = |message: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, message.to_owned());
        let image = i_slint_core::graphics::load_image_from_embedded_data(
            Slice::from_slice(shape.svg()),
            Slice::from_slice(b"svg"),
        );
        let pixels = rasterize(&image, self.scale).ok_or_else(|| invalid("cursor image"))?;
        let (width, height) = (pixels.width().min(self.size), pixels.height().min(self.size));
        let mut buffer =
            self.device.create_dumb_buffer((self.size, self.size), DrmFourcc::Argb8888, 32)?;
        let pitch = drm::buffer::Buffer::pitch(&buffer) as usize;
        {
            let mut mapping = self.device.map_dumb_buffer(&mut buffer)?;
            let bytes: &mut [u8] = mapping.as_mut();
            bytes.fill(0);
            let source = pixels.as_bytes();
            for row in 0..height as usize {
                for column in 0..width as usize {
                    let from = (row * pixels.width() as usize + column) * 4;
                    let to = row * pitch + column * 4;
                    // Premultiplied RGBA to little-endian ARGB8888 (B, G, R, A in memory).
                    bytes[to] = source[from + 2];
                    bytes[to + 1] = source[from + 1];
                    bytes[to + 2] = source[from];
                    bytes[to + 3] = source[from + 3];
                }
            }
        }
        let (x, y) = shape.hotspot(width, height);
        Ok(Loaded { buffer, hotspot: (x as i32, y as i32) })
    }

    fn move_to(&mut self, x: i32, y: i32) {
        self.position = (x, y);
        if self.hidden {
            return;
        }
        if !self.moved {
            self.moved = true;
            self.apply(Some(self.wanted));
            return;
        }
        self.update_position();
    }

    fn update_position(&mut self) {
        let hotspot = self.shown.and_then(|shape| self.shapes.get(&shape)).map_or((0, 0), |l| l.hotspot);
        let (x, y) = (self.position.0 - hotspot.0, self.position.1 - hotspot.1);
        if let Err(error) = self.device.move_cursor(self.crtc, (x, y)) {
            eprintln!("agx: moving the hardware cursor failed ({error}); drawing the pointer in the frame");
            self.failed = true;
        }
    }
}

fn rasterize(image: &Image, scale: u32) -> Option<i_slint_core::graphics::SharedPixelBuffer<i_slint_core::graphics::Rgba8Pixel>> {
    let inner: &i_slint_core::ImageInner = image.into();
    let i_slint_core::ImageInner::Svg(svg) = inner else {
        return None;
    };
    let size = svg.size();
    let target = (size * scale).cast_unit();
    match svg.render(Some(target)).ok()? {
        i_slint_core::graphics::SharedImageBuffer::RGBA8Premultiplied(buffer) => Some(buffer),
        _ => None,
    }
}
