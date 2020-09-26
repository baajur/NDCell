//! 2D grid rendering.
//!
//! Currently, only solid colors are supported, however I plan to add icons in
//! the future.
//!
//! Not including preliminary computations and extra effects like gridlines,
//! there are four main stages to rendering a grid of cells:
//!
//! 1. Create an indexed quadtree of render cells encoded in an OpenGL texture.
//!    (A "render cell" is a node of the quadtree which is rendered as single
//!    square; this is one cell when zoomed in, but may be larger when zoomed
//!    out.)
//! 2. Render the visible portion of that quadtree into an OpenGL texture, where
//!    each pixel represents one cell.
//! 3. Resize that texture to the next-highest power-of-2 scale using
//!    nearest-neighbor sampling. For example, if the scale factor is 2.4 (i.e.
//!    each cell takes up a square 2.4 pixels wide), resize the texture by a
//!    factor of 4.
//! 4. Blit that texture onto the screen at the appropriate position using
//!    linear sampling, which accounts for the remaining scaling.
//!
//! We resize the texture twice to get the best of both nearest-neighbor scaling
//! and linear scaling: if we only used nearest-neighbor, cells at non-integer
//! scale factors would look uneven; some cells would be larger and some would
//! be smaller, and many would be slightly rectangular instead of square. (You
//! can see this effect using the default configuration in LifeViewer.) If we
//! only used linear scaling, then cells would appear blurry when zoomed in.
//! (This is a common problem when resizing pixel art.) Performing
//! nearest-neighbor scaling for the potentially large integer factor and then
//! linear scaling for the relatively small remaining factor yields a much nicer
//! result that imperceptibly blends between different scale factors.
//!
//! TODO: Write custom vertex shader that can do pretty resizing in one step.
//!
//! It's worth noting that translation is always rounded to the nearest pixel,
//! so at any integer scale factor, cell boundaries are always between pixels. At
//! non-integer scale factors, however, this is not guaranteed.

use anyhow::{Context, Result};
use cgmath::Matrix4;
use glium::index::PrimitiveType;
use glium::{uniform, Surface};
use itertools::Itertools;

use ndcell_core::axis::{X, Y};
use ndcell_core::prelude::*;

use super::consts::*;
use super::gl_quadtree::CachedGlQuadtree;
use super::vertices::RgbaVertex;
use super::{ibos, shaders, textures, vbos};
use crate::gridview::*;

#[derive(Default)]
pub struct RenderCache {
    gl_quadtree: CachedGlQuadtree,
}

