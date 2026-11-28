//! Retain bitmap operation order without keeping pixels a complete copy replaces.
use super::{ImageCompositeLayer, ImageCompositeMath, ImageInput};
use crate::region_map::clip_region_copy;
use alloc::{boxed::Box, vec};

impl ImageInput {
    /// Append an immutable bitmap operation. A copy reads only its clipped
    /// source region. Full-opacity RGBA copies also discard earlier writes
    /// wholly covered by the replacement.
    pub fn with_composite_layer(self, mut layer: ImageCompositeLayer) -> Self {
        let (Some(size), Some(source_size)) = (extent(&self), extent(&layer.input)) else {
            return append(self, layer);
        };
        // This copy owns its source recipe. Writes outside the sampled source
        // rectangle cannot affect it, even when the original layer still
        // needs those pixels in future reads.
        let copied = if matches!(layer.math, ImageCompositeMath::Copy) {
            clipped_layer(&layer, size).ok().flatten()
        } else {
            None
        };
        if let Some((sample, _)) = copied
            && matches!(*layer.input, Self::Composite { .. })
        {
            layer.input = Box::new(retain_sampled_writes(*layer.input, sample));
        }
        let [sx, sy, width, height] = layer.source;
        let [dx, dy] = layer.destination;
        let full_copy = matches!(layer.math, ImageCompositeMath::Copy)
            && layer.opacity == 255
            && dx <= 0
            && dy <= 0
            && i64::from(dx) + i64::from(width) >= i64::from(size[0])
            && i64::from(dy) + i64::from(height) >= i64::from(size[1])
            && sx >= 0
            && sy >= 0
            && width > 0
            && height > 0
            && i64::from(sx) + i64::from(width) <= i64::from(source_size[0])
            && i64::from(sy) + i64::from(height) <= i64::from(source_size[1])
            && layer.clip.is_none_or(|[x, y, w, h]| {
                x <= 0
                    && y <= 0
                    && i64::from(x) + i64::from(w) >= i64::from(size[0])
                    && i64::from(y) + i64::from(h) >= i64::from(size[1])
            });
        if full_copy {
            if layer.source == [0, 0, size[0] as i32, size[1] as i32]
                && layer.destination == [0, 0]
                && source_size == size
            {
                return *layer.input;
            }
            // A copied atlas cell can cover the entire destination too. Keep
            // its original coordinates but release the overwritten history.
            return Self::Composite {
                base: Box::new(Self::Solid {
                    size,
                    color: [0; 4],
                }),
                layers: vec![layer],
            };
        }
        let base = match copied {
            Some(([_, _, w, h], [x, y])) if layer.opacity == 255 => {
                discard_overwritten_writes(self, size, [x, y, w, h])
            }
            _ => self,
        };
        append(base, layer)
    }
}

type Rect = [i64; 4];

fn clipped_layer(
    layer: &ImageCompositeLayer,
    size: [u32; 2],
) -> crate::Result<Option<(Rect, [i64; 2])>> {
    clip_region_copy(
        extent(&layer.input),
        size,
        layer.source.map(i64::from),
        layer.destination.map(i64::from),
        layer.clip.map(|clip| clip.map(i64::from)),
    )
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    (right > x && bottom > y).then_some([x, y, right - x, bottom - y])
}

fn discard_overwritten_writes(input: ImageInput, size: [u32; 2], covered: Rect) -> ImageInput {
    let ImageInput::Composite { base, mut layers } = input else {
        return input;
    };
    layers.retain(|layer| {
        !clipped_layer(layer, size).is_ok_and(|result| {
            result.is_none_or(|([_, _, w, h], [x, y])| {
                intersect(covered, [x, y, w, h]) == Some([x, y, w, h])
            })
        })
    });
    let base = discard_overwritten_writes(*base, size, covered);
    if layers.is_empty() {
        base
    } else {
        ImageInput::Composite { base: Box::new(base), layers }
    }
}

