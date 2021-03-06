# Changelog (`ndcell_core`)

All notable changes to `ndcell_core` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `NdVec` methods `min_component()` and `max_component()`
- `NdRect` method `span_rects()`
- Re-export of `num` modules `cast`, `iter`, and `pow` from `crate::num`

### Changed

- `FlatNdTree` now uses index `0` for all empty nodes, regardless of layer
- The closure passed to `FlatNdTree::to_node()` now takes an `Option`; if the argument is `None`, it must return an empty node at `min_layer`
- The `FixedPoint` methods `floor()`, `ceil()`, and `round()` now return `BigInt` instead of `(BigInt, f64)`
- The `FixedVec` methods `floor()` and `ceil()` now return `BigVec` instead of `(BigVec, FVec)`
- The `NdVec` methods `min_axis()` and `max_axis()` now longer take a closure; the identity function is used instead
- Renamed the `NdTree` methods `offset` and `set_offset()` to `base_pos()` and `set_base_pos()`
- Renamed the `NdTree` methods `center` and `set_center()` to `center_pos()` and `set_center_pos()`
- Renamed the `NdTreeSlice` field `offset` to `base_pos`

### Removed

- `crate::math` module, including `math::try_pow_2()` and `math::bresenham()`

## [0.1.0] - 2020-12-17

### Added

- Initial release