pub struct RenderInProgress<'a> {
    /// Camera to render the grid from.
    camera: Camera2D,
    /// Target to render to.
    target: &'a mut glium::Frame,
    /// Node layer of a render cell.
    render_cell_layer: Layer,
    /// Scale to draw render cells at.
    render_cell_scale: Scale,
    /// Quadtree encompassing all visible cells.
    visible_quadtree: NdTreeSlice2D<'a>,
    /// The render cell position within `visible_quadtree` that is centered on the
    /// screen.
    pos: FVec2D,
    /// Rectangle of render cells within `visible_quadtree` that is visible.
    visible_rect: IRect2D,
    /// Transform from `visible_quadtree` space (1 unit = 1 render cell; (0, 0)
    /// = bottom left) to screen space ((-1, -1) = bottom left; (1, 1) = top
    /// right) and pixel space (1 unit = 1 pixel; (0, 0) = top left).
    transform: CellTransform2D,
    /// Cached render data maintained by the `GridView2D`.
    render_cache: &'a mut RenderCache,
}
impl<'a> RenderInProgress<'a> {
    /// Creates a `RenderInProgress` for a gridview.
    pub fn new(
        g: &GridView2D,
        node_cache: &'a NodeCache<Dim2D>,
        render_cache: &'a mut RenderCache,
        target: &'a mut glium::Frame,
    ) -> Result<Self> {
        target.clear_depth(0.0);

        let (target_w, target_h) = target.get_dimensions();
        if target_w < 5 || target_h < 5 {
            return Err(anyhow!(
                "2D grid render target too small: ({}, {})",
                target_w,
                target_h,
            ));
        }
        let target_pixels_size: FVec2D = NdVec([r64(target_w as f64), r64(target_h as f64)]);
        let camera = g.camera().clone();

        // Determine the lowest layer of the quadtree that we must visited,
        // which is the layer of a "render cell," a quadtree node that is
        // rendered as one unit (one pixel in step #1).
        let render_cell_layer: Layer = Layer(
            (-camera.scale().round().log2_factor())
                .to_u32()
                .unwrap_or(0),
        );
        // Compute the width of cells represented by each render cell.
        let render_cell_len: BigInt = BigInt::from(1) << render_cell_layer.to_u32();
        // Compute the scaling to use for render cells by the scale factor by
        // the size of a render cell -- except it's addition, because we're
        // actually using logarithms.
        let render_cell_scale = Scale::from_log2_factor(
            camera.scale().log2_factor() + render_cell_layer.to_u32() as f64,
        );
        // Now the scale factor should be greater than 1/2 (since 1/2 or
        // anything lower should be handled by having a higher render cell
        // layer). log₂(1/2) = -1.
        assert!(render_cell_scale.log2_factor() > -1.0);

        // Get a smaller slice of the grid that covers the entire visible area,
        // a rectangle of visible render cells relative to that node, and a
        // floating-point render cell position relative to that node that will
        // be in the center of the screen.
        let visible_quadtree: NdTreeSlice2D<'a>;
        let visible_rect: IRect2D;
        let mut pos: FVec2D;
        {
            // Find the global rectangle of visible cells.
            let global_visible_rect: BigRect2D;
            {
                // Compute the width and height of individual cells that fit on
                // the screen.
                let target_cells_size: FixedVec2D = camera
                    .scale()
                    .units_to_cells(target_pixels_size.to_fixedvec());
                // Compute the cell vector pointing from the origin to the top
                // right corner of the screen; i.e. the "half diagonal."
                let half_diag: FixedVec2D = target_cells_size / 2.0;

                global_visible_rect =
                    BigRect2D::centered(camera.pos().floor().0, &half_diag.ceil().0);
            }

            // Now fetch the `NdTreeSlice` containing all of the visible cells.
            visible_quadtree = g
                .automaton
                .projected_tree()
                .slice_containing(node_cache, &global_visible_rect);

            // Subtract the slice offset from `global_visible_rect` and
            // `global_visible_rect` to get the rectangle of visible cells
            // within the slice.
            let mut tmp_visible_rect = global_visible_rect - &visible_quadtree.offset;
            // Divide by `render_cell_len` to get render cells.
            tmp_visible_rect = tmp_visible_rect.div_outward(&render_cell_len);
            // Now it is safe to convert from `BigInt` to `isize`, because the
            // number of visible render cells must be reasonable.
            visible_rect = tmp_visible_rect.to_irect();

            // Subtract the slice offset from the camera position and divide by
            // the size of a render cell to get the render cell offset from the
            // lower corner of the slice.
            pos = ((camera.pos() - visible_quadtree.offset.to_fixedvec())
                >> render_cell_layer.to_u32())
            .to_fvec();
            // TODO: uncomment this
            // // Round to the nearest pixel.
            // pos = (pos * render_cell_scale.pixels_per_cell()).round()
            //     * render_cell_scale.cells_per_pixel();
            // Offset by half a pixel if the target dimensions are odd, so that
            // cells boundaries always line up with pixel boundaries.
            pos += (target_pixels_size % 2.0) * render_cell_scale.cells_per_unit() / 2.0;
        }

        // Compute the render cell transform; see `NdCellTransform` docs.
        let render_cell_transform: Matrix4<f32>;
        {
            // Compute the scale factor for the view matrix. Multiply by 2
            // because the OpenGL screen space ranges from -1 to +1.
            let pixels_per_render_cell = render_cell_scale.units_per_cell();
            let scale = FVec2D::repeat(pixels_per_render_cell);

            // Determine the translation offset for the view matrix.
            let mut offset = pos;
            // Offset by a tiny fraction of a pixel so that gridlines round to
            // the top-right instead of rounding whichever way OpenGL picks.
            offset -= r64(TINY_OFFSET as f64) * render_cell_scale.cells_per_unit();

            // The offset is in units of render cells, not screen space units,
            // so we apply the translation before scaling, contrary to the usual
            // scale-rotate-translate order in computer graphics.
            let scale_x = scale[X].to_f32().context("scale[X].to_f32()")?;
            let scale_y = scale[Y].to_f32().context("scale[Y].to_f32()")?;
            let offset_x = offset[X].to_f32().context("offset[X].to_f32()")?;
            let offset_y = offset[Y].to_f32().context("offset[Y].to_f32()")?;
            render_cell_transform = Matrix4::from_nonuniform_scale(scale_x, scale_y, 1.0)
                * Matrix4::from_translation(-cgmath::Vector3::new(offset_x, offset_y, 0.0));
        }

        let transform = CellTransform2D::new_ortho(
            visible_quadtree.offset.clone(),
            render_cell_layer,
            render_cell_transform,
            (target_w as f32, target_h as f32),
        );

        Ok(Self {
            camera,
            target,
            render_cell_layer,
            render_cell_scale,
            visible_quadtree,
            pos,
            visible_rect,
            transform,
            render_cache,
        })
    }