/// Keep exactly the writes that a parent copy can sample. The source canvas
/// keeps its original extent and coordinate origin.
fn retain_sampled_writes(input: ImageInput, sampled: Rect) -> ImageInput {
    let Some(size) = extent(&input) else {
        return input;
    };
    if sampled == [0, 0, size[0].into(), size[1].into()] {
        return input;
    }
    let ImageInput::Composite { base, mut layers } = input else {
        return input;
    };
    // A complete RGBA replacement makes every earlier input irrelevant
    // within the sample, even when the write is partial on the full canvas.
    let replacement = layers.iter().rposition(|layer| {
        layer.opacity == 255
            && matches!(layer.math, ImageCompositeMath::Copy)
            && extent(&layer.input).is_some()
            && clipped_layer(layer, size).is_ok_and(|result| {
                result.is_some_and(|([_, _, w, h], [x, y])| {
                    intersect(sampled, [x, y, w, h]) == Some(sampled)
                })
            })
    });
    let base = if let Some(index) = replacement {
        layers.drain(..index);
        Box::new(ImageInput::Solid { size, color: [0; 4] })
    } else {
        Box::new(retain_sampled_writes(*base, sampled))
    };
    let layers: alloc::vec::Vec<_> = layers
        .into_iter()
        .filter_map(|mut layer| {
            let ([sx, sy, w, h], [dx, dy]) = match clipped_layer(&layer, size) {
                Ok(result) => result?,
                Err(_) => return Some(layer),
            };
            let live = intersect(sampled, [dx, dy, w, h])?;
            let [x, y, w, h] = live;
            if matches!(*layer.input, ImageInput::Composite { .. }) {
                layer.input = Box::new(retain_sampled_writes(
                    *layer.input,
                    [sx + x - dx, sy + y - dy, w, h],
                ));
            }
            layer.clip = Some(live.map(|v| v as i32));
            Some(layer)
        })
        .collect();
    if layers.is_empty() {
        *base
    } else {
        ImageInput::Composite { base, layers }
    }
}

/// Largest operation graph the shared image worker admits (nodes, nesting).
/// Adapters that retain a layer's history as a graph must not keep one past
/// this bound: it can never be evaluated, and repeated self-copies would grow
/// it geometrically.
pub const IMAGE_GRAPH_MAX_NODES: usize = 1024;
pub const IMAGE_GRAPH_MAX_DEPTH: usize = 32;

impl ImageInput {
    /// Whether the shared worker's graph admission bound holds. Stops counting
    /// at the bound, so the check is O(bound) for any graph.
    pub fn within_graph_bound(&self) -> bool {
        fn walk(input: &ImageInput, depth: usize, nodes: &mut usize) -> bool {
            *nodes += 1;
            if depth > IMAGE_GRAPH_MAX_DEPTH || *nodes > IMAGE_GRAPH_MAX_NODES {
                return false;
            }
            let d = depth + 1;
            match input {
                ImageInput::FieldWarp { source, .. } => walk(source, d, nodes),
                ImageInput::Process { input, .. } => walk(input, d, nodes),
                ImageInput::Composite { base, layers } => {
                    walk(base, d, nodes) && layers.iter().all(|l| walk(&l.input, d, nodes))
                }
                ImageInput::Raster { destination, source } => walk(destination, d, nodes) && walk(source, d, nodes),
                ImageInput::Ternary { destination, first, second } => {
                    walk(destination, d, nodes) && walk(first, d, nodes) && walk(second, d, nodes)
                }
                ImageInput::ParticlePlane { .. }
                | ImageInput::PyramidLevel { .. }
                | ImageInput::Encoded { .. }
                | ImageInput::Solid { .. }
                | ImageInput::Transition { .. }
                | ImageInput::Asset { .. }
                | ImageInput::ScopedAsset { .. }
                | ImageInput::Resource(_) => true,
            }
        }
        walk(self, 0, &mut 0)
    }
}

fn append(base: ImageInput, layer: ImageCompositeLayer) -> ImageInput {
    match base {
        ImageInput::Composite { base, mut layers } if layers.len() < 64 => {
            layers.push(layer);
            ImageInput::Composite { base, layers }
        }
        base => ImageInput::Composite {
            base: Box::new(base),
            layers: vec![layer],
        },
    }
}

