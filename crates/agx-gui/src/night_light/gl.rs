//! The night light multiply pass, drawn over the finished frame in Slint's `AfterRendering` hook.
//!
//! One full-screen triangle with blending set to `destination × gain` scales each colour channel of
//! everything already drawn (windows, browser, terminal, pointer) and leaves alpha alone. It costs
//! nothing while night light is off, and the sliders only move the three gain numbers.
//!
//! Slint's femtovg renderer owns the GL state, so the pass saves and restores what it touches.
use crate::{AppWindow, night_light};
use glow::HasContext;
use slint::{GraphicsAPI, Weak};
use std::ffi::CString;

/// Covers the screen with one triangle: (-1,-1), (3,-1), (-1,3).
const TRIANGLE: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];

const ES_VERTEX: &str = "attribute vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }";
const ES_FRAGMENT: &str =
    "precision mediump float; uniform vec3 gain; void main() { gl_FragColor = vec4(gain, 1.0); }";
const CORE_VERTEX: &str =
    "#version 330 core\nlayout(location = 0) in vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }";
const CORE_FRAGMENT: &str =
    "#version 330 core\nuniform vec3 gain; out vec4 color; void main() { color = vec4(gain, 1.0); }";

pub struct Pass {
    gl: glow::Context,
    program: glow::Program,
    buffer: glow::Buffer,
    gain: glow::UniformLocation,
    /// Desktop core profiles need a vertex array object; GLES does not.
    vao: Option<glow::VertexArray>,
}

impl Pass {
    /// # Safety
    /// Call from Slint's `RenderingSetup`, with its GL context current.
    pub unsafe fn new(api: &GraphicsAPI) -> Result<Self, String> {
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            return Err("No OpenGL renderer".into());
        };
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                get_proc_address(&CString::new(name).unwrap())
            })
        };
        let version = gl.version();
        let (vertex, fragment) = if version.is_embedded {
            (ES_VERTEX, ES_FRAGMENT)
        } else if version.major >= 3 {
            (CORE_VERTEX, CORE_FRAGMENT)
        } else {
            return Err(format!("OpenGL {}.{} is too old", version.major, version.minor));
        };
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, source) in [(glow::VERTEX_SHADER, vertex), (glow::FRAGMENT_SHADER, fragment)] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    let log = gl.get_shader_info_log(shader);
                    gl.delete_shader(shader);
                    gl.delete_program(program);
                    return Err(format!("night light shader: {log}"));
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }
            gl.bind_attrib_location(program, 0, "p");
            gl.link_program(program);
            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }
            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("night light program: {log}"));
            }
            let gain = gl
                .get_uniform_location(program, "gain")
                .ok_or("night light shader has no gain")?;
            let buffer = gl.create_buffer()?;
            let previous_buffer = gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&TRIANGLE), glow::STATIC_DRAW);
            gl.bind_buffer(glow::ARRAY_BUFFER, previous_buffer);
            let vao = if version.is_embedded {
                None
            } else {
                let previous = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
                let vao = gl.create_vertex_array()?;
                gl.bind_vertex_array(Some(vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
                gl.bind_buffer(glow::ARRAY_BUFFER, previous_buffer);
                gl.bind_vertex_array(previous);
                Some(vao)
            };
            Ok(Self {
                gl,
                program,
                buffer,
                gain,
                vao,
            })
        }
    }

    /// Scale the bound framebuffer by the current gains; does nothing while night light is off.
    ///
    /// # Safety
    /// Call from Slint's `AfterRendering`, with its GL context current.
    pub unsafe fn draw(&self) {
        let Some(gain) = night_light::gain() else {
            return;
        };
        let gl = &self.gl;
        unsafe {
            let program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
            let buffer = gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING);
            let vao = self
                .vao
                .map(|_| gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING));
            let (blend, scissor, stencil, depth, cull) = (
                gl.is_enabled(glow::BLEND),
                gl.is_enabled(glow::SCISSOR_TEST),
                gl.is_enabled(glow::STENCIL_TEST),
                gl.is_enabled(glow::DEPTH_TEST),
                gl.is_enabled(glow::CULL_FACE),
            );
            let func = [
                glow::BLEND_SRC_RGB,
                glow::BLEND_DST_RGB,
                glow::BLEND_SRC_ALPHA,
                glow::BLEND_DST_ALPHA,
            ]
            .map(|parameter| gl.get_parameter_i32(parameter) as u32);
            let attribute = self.vao.is_none().then(|| {
                let mut enabled = [0.0_f32; 4];
                gl.get_vertex_attrib_parameter_f32_slice(
                    0,
                    glow::VERTEX_ATTRIB_ARRAY_ENABLED,
                    &mut enabled,
                );
                enabled[0] != 0.0
            });

            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::STENCIL_TEST);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.color_mask(true, true, true, true);
            gl.enable(glow::BLEND);
            // result = destination × gain, alpha untouched.
            gl.blend_func_separate(glow::ZERO, glow::SRC_COLOR, glow::ZERO, glow::ONE);
            gl.use_program(Some(self.program));
            gl.uniform_3_f32(Some(&self.gain), gain[0], gain[1], gain[2]);
            match self.vao {
                Some(vao) => gl.bind_vertex_array(Some(vao)),
                None => {
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.buffer));
                    gl.enable_vertex_attrib_array(0);
                    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
                }
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            if attribute == Some(false) {
                gl.disable_vertex_attrib_array(0);
            }
            if let Some(vao) = vao {
                gl.bind_vertex_array(vao);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer);
            gl.use_program(program);
            gl.blend_func_separate(func[0], func[1], func[2], func[3]);
            for (enabled, capability) in [
                (blend, glow::BLEND),
                (scissor, glow::SCISSOR_TEST),
                (stencil, glow::STENCIL_TEST),
                (depth, glow::DEPTH_TEST),
                (cull, glow::CULL_FACE),
            ] {
                if enabled {
                    gl.enable(capability);
                } else {
                    gl.disable(capability);
                }
            }
        }
    }

    /// # Safety
    /// Call from Slint's `RenderingTeardown`, with its GL context current.
    pub unsafe fn destroy(self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_buffer(self.buffer);
            if let Some(vao) = self.vao {
                self.gl.delete_vertex_array(vao);
            }
        }
    }
}