    pub fn cell_transform(&self) -> &CellTransform2D {
        &self.transform
    }

    /// Draw the cells that appear in the viewport.
    pub fn draw_cells(&mut self) -> Result<()> {
        let textures: &mut textures::TextureCache = &mut textures::CACHE.borrow_mut();
        // Steps #1: encode the quadtree as a texture.
        let gl_quadtree = self.render_cache.gl_quadtree.from_node(
            self.visible_quadtree.root.into(),
            self.render_cell_layer,
            Self::node_pixel_color,
        )?;
        // Step #2: draw at 1 pixel per render cell, including only the cells
        // inside `self.visible_rect`.
        let unscaled_cells_w = self.visible_rect.len(X) as u32;
        let unscaled_cells_h = self.visible_rect.len(Y) as u32;
        let (_tex, mut unscaled_cells_fbo, unscaled_cells_texture_fract) = textures
            .unscaled_cells
            .at_min_size(unscaled_cells_w, unscaled_cells_h);
        unscaled_cells_fbo
            .draw(
                &*vbos::quadtree_quad_with_quadtree_coords(
                    self.visible_rect,
                    unscaled_cells_texture_fract,
                ),
                &glium::index::NoIndices(PrimitiveType::TriangleStrip),
                &shaders::QUADTREE,
                &uniform! {
                    quadtree_texture: &gl_quadtree.texture,
                    layer_count: gl_quadtree.layers as i32,
                    root_idx: gl_quadtree.root_idx as u32,
                },
                &glium::DrawParameters::default(),
            )
            .context("unscaled_cells_fbo.draw()")?;

        // Step #3: resize that texture by an integer factor.
        // Compute the width of pixels for each render cell.
        let pixels_per_render_cell = self.render_cell_scale.units_per_cell();
        let integer_scale_factor = pixels_per_render_cell.round().to_u32().unwrap();
        let visible_cells_w = self.visible_rect.len(X) as u32;
        let visible_cells_h = self.visible_rect.len(Y) as u32;
        let scaled_cells_w = visible_cells_w * integer_scale_factor;
        let scaled_cells_h = visible_cells_h * integer_scale_factor;
        let (scaled_cells_texture, scaled_cells_fbo, scaled_cells_texture_fract) = textures
            .scaled_cells
            .at_min_size(scaled_cells_w, scaled_cells_h);
        scaled_cells_fbo.blit_from_simple_framebuffer(
            &unscaled_cells_fbo,
            &glium::Rect {
                left: 0,
                bottom: 0,
                width: unscaled_cells_w,
                height: unscaled_cells_h,
            },
            &glium::BlitTarget {
                left: 0,
                bottom: 0,
                width: scaled_cells_w as i32,
                height: scaled_cells_h as i32,
            },
            glium::uniforms::MagnifySamplerFilter::Nearest,
        );

        // Step #4: render that onto the screen.
        let (target_w, target_h) = self.target.get_dimensions();
        let target_size = NdVec([r64(target_w as f64), r64(target_h as f64)]);
        let render_cells_size = target_size / self.render_cell_scale.units_per_cell();
        let render_cells_center = self.pos - self.visible_rect.min().to_fvec();
        let mut render_cells_frect =
            FRect2D::centered(render_cells_center, render_cells_size / 2.0);
        render_cells_frect /= self.visible_rect.size().to_fvec();
        render_cells_frect *= scaled_cells_texture_fract;

        self.target
            .draw(
                &*vbos::blit_quad_with_src_coords(render_cells_frect),
                &glium::index::NoIndices(PrimitiveType::TriangleStrip),
                &shaders::BLIT,
                &uniform! {
                    src_texture: scaled_cells_texture.sampled(),
                    alpha: 1.0_f32,
                },
                &glium::DrawParameters::default(),
            )
            .context("Drawing cells to target")?;

        // // Draw a 1:1 "minimap" in the corner
        // self.target.blit_from_simple_framebuffer(
        //     &unscaled_cells_fbo,
        //     &entire_rect(&unscaled_cells_fbo),
        //     &entire_blit_target(&unscaled_cells_fbo),
        //     glium::uniforms::MagnifySamplerFilter::Linear,
        // );

        Ok(())
    }

