#![allow(missing_docs)]

use std::rc::Rc;

use super::*;

// See super::quad_impl for the actual implementation of these traits on these
// enums.

/// A 2D quadtree interface for an N-dimensional automaton.
#[derive(Debug, Clone)]
pub enum QuadTreeAutomaton<C: CellType> {
    Automaton1D(NdAutomaton<C, Dim1D, NdProjectionInfo2D<Dim1D>>),
    Automaton2D(NdAutomaton<C, Dim2D, NdProjectionInfo2D<Dim2D>>),
    Automaton3D(NdAutomaton<C, Dim3D, NdProjectionInfo2D<Dim3D>>),
    Automaton4D(NdAutomaton<C, Dim4D, NdProjectionInfo2D<Dim4D>>),
    Automaton5D(NdAutomaton<C, Dim5D, NdProjectionInfo2D<Dim5D>>),
    Automaton6D(NdAutomaton<C, Dim6D, NdProjectionInfo2D<Dim6D>>),
}

/// A 2D quadtree interface for an N-dimensional NdTreeSlice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuadTreeSlice<C: CellType> {
    Slice1D(NdProjectedTreeSlice<C, Dim1D, NdProjectionInfo2D<Dim1D>>),
    Slice2D(NdProjectedTreeSlice<C, Dim2D, NdProjectionInfo2D<Dim2D>>),
    Slice3D(NdProjectedTreeSlice<C, Dim3D, NdProjectionInfo2D<Dim3D>>),
    Slice4D(NdProjectedTreeSlice<C, Dim4D, NdProjectionInfo2D<Dim4D>>),
    Slice5D(NdProjectedTreeSlice<C, Dim5D, NdProjectionInfo2D<Dim5D>>),
    Slice6D(NdProjectedTreeSlice<C, Dim6D, NdProjectionInfo2D<Dim6D>>),
}

/// A 2D quadtree interface for an N-dimensional NdTreeCachedNode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuadTreeNode<C: CellType> {
    Node1D(NdProjectedTreeNode<C, Dim1D, NdProjectionInfo2D<Dim1D>>),
    Node2D(NdProjectedTreeNode<C, Dim2D, NdProjectionInfo2D<Dim2D>>),
    Node3D(NdProjectedTreeNode<C, Dim3D, NdProjectionInfo2D<Dim3D>>),
    Node4D(NdProjectedTreeNode<C, Dim4D, NdProjectionInfo2D<Dim4D>>),
    Node5D(NdProjectedTreeNode<C, Dim5D, NdProjectionInfo2D<Dim5D>>),
    Node6D(NdProjectedTreeNode<C, Dim6D, NdProjectionInfo2D<Dim6D>>),
}

/// A single branch of a quadtree node, with global positional information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuadTreeSliceBranch<C: CellType> {
    Leaf(C, Vec2D),
    Node(QuadTreeSlice<C>),
}

/// A single branch of quadtree node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuadTreeBranch<C: CellType> {
    Leaf(C),
    Node(QuadTreeNode<C>),
}

/// Anything that can act as a mutable quadtree of cells.
pub trait QuadTreeAutomatonTrait<C: CellType>: NdSimulate {
    fn slice(&self) -> QuadTreeSlice<C>;
    fn get_root(&self) -> QuadTreeNode<C>;
    fn get_cell(&self, pos: Vec2D) -> C;
    fn set_view_pos_on_axis(&mut self, axis: Axis, pos: isize);
    fn set_display_axes(&mut self, horizontal: Axis, vertical: Axis) -> Result<(), ()>;
    fn set_cell(&mut self, pos: Vec2D, new_state: C);
    fn get_slice_containing(&mut self, rect: Rect2D) -> QuadTreeSlice<C>;
    fn expand_to(&mut self, pos: Vec2D);
    fn shrink(&mut self);
}

/// Anything that can act as an immutable quadtree of cells.
pub trait QuadTreeSliceTrait<C: CellType> {
    fn get_root(&self) -> QuadTreeNode<C>;
    fn get_cell(&self, pos: Vec2D) -> Option<C>;
    fn get_rect(&self) -> Rect2D;
    fn get_branch(&self, branch_idx: usize) -> QuadTreeSliceBranch<C>;
    fn get_branches(&self) -> [QuadTreeSliceBranch<C>; 4];
}

/// Anything that can act as an immutable node in a quadtree of cells.
pub trait QuadTreeNodeTrait<C: CellType> {
    fn get_cell(&self, pos: Vec2D) -> C;
    fn get_layer(&self) -> usize;
    fn get_branch(&self, branch_idx: usize) -> QuadTreeBranch<C>;
    fn get_branches(&self) -> [QuadTreeBranch<C>; 4];
    fn get_population(&self) -> usize;
}

