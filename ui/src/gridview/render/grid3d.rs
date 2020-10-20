//! 3D grid rendering.
//!
//! Currently, only solid colors are supported, however I plan to add custom
//! models and maybe textures in the future.

use anyhow::Result;
use cgmath::Matrix4;
use glium::index::PrimitiveType;
use glium::Surface;

use ndcell_core::axis::{X, Y, Z};
use ndcell_core::prelude::*;

use super::shaders;
use crate::gridview::*;
use crate::DISPLAY;

/// Number of cubes to render in each render batch.
const CUBE_BATCH_SIZE: usize = 256;

#[derive(Default)]
pub struct RenderCache {}

pub struct RenderInProgress<'a> {
    octree: NdTree3D,
    /// Camera to render the scene from.
    camera: &'a Camera3D,
    /// Target to render to.
    target: &'a mut glium::Frame,
    /// Transform from `visible_octree` space (1 unit = 1 render cell; (0, 0) =
    /// bottom left) to screen space ((-1, -1) = bottom left; (1, 1) = top
    /// right) and pixel space (1 unit = 1 pixel; (0, 0) = top left).
    transform: CellTransform3D,
}
impl<'a> RenderInProgress<'a> {
    pub fn new(
        g: &'a GridView3D,
        RenderParams { target, config: _ }: RenderParams<'a>,
        _node_cache: &'a NodeCache<Dim3D>,
    ) -> Result<Self> {
        target.clear_depth(f32::INFINITY);
        let camera = g.camera();
        let transform = camera.cell_transform_with_base(BigVec3D::origin())?;

        Ok(Self {
            octree: g.automaton.projected_tree(),
            camera,
            target,
            transform,
        })
    }

    pub fn cell_transform(&self) -> &CellTransform3D {
        &self.transform
    }

    pub fn draw_cells(&mut self) {
        let rainbow_cube_matrix: [[f32; 4]; 4] = self.transform.gl_matrix();
        let cam_pos = self.camera.pos().floor().0;
        let selection_cube_matrix: [[f32; 4]; 4] = (self.transform.projection_transform
            * self.transform.render_cell_transform
            * Matrix4::from_translation(cgmath::vec3(
                cam_pos[X].to_f32().unwrap(),
                cam_pos[Y].to_f32().unwrap(),
                cam_pos[Z].to_f32().unwrap(),
            )))
        .into();

        #[derive(Debug, Copy, Clone)]
        struct Vert {
            pos: [f32; 3],
            color: [f32; 4],
        };
        implement_vertex!(Vert, pos, color);

        let rainbow_cube_verts: Vec<_> = vec![
            ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]),
            ([0.0, 1.0, 0.0], [0.0, 1.0, 0.0, 1.0]),
            ([0.0, 1.0, 1.0], [0.0, 1.0, 1.0, 1.0]),
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0]),
            ([1.0, 0.0, 1.0], [1.0, 0.0, 1.0, 1.0]),
            ([1.0, 1.0, 0.0], [1.0, 1.0, 0.0, 1.0]),
            ([1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
        ]
        .into_iter()
        .map(|(pos, color)| Vert { pos, color })
        .collect();

        let selection_cube_verts: Vec<_> = vec![
            ([-0.1, -0.1, -0.1], [1.0, 1.0, 1.0, 0.5]),
            ([-0.1, -0.1, 1.1], [1.0, 1.0, 1.0, 0.5]),
            ([-0.1, 1.1, -0.1], [1.0, 1.0, 1.0, 0.5]),
            ([-0.1, 1.1, 1.1], [1.0, 1.0, 1.0, 0.5]),
            ([1.1, -0.1, -0.1], [1.0, 1.0, 1.0, 0.5]),
            ([1.1, -0.1, 1.1], [1.0, 1.0, 1.0, 0.5]),
            ([1.1, 1.1, -0.1], [1.0, 1.0, 1.0, 0.5]),
            ([1.1, 1.1, 1.1], [1.0, 1.0, 1.0, 0.5]),
        ]
        .into_iter()
        .map(|(pos, color)| Vert { pos, color })
        .collect();

        let cube_vbo = glium::VertexBuffer::new(&**DISPLAY, &rainbow_cube_verts).unwrap();
        let cube_ibo = glium::IndexBuffer::new(
            &**DISPLAY,
            PrimitiveType::TrianglesList,
            &[
                1, 2, 3, 2, 1, 0, // x-
                7, 6, 5, 4, 5, 6, // x+
                0, 1, 4, 5, 4, 1, // y-
                6, 3, 2, 3, 6, 7, // y+
                2, 4, 6, 4, 2, 0, // z-
                7, 5, 3, 1, 3, 5_u16, // z+
            ],
        )
        .unwrap();
        self.target.clear_color_srgb(0.5, 0.5, 0.5, 1.0);
        self.target
            .draw(
                &cube_vbo,
                &cube_ibo,
                &shaders::RGBA,
                &uniform! {
                    matrix: rainbow_cube_matrix,
                },
                &glium::DrawParameters {
                    depth: glium::Depth {
                        test: glium::DepthTest::IfLessOrEqual,
                        write: true,
                        ..glium::Depth::default()
                    },
                    backface_culling: glium::BackfaceCullingMode::CullClockwise,
                    smooth: Some(glium::Smooth::Nicest),
                    ..Default::default()
                },
            )
            .expect("Failed to draw cube");

        cube_vbo.write(&selection_cube_verts);
        self.target
            .draw(
                &cube_vbo,
                &cube_ibo,
                &shaders::RGBA,
                &uniform! {
                    matrix: selection_cube_matrix,
                },
                &glium::DrawParameters {
                    depth: glium::Depth {
                        test: glium::DepthTest::IfLessOrEqual,
                        write: true,
                        ..glium::Depth::default()
                    },
                    blend: glium::Blend::alpha_blending(),
                    backface_culling: glium::BackfaceCullingMode::CullClockwise,
                    smooth: Some(glium::Smooth::Nicest),
                    ..Default::default()
                },
            )
            .expect("Failed to draw cube");
    }
}