    /// Draws gridlines at varying opacity and spacing depending on scaling.
    ///
    /// TOOD: support arbitrary exponential base and factor (a*b^n for any a, b)
    pub fn draw_gridlines(&mut self, width: f64) -> Result<()> {
        // Compute the minimum pixel spacing between maximum-opacity gridlines.
        let log2_max_pixel_spacing = r64(MAX_GRIDLINE_SPACING).log2();
        // Compute the cell spacing between the gridlines that will be drawn
        // with the maximum opacity.
        let log2_cell_spacing = log2_max_pixel_spacing - self.camera.scale().log2_factor();
        // Round up to the nearest power of GRIDLINE_SPACING_BASE.
        let log2_spacing_base = (GRIDLINE_SPACING_BASE as f64).log2();
        let log2_cell_spacing = (log2_cell_spacing / log2_spacing_base).ceil() * log2_spacing_base;
        // Convert from cells to render cells.
        let log2_render_cell_spacing = log2_cell_spacing - self.render_cell_layer.to_u32() as f64;
        let mut spacing = log2_render_cell_spacing.exp2().raw() as usize;
        // Convert from render cells to pixels.
        let mut pixel_spacing = spacing as f64 * self.render_cell_scale.units_per_cell().raw();

        while spacing > 0 && pixel_spacing > MIN_GRIDLINE_SPACING {
            // Compute grid color, including alpha.
            let mut color = GRID_COLOR;
            let alpha = gridline_alpha(pixel_spacing, self.camera.scale()) as f32;
            color[3] *= alpha;
            // Draw gridlines with the given spacing.
            let offset = (-self.visible_rect.min()).mod_floor(&(spacing as isize));
            self.draw_cell_overlay_rects(
                &self.generate_solid_cell_borders(
                    self.visible_rect
                        .axis_range(X)
                        .skip(offset[X] as usize)
                        .step_by(spacing),
                    self.visible_rect
                        .axis_range(Y)
                        .skip(offset[Y] as usize)
                        .step_by(spacing),
                    GRIDLINE_DEPTH,
                    width,
                    color,
                ),
            )
            .context("Drawing gridlines")?;
            // Decrease the spacing.
            spacing /= GRIDLINE_SPACING_BASE;
            pixel_spacing /= GRIDLINE_SPACING_BASE as f64;
        }
        Ok(())
    }

    /// Draws a highlight around the given cell.
    pub fn draw_blue_cursor_highlight(
        &mut self,
        cell_position: &BigVec2D,
        width: f64,
    ) -> Result<()> {
        if !self.visible_quadtree.rect().contains(cell_position) {
            warn!(
                "Cursor position is outside visible rectangle: {:?}",
                cell_position
            );
            return Ok(());
        }
        let cell_pos = cell_position - &self.visible_quadtree.offset;
        let render_cell_pos = cell_pos.div_floor(&self.render_cell_layer.big_len());

        self.draw_cell_overlay_rects(&self.generate_cell_rect_outline(
            IRect2D::single_cell(render_cell_pos.to_ivec()),
            CURSOR_DEPTH,
            width,
            GRID_HIGHLIGHT_COLOR,
            RectHighlightParams {
                fill: true,
                crosshairs: true,
            },
        ))
        .context("Drawing blue cursor highlight")?;
        Ok(())
    }