// Automaton implemention.
impl<C: CellType, D: Dim> QuadTreeAutomatonTrait<C> for NdAutomaton<C, D, NdProjectionInfo2D<D>>
where
    QuadTreeSlice<C>: From<NdProjectedTreeSlice<C, D, NdProjectionInfo2D<D>>>,
{
    fn slice(&self) -> QuadTreeSlice<C> {
        self.nd_slice().into()
    }
    fn get_root(&self) -> QuadTreeNode<C> {
        self.slice().get_root().into()
    }
    fn get_cell(&self, pos: Vec2D) -> C {
        self.slice().get_cell(pos).unwrap_or_default()
    }
    fn set_view_pos_on_axis(&mut self, axis: Axis, coordinate: isize) {
        let mut slice_pos = self.projection_info.slice_pos;
        slice_pos[axis] = coordinate;
        self.projection_info = Rc::new(self.projection_info.with_slice_pos(slice_pos));
    }
    fn set_display_axes(&mut self, horizontal: Axis, vertical: Axis) -> Result<(), ()> {
        match self.projection_info.with_display_axes(horizontal, vertical) {
            Ok(new_projection_info) => {
                self.projection_info = Rc::new(new_projection_info);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
    fn set_cell(&mut self, pos: Vec2D, new_state: C) {
        self.tree
            .set_cell(self.projection_info.pdim_to_ndim(pos), new_state);
    }
    fn get_slice_containing(&mut self, rect: Rect2D) -> QuadTreeSlice<C> {
        let ndrect = NdRect::span(
            self.projection_info.pdim_to_ndim(rect.min()),
            self.projection_info.pdim_to_ndim(rect.max()),
        );
        NdProjectedTreeSlice {
            slice: self.tree.get_slice_containing(ndrect),
            projection_info: self.projection_info.clone(),
        }
        .into()
    }
    fn expand_to(&mut self, pos: Vec2D) {
        self.tree.expand_to(self.projection_info.pdim_to_ndim(pos));
    }
    fn shrink(&mut self) {
        self.tree.shrink();
    }
}

// Slice implementation.
impl<C: CellType, D: Dim> QuadTreeSliceTrait<C>
    for NdProjectedTreeSlice<C, D, NdProjectionInfo2D<D>>
where
    QuadTreeNode<C>: From<NdProjectedTreeNode<C, D, NdProjectionInfo2D<D>>>,
    QuadTreeSlice<C>: From<NdProjectedTreeSlice<C, D, NdProjectionInfo2D<D>>>,
{
    fn get_root(&self) -> QuadTreeNode<C> {
        NdProjectedTreeNode {
            node: self.slice.root.clone(),
            projection_info: self.projection_info.clone(),
        }
        .into()
    }
    fn get_cell(&self, pos: Vec2D) -> Option<C> {
        self.slice.get_cell(self.projection_info.pdim_to_ndim(pos))
    }
    fn get_rect(&self) -> Rect2D {
        let ndrect = self.slice.rect();
        Rect2D::span(
            self.projection_info.ndim_to_pdim(ndrect.min()),
            self.projection_info.ndim_to_pdim(ndrect.max()),
        )
    }
    fn get_branch(&self, branch_idx: usize) -> QuadTreeSliceBranch<C> {
        match self.slice.get_branch(branch_idx) {
            NdTreeSliceBranch::Leaf(cell_state, pos) => {
                QuadTreeSliceBranch::Leaf(cell_state, self.projection_info.ndim_to_pdim(pos))
            }
            NdTreeSliceBranch::Node(slice) => QuadTreeSliceBranch::Node(
                NdProjectedTreeSlice {
                    slice,
                    projection_info: self.projection_info.clone(),
                }
                .into(),
            ),
        }
    }
    fn get_branches(&self) -> [QuadTreeSliceBranch<C>; 4] {
        [
            self.get_branch(0),
            self.get_branch(1),
            self.get_branch(2),
            self.get_branch(3),
        ]
    }
}

// Node implementation.
impl<C: CellType, D: Dim> QuadTreeNodeTrait<C> for NdProjectedTreeNode<C, D, NdProjectionInfo2D<D>>
where
    QuadTreeNode<C>: From<NdProjectedTreeNode<C, D, NdProjectionInfo2D<D>>>,
{
    fn get_cell(&self, pos: Vec2D) -> C {
        self.node.get_cell(self.projection_info.pdim_to_ndim(pos))
    }
    fn get_layer(&self) -> usize {
        self.node.layer
    }
    fn get_branch(&self, branch_idx: usize) -> QuadTreeBranch<C> {
        // Now that we have the real N-dimensional branch index, convert the NdTreeBranch into a QuadTreeBranch.
        let nd_branch_idx = self
            .projection_info
            .get_ndim_branch_idx(self.get_layer(), branch_idx);
        match &self.node.branches[nd_branch_idx] {
            NdTreeBranch::Leaf(cell_state) => QuadTreeBranch::Leaf(*cell_state),
            NdTreeBranch::Node(node) => QuadTreeBranch::Node(
                NdProjectedTreeNode {
                    node: node.clone(),
                    projection_info: self.projection_info.clone(),
                }
                .into(),
            ),
        }
    }
    fn get_branches(&self) -> [QuadTreeBranch<C>; 4] {
        [
            self.get_branch(0),
            self.get_branch(1),
            self.get_branch(2),
            self.get_branch(3),
        ]
    }
    fn get_population(&self) -> usize {
        self.node.population
    }
}