fn extent(input: &ImageInput) -> Option<[u32; 2]> {
    match input {
        ImageInput::ParticlePlane { size, .. }
        | ImageInput::Encoded { size, .. }
        | ImageInput::Solid { size, .. }
        | ImageInput::Asset { size, .. } => Some(*size),
        ImageInput::Composite { base, .. } => extent(base),
        ImageInput::Raster { destination, .. } | ImageInput::Ternary { destination, .. } => {
            extent(destination)
        }
        ImageInput::Resource(resource) => Some(resource.size()),
        ImageInput::Transition { from, .. } => Some(from.size()),
        // New worker graph kinds may change dimensions. Conservatively keep
        // their history rather than interpreting effect programs in core.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn solid(color: u8) -> ImageInput {
        ImageInput::Solid {
            size: [2, 2],
            color: [color; 4],
        }
    }
    fn copy(input: ImageInput) -> ImageCompositeLayer {
        ImageCompositeLayer {
            input: Box::new(input),
            source: [0, 0, 2, 2],
            destination: [0, 0],
            clip: None,
            opacity: 255,
            math: ImageCompositeMath::Copy,
        }
    }
    #[test]
    fn worker_effect_graph_keeps_history_without_guessing_its_output_size() {
        let source = ImageInput::Process {
            input: Box::new(solid(2)),
            effects: alloc::sync::Arc::from(&b"[]"[..]),
        };
        let result = solid(1).with_composite_layer(copy(source));
        let ImageInput::Composite { base, .. } = result else {
            panic!("lost destination history")
        };
        assert!(matches!(
            *base,
            ImageInput::Solid {
                color: [1, 1, 1, 1],
                ..
            }
        ));
    }

    #[test]
    fn unoptimized_self_copies_leave_the_graph_bound() {
        // A partial copy of a graph onto itself duplicates its history in
        // both branches, doubling the node count each time.
        let mut a = solid(1);
        let mut steps = 0;
        while a.within_graph_bound() {
            let mut layer = copy(a.clone());
            layer.source = [0, 0, 1, 1];
            a = ImageInput::Composite { base: Box::new(a), layers: vec![layer] };
            steps += 1;
            assert!(steps < 64, "bound never reached");
        }
        assert!(steps <= 11);
    }

    #[test]
    fn repeated_partial_copies_keep_only_the_latest_source_in_the_sample() {
        let mut a = solid(1);
        for color in 2..48 {
            let mut layer = copy(solid(color));
            layer.source = [0, 0, 1, 1];
            a = a.with_composite_layer(layer);
        }
        let mut layer = copy(a.clone());
        layer.source = [0, 0, 1, 1];
        let ImageInput::Composite { layers, .. } = a else { panic!() };
        assert_eq!(layers.len(), 1);
        let ImageInput::Composite { layers, .. } = solid(0).with_composite_layer(layer)
        else { panic!() };
        let ImageInput::Composite { base, layers } = layers[0].input.as_ref() else { panic!() };
        assert!(matches!(**base, ImageInput::Solid { color: [0, 0, 0, 0], .. }));
        assert_eq!(layers.len(), 1);
        assert!(matches!(*layers[0].input, ImageInput::Solid { color: [47, 47, 47, 47], .. }));
    }

    #[test]
    fn repeated_whole_copies_do_not_retain_overwritten_histories() {
        let (mut a, mut b) = (solid(1), solid(2));
        for _ in 0..1000 {
            a = a.with_composite_layer(copy(b.clone()));
            b = b.with_composite_layer(copy(a.clone()));
            assert!(matches!(
                a,
                ImageInput::Solid {
                    color: [2, 2, 2, 2],
                    ..
                }
            ));
        }
    }
    #[test]
    fn partial_and_channel_only_copies_keep_destination_pixels() {
        for change in 0..5 {
            let mut layer = copy(solid(2));
            match change {
                0 => layer.math = ImageCompositeMath::CopyRgb,
                1 => layer.math = ImageCompositeMath::CopyAlpha,
                2 => layer.opacity = 254,
                3 => layer.clip = Some([0, 0, 1, 2]),
                _ => layer.source = [1, 0, 2, 2],
            }
            let result = solid(1).with_composite_layer(layer);
            let ImageInput::Composite { base, .. } = result else {
                panic!("lost destination")
            };
            assert!(matches!(
                *base,
                ImageInput::Solid {
                    color: [1, 1, 1, 1],
                    ..
                }
            ));
        }
    }
    #[test]
    fn full_atlas_copy_keeps_source_mapping_and_drops_old_base() {
        let mut layer = copy(ImageInput::Solid {
            size: [8, 8],
            color: [2; 4],
        });
        layer.source = [3, 4, 2, 2];
        let ImageInput::Composite { base, layers } = solid(1).with_composite_layer(layer) else {
            panic!()
        };
        assert!(matches!(
            *base,
            ImageInput::Solid {
                color: [0, 0, 0, 0],
                ..
            }
        ));
        assert_eq!(layers[0].source, [3, 4, 2, 2]);
    }
}