    /// Generates a cell overlay to outline the given cell rectangle, with
    /// optional fill and crosshairs.
    #[must_use = "This method only generates the rectangles; call `draw_cell_overlay_rects` to draw them"]
    fn generate_cell_rect_outline(
        &self,
        rect: IRect2D,
        z: f32,
        width: f64,
        color: [f32; 4],
        params: RectHighlightParams,
    ) -> Vec<CellOverlayRect> {
        let bright_color = color;
        let mut dull_color = color;
        dull_color[3] *= 0.25;
        let mut fill_color = color;
        fill_color[0] *= 0.5;
        fill_color[1] *= 0.5;
        fill_color[2] *= 0.5;
        fill_color[3] *= 0.75;

        let NdVec([min_x, min_y]) = self.visible_rect.min();
        let NdVec([max_x, max_y]) = self.visible_rect.max() + 1;

        // If there are more than 1.5 pixels per render cell, the upper boundary
        // should be *between* cells (+1). If there are fewer, the upper
        // boundary should be *on* the cell (+0).
        let pixels_per_cell = self.render_cell_scale.units_per_cell();
        let a = rect.min();
        let b = rect.max() + (pixels_per_cell > 1.5) as isize;
        let NdVec([ax, ay]) = a;
        let NdVec([bx, by]) = b;

        let mut h_stops = vec![
            (min_x, dull_color),
            (ax - 1, dull_color),
            (ax, bright_color),
            (bx, bright_color),
            (bx + 1, dull_color),
            (max_x, dull_color),
        ];
        let mut v_stops = vec![
            (min_y, dull_color),
            (ay - 1, dull_color),
            (ay, bright_color),
            (by, bright_color),
            (by + 1, dull_color),
            (max_y, dull_color),
        ];

        // Optionally remove crosshairs.
        if !params.crosshairs {
            h_stops = h_stops
                .into_iter()
                .filter(|&(_, color)| color == bright_color)
                .collect();
            v_stops = v_stops
                .into_iter()
                .filter(|&(_, color)| color == bright_color)
                .collect();
        }

        let mut ret = vec![];

        // In case the crosshairs/outline is transparent, render gridlines
        // beneath it. Draw order works in our favor here:
        // https://stackoverflow.com/a/20231235/4958484
        if params.crosshairs {
            ret.extend_from_slice(&self.generate_solid_cell_borders(
                vec![ax, bx],
                vec![ay, by],
                z - TINY_OFFSET,
                width,
                GRID_COLOR,
            ));
        }

        // Generate lines.
        for &x in &[ax, bx] {
            ret.extend_from_slice(&self.generate_gradient_cell_border(
                v_stops.iter().map(|&(y, color)| (NdVec([x, y]), color)),
                z,
                width,
                Y,
            ));
        }
        for &y in &[ay, by] {
            ret.extend_from_slice(&self.generate_gradient_cell_border(
                h_stops.iter().map(|&(x, color)| (NdVec([x, y]), color)),
                z,
                width,
                X,
            ));
        }

        // Generate fill after lines.
        if params.fill {
            ret.push(CellOverlayRect {
                start: a,
                end: b,
                z,
                start_color: fill_color,
                end_color: fill_color,
                line_params: None,
            })
        }

        ret
    }

    /// Generates a cell overlay for solid borders along the given columns and
    /// rows.
    #[must_use = "This method only generates the rectangles; call `draw_cell_overlay_rects` to draw them"]
    fn generate_solid_cell_borders(
        &self,
        columns: impl IntoIterator<Item = isize>,
        rows: impl IntoIterator<Item = isize>,
        z: f32,
        width: f64,
        color: [f32; 4],
    ) -> Vec<CellOverlayRect> {
        let min = self.visible_rect.min();
        let max = self.visible_rect.max() + 1;
        let min_x = min[X];
        let min_y = min[Y];
        let max_x = max[X];
        let max_y = max[Y];

        let h_line_params = Some(LineParams {
            width,
            include_endpoints: true,
            axis: X,
        });
        let v_line_params = Some(LineParams {
            width,
            include_endpoints: true,
            axis: Y,
        });

        let mut ret = Vec::with_capacity(4 * self.visible_rect.size().sum() as usize);
        for x in columns {
            ret.push(CellOverlayRect {
                start: NdVec([x, min_y]),
                end: NdVec([x, max_y]),
                z,
                start_color: color,
                end_color: color,
                line_params: v_line_params,
            });
        }
        for y in rows {
            ret.push(CellOverlayRect {
                start: NdVec([min_x, y]),
                end: NdVec([max_x, y]),
                z,
                start_color: color,
                end_color: color,
                line_params: h_line_params,
            });
        }
        ret
    }

