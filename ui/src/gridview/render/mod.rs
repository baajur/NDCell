//! 2D and 3D grid rendering.

use glium::glutin::event::ModifiersState;
use send_wrapper::SendWrapper;
use std::cell::RefCell;

use ndcell_core::prelude::*;

use crate::config::{Config, MouseDragBinding};
use crate::mouse::{MouseDisplay, MouseState};

mod generic;
mod gl_ndtree;
mod ibos;
mod picker;
mod render2d;
mod render3d;
mod resizing;
mod shaders;
mod textures;
mod vbos;
mod vertices;

pub(super) use render2d::GridViewRender2D;
pub(super) use render3d::GridViewRender3D;

mod consts {
    /// Width of gridlines, measured in cells.
    pub const GRIDLINE_WIDTH: f64 = 1.0 / 32.0;
    /// Width of hover outline, measured in cells.
    pub const HOVER_HIGHLIGHT_WIDTH: f64 = 2.0 * GRIDLINE_WIDTH;
    /// Width of selection outline, measured in cells.
    pub const SELECTION_HIGHLIGHT_WIDTH: f64 = 4.0 * GRIDLINE_WIDTH;
    /// Width of selection resize preview outline, measured in cells.
    pub const SELECTION_RESIZE_PREVIEW_WIDTH: f64 = 2.0 * GRIDLINE_WIDTH;

    /// Coefficient to use for gridline spacing.
    ///
    /// `a` in `a * b^n`
    pub const GRIDLINE_SPACING_COEFF: usize = 1;
    /// Exponential base to use for fading out smaller gridlines.
    ///
    /// `b` in `a * b^n` (`n` varies based on zoom level)
    pub const GRIDLINE_SPACING_BASE: usize = 8;
    /// Minimum number of units (pixels in 2D) between gridlines.
    pub const MIN_GRIDLINE_SPACING: f64 = 4.0;
    /// Minimum number of units (pixels in 2D) between gridlines with full opacity.
    pub const MAX_GRIDLINE_SPACING: f64 = 256.0;
    /// Maximum opacity of gridlines when zoomed out beyond 1:1.
    ///
    /// This is less than 1.0 so that gridlines do not completely obscure the
    /// presence of cells.
    pub const ZOOMED_OUT_MAX_GRID_ALPHA: f64 = 0.75;

    /// Number of quads in each render batch.
    pub const QUAD_BATCH_SIZE: usize = 4096;
    /// Number of mouse target rectangles in each render batch.
    pub const MOUSE_TARGET_BATCH_SIZE: usize = 256;

    /// Depth at which to render gridlines.
    pub const GRIDLINE_DEPTH: f32 = 0.1;
    /// Depth at which to render highlight/crosshairs.
    pub const CURSOR_DEPTH: f32 = 0.2;
    /// Depth at which to render selection rectangle.
    pub const SELECTION_DEPTH: f32 = 0.3;
    /// Depth at which to render selection resize preview.
    pub const SELECTION_RESIZE_DEPTH: f32 = 0.4;

    /// Direction that 3D light comes from (normalized in GLSL).
    pub const LIGHT_DIRECTION: [f32; 3] = [1.0, 7.0, -3.0];
    /// Proportion of 3D light that is ambient, as opposed to directional.
    pub const LIGHT_AMBIENTNESS: f32 = 0.4;
    /// Maximum 3D light level.
    pub const MAX_LIGHT: f32 = 1.0;

    /// Proportional radius of the visible area beyond which there is fog.
    pub const FOG_START_FACTOR: f32 = 0.8;

    /// Small offset used to force correct Z order or align things at the
    /// sub-pixel scale.
    pub const TINY_OFFSET: f32 = 1.0 / 16.0;
}

lazy_static! {
    static ref CACHE: SendWrapper<RefCell<RenderCache>> =
        SendWrapper::new(RefCell::new(RenderCache::default()));
}

/// Parameters that may control the rendering process.
pub struct RenderParams<'a> {
    /// Render target.
    pub target: &'a mut glium::Frame,
    /// User configuration.
    pub config: &'a Config,
    /// Mouse state.
    pub mouse: MouseState,
    /// Modifiers held on the keyboard.
    pub modifiers: ModifiersState,
}

/// Data generated when rendering a frame.
#[derive(Debug, Default, Clone)]
pub struct RenderResult {
    /// Target under the mouse cursor, if any.
    pub mouse_target: Option<MouseTargetData>,
}

pub(super) struct CellDrawParams<'a, D: Dim> {
    /// ND-tree to draw.
    pub ndtree: &'a NdTree<D>,
    /// Rectangular region of the ND-tree to draw.
    pub rect: Option<&'a BigRect<D>>,
    /// Alpha value for the whole ND-tree.
    pub alpha: f32,
}

/// How to handle a mouse hover or click on a particular location on the screen.
#[derive(Debug, Default, Clone)]
pub struct MouseTargetData {
    /// Mouse binding for clicking the left mouse button over the target and
    /// dragging.
    pub binding: Option<MouseDragBinding>,
    /// Display mode for the cursor when hovering over the target or clicking on
    /// it and dragging.
    pub display: MouseDisplay,
}

#[derive(Default)]
struct RenderCache {
    pub ibos: ibos::IboCache,
    pub vbos: vbos::VboCache,
    pub textures: textures::TextureCache,
    pub picker: picker::MousePicker,
    pub gl_quadtrees: gl_ndtree::GlQuadtreeCache,
    pub gl_octrees: gl_ndtree::GlOctreeCache,
}

pub fn post_frame_clean_cache() {
    let mut cache = CACHE.borrow_mut();
    cache.gl_quadtrees.post_frame_clean_cache();
}