fn bytes(floats: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding or invalid bit patterns, and u8 has alignment 1.
    unsafe { std::slice::from_raw_parts(floats.as_ptr().cast(), std::mem::size_of_val(floats)) }
}

/// Build the pass in Slint's `RenderingSetup` and hide the Slint overlay once it works.
pub fn setup(ui: &Weak<AppWindow>, api: &GraphicsAPI) -> Option<Pass> {
    match unsafe { Pass::new(api) } {
        Ok(pass) => {
            if let Some(ui) = ui.upgrade() {
                night_light::mark_native(&ui);
            }
            eprintln!("agx-gui: night light: GPU colour pass ready");
            Some(pass)
        }
        Err(error) => {
            eprintln!("agx-gui: night light: GPU pass unavailable: {error}; using an overlay");
            None
        }
    }
}

/// The desktop preview's own render hook (the KMS build shares the browser's).
#[cfg(feature = "desktop")]
pub fn install_preview(ui: &AppWindow) {
    use slint::{ComponentHandle, RenderingState};
    let weak = ui.as_weak();
    let mut pass: Option<Pass> = None;
    let result = ui.window().set_rendering_notifier(move |event, api| match event {
        RenderingState::RenderingSetup => {
            crate::drivers::record_renderer(&weak, api);
            pass = setup(&weak, api);
        }
        RenderingState::AfterRendering => {
            if let Some(pass) = &pass {
                unsafe { pass.draw() };
            }
        }
        RenderingState::RenderingTeardown => {
            if let Some(pass) = pass.take() {
                unsafe { pass.destroy() };
            }
        }
        _ => {}
    });
    if let Err(error) = result {
        eprintln!("agx-gui: night light: render hook unavailable: {error}; using an overlay");
    }
}