    /// Generates a cell overlay for a gradient cell border.
    #[must_use = "This method only generates the rectangles; call `draw_cell_overlay_rects` to draw them"]
    fn generate_gradient_cell_border(
        &self,
        stops: impl IntoIterator<Item = (IVec2D, [f32; 4])>,
        z: f32,
        width: f64,
        axis: Axis,
    ) -> Vec<CellOverlayRect> {
        // Generate a rectangle for each stop (so that there is a definitive
        // color at each point) AND a rectangle between each adjacent pair of
        // stops.
        let mut ret = vec![];
        let mut prev_stop = None;
        let btwn_stops_line_params = Some(LineParams {
            width,
            include_endpoints: false,
            axis,
        });
        let single_stop_line_params = Some(LineParams {
            width,
            include_endpoints: true,
            axis,
        });
        for stop in stops {
            let (pos, color) = stop;
            if let Some((prev_pos, prev_color)) = prev_stop {
                ret.push(CellOverlayRect {
                    start: prev_pos,
                    end: pos,
                    z,
                    start_color: prev_color,
                    end_color: color,
                    line_params: btwn_stops_line_params,
                });
            }
            ret.push(CellOverlayRect {
                start: pos,
                end: pos,
                z,
                start_color: color,
                end_color: color,
                line_params: single_stop_line_params,
            });
            prev_stop = Some(stop);
        }
        ret
    }

    /// Draws a cell overlay.
    fn draw_cell_overlay_rects(&mut self, rects: &[CellOverlayRect]) -> Result<()> {
        // Draw the gridlines in batches, because the VBO might not be able to
        // hold all the points at once.
        for rect_batch in rects
            .into_iter()
            .chunks(CELL_OVERLAY_BATCH_SIZE)
            .into_iter()
        {
            let rect_batch = rect_batch.into_iter().collect_vec();
            let count = rect_batch.len();
            // Generate vertices.
            let verts = rect_batch
                .iter()
                .flat_map(|&rect| rect.verts(self.render_cell_scale).to_vec())
                .collect_vec();
            // Put the data in a slice of the VBO.
            let gridlines_vbo = vbos::gridlines();
            let vbo_slice = gridlines_vbo.slice(0..(4 * count)).unwrap();
            vbo_slice.write(&verts);
            // Draw gridlines.
            self.target
                .draw(
                    vbo_slice,
                    &ibos::rect_indices(count),
                    &shaders::POINTS,
                    &uniform! { matrix: self.transform.gl_matrix() },
                    &glium::DrawParameters {
                        blend: glium::Blend::alpha_blending(),
                        depth: glium::Depth {
                            test: glium::DepthTest::IfMore,
                            write: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .context("Drawing cell-aligned rectangles")?;
        }
        Ok(())
    }

    /// Returns the color for a pixel representing the given node.
    fn node_pixel_color(node: NodeRef<'_, Dim2D>) -> [u8; 4] {
        let (r, g, b) = if node.layer() == Layer(0) {
            let cell_state = node.as_leaf().unwrap().cells()[0];
            match cell_state {
                0 => DEAD_COLOR,
                1 => LIVE_COLOR,
                i => colorous::TURBO
                    .eval_rational(257 - i as usize, 256)
                    .as_tuple(),
            }
        } else {
            let ratio = if node.is_empty() {
                0.0
            } else {
                // Multiply then divide by 255 to keep some precision.
                let population_ratio = (node.population() * 255_usize / node.big_num_cells())
                    .to_f64()
                    .unwrap()
                    / 255.0;
                // Bias so that 50% is the minimum brightness if there is any population.
                (population_ratio / 2.0) + 0.5
            };
            let r = ((LIVE_COLOR.0 as f64).powf(2.0) * ratio
                + (DEAD_COLOR.0 as f64).powf(2.0) * (1.0 - ratio))
                .powf(0.5);
            let g = ((LIVE_COLOR.1 as f64).powf(2.0) * ratio
                + (DEAD_COLOR.1 as f64).powf(2.0) * (1.0 - ratio))
                .powf(0.5);
            let b = ((LIVE_COLOR.2 as f64).powf(2.0) * ratio
                + (DEAD_COLOR.2 as f64).powf(2.0) * (1.0 - ratio))
                .powf(0.5);
            (r as u8, g as u8, b as u8)
        };
        [r, g, b, 255]
    }
}

fn gridline_alpha(pixel_spacing: f64, scale: Scale) -> f64 {
    // Fade maximum grid alpha as zooming out beyond 1 cell per pixel.
    let max_alpha = clamped_interpolate(
        scale.log2_factor().raw(),
        0.0,
        1.0,
        ZOOMED_OUT_MAX_GRID_ALPHA,
        1.0,
    );
    let alpha = clamped_interpolate(
        pixel_spacing.log2(),
        (MIN_GRIDLINE_SPACING).log2(),
        (MAX_GRIDLINE_SPACING).log2(),
        0.0,
        1.0,
    );
    // Clamp to max alpha.
    if alpha > max_alpha {
        max_alpha
    } else {
        alpha
    }
}

fn clamped_interpolate(x: f64, min: f64, max: f64, min_result: f64, max_result: f64) -> f64 {
    if x < min {
        return min_result;
    }
    if x > max {
        return max_result;
    }
    let progress = (x - min) / (max - min);
    min_result + (max_result - min_result) * progress
}

/// A simple rectangle in a cell overlay.
///
/// Because glLineWidth is not supported on all platforms, we draw rectangles to
/// vary gridline width.
#[derive(Debug, Copy, Clone)]
struct CellOverlayRect {
    /// Start point of a line, or one corner of a rectangle.
    start: IVec2D,
    /// End point of a line, or other corner of a rectangle.
    end: IVec2D,
    /// Z order.
    z: f32,
    /// Color at the start of the line.
    start_color: [f32; 4],
    /// Color at the end of the line.
    end_color: [f32; 4],
    /// Optional parameters for lines.
    line_params: Option<LineParams>,
}
impl CellOverlayRect {
    fn solid_rect(rect: IRect2D, z: f32, color: [f32; 4]) -> Self {
        Self {
            start: rect.min(),
            end: rect.max(),
            z,
            start_color: color,
            end_color: color,
            line_params: None,
        }
    }
    fn verts(self, render_cell_scale: Scale) -> [RgbaVertex; 4] {
        let mut a = self.start.to_fvec();
        let mut b = self.end.to_fvec();
        let mut colors = [
            self.start_color,
            self.start_color,
            self.end_color,
            self.end_color,
        ];
        if let Some(LineParams {
            width,
            include_endpoints,
            axis,
        }) = self.line_params
        {
            let width = width.round().max(1.0);
            // At this point, the rectangle should have zero width.
            let cells_per_pixel = render_cell_scale.cells_per_unit();
            let offset = FVec::repeat(cells_per_pixel * width / 2.0) * (b - a).signum();
            // Expand it in all directions, so now it has the correct width and
            // includes its endpoints.
            a -= offset;
            b += offset;
            // Now exclude the endpoints, if requested.
            if !include_endpoints {
                a[axis] += offset[axis] * 2.0;
                b[axis] -= offset[axis] * 2.0;
            }
            if axis == X {
                // Use horizontal gradient instead of vertical gradient.
                colors.swap(1, 2);
            }
        }
        let ax = a[X].to_f32().unwrap();
        let ay = a[Y].to_f32().unwrap();
        let bx = b[X].to_f32().unwrap();
        let by = b[Y].to_f32().unwrap();
        [
            RgbaVertex::from(([ax, ay, self.z], colors[0])),
            RgbaVertex::from(([bx, ay, self.z], colors[1])),
            RgbaVertex::from(([ax, by, self.z], colors[2])),
            RgbaVertex::from(([bx, by, self.z], colors[3])),
        ]
    }
}

#[derive(Debug, Copy, Clone)]
struct LineParams {
    /// Line width.
    pub width: f64,
    /// Whether to include the squares at the endpoints of this line.
    pub include_endpoints: bool,
    /// The axis this line is along.
    pub axis: Axis,
}

#[derive(Debug, Copy, Clone)]
struct RectHighlightParams {
    pub fill: bool,
    pub crosshairs: bool,
}
