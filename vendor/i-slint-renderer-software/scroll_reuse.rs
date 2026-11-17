// Copyright (c) Orca contributors
// SPDX-License-Identifier: MIT

use super::*;
use alloc::boxed::Box;
use i_slint_core::item_tree::{ItemWeak, ParentItemTraversalMode};
use i_slint_core::properties::{PropertyTracker, evaluate_no_tracking};

#[derive(Default)]
pub(super) struct Cache {
    pub entry: Option<Entry>,
    checked: Option<ItemWeak>,
    clear: bool,
}
pub(super) struct Entry {
    item: ItemWeak,
    viewport: PhysicalRect,
    offset: euclid::Vector2D<f32, PhysicalPx>,
    scale: f32,
    size: LogicalSize,
    eligible: bool,
    tracker: Pin<Box<PropertyTracker>>,
    regions: Vec<PhysicalRect>,
    proofs: Vec<OpaqueProof>,
}
pub(super) struct OpaqueProof {
    image: SharedImageBuffer,
    inner: Option<PhysicalRect>,
    used: bool,
}
#[derive(Default)]
pub(super) struct Capture {
    pub proofs: Vec<OpaqueProof>,
    pub regions: Vec<PhysicalRect>,
}

impl Cache {
    pub fn component_destroyed(&mut self, component: i_slint_core::item_tree::ItemTreeRef) {
        let Some(entry) = self.entry.as_mut() else {
            return;
        };
        let Some(item) = entry.item.upgrade() else {
            self.entry = None;
            return;
        };
        let owner_ptr = &(**item.item_tree()) as *const _;
        let component_ptr = i_slint_core::item_tree::ItemTreeRef::as_ptr(component)
            .cast()
            .as_ptr();
        if owner_ptr == component_ptr {
            self.entry = None;
        } else {
            // Repaint after component removal, retaining opacity checks for surviving covers.
            entry.eligible = false;
        }
    }
}

impl Capture {
    pub fn image(&mut self, args: &target_pixel_buffer::DrawTextureArgs, clip: PhysicalRect) {
        if self.regions.len() >= 64
            || args.alpha != 255
            || args.colorize.is_some()
            || args.tiling.is_some()
            || args.rotation != RenderingRotation::NoRotation
        {
            return;
        }
        let target_pixel_buffer::TextureDataContainer::Shared {
            buffer: SharedBufferData::SharedImage(image),
            source_rect,
        } = &args.data
        else {
            return;
        };
        if args.dst_width != source_rect.width() as usize
            || args.dst_height != source_rect.height() as usize
        {
            return;
        }
        let index = if let Some(index) = self.proofs.iter().position(|p| p.image == *image) {
            index
        } else {
            if self.proofs.len() >= 64 {
                return;
            }
            let size = image.size();
            if size.width > i16::MAX as u32 || size.height > i16::MAX as u32 {
                return;
            }
            let full =
                PhysicalRect::from_size(PhysicalSize::new(size.width as i16, size.height as i16));
            let inner = match image {
                SharedImageBuffer::RGB8(_) => Some(full),
                SharedImageBuffer::RGBA8(buffer)
                | SharedImageBuffer::RGBA8Premultiplied(buffer) => {
                    let margin =
                        ((full.width().min(full.height()) as i32 * 5 + 99) / 100).max(1) as i16;
                    let inner = full.inflate(-margin, -margin);
                    (!inner.is_empty()
                        && inner.y_range().all(|y| {
                            buffer.as_slice()[y as usize * buffer.width() as usize
                                + inner.min_x() as usize
                                ..y as usize * buffer.width() as usize + inner.max_x() as usize]
                                .iter()
                                .all(|p| p.a == 255)
                        }))
                    .then_some(inner)
                }
            };
            self.proofs.push(OpaqueProof {
                image: image.clone(),
                inner,
                used: true,
            });
            self.proofs.len() - 1
        };
        self.proofs[index].used = true;
        let Some(inner) = self.proofs[index]
            .inner
            .and_then(|r| r.intersection(source_rect))
        else {
            return;
        };
        let Some(target) = inner
            .cast::<i32>()
            .translate(euclid::vec2(
                args.dst_x as i32 - source_rect.min_x() as i32,
                args.dst_y as i32 - source_rect.min_y() as i32,
            ))
            .intersection(&clip.cast::<i32>())
        else {
            return;
        };
        let target = target.cast::<i16>();
        if !self.regions.iter().any(|old| old.intersects(&target)) {
            self.regions.push(target);
        }
    }
}

fn viewport(item: &ItemRc, size: LogicalSize) -> Option<LogicalRect> {
    let mut clip = LogicalRect::from_size(size).intersection(&LogicalRect::new(
        item.map_to_window(item.geometry().origin),
        item.geometry().size,
    ))?;
    let mut parent = Some(item.clone());
    while let Some(node) = parent {
        let node_ref = node.borrow();
        if i_slint_core::items::ItemRef::downcast_pin::<i_slint_core::items::Transform>(node_ref)
            .is_some()
        {
            return None;
        }
        if let Some(opacity) =
            i_slint_core::items::ItemRef::downcast_pin::<i_slint_core::items::Opacity>(node_ref)
        {
            if opacity.opacity() != 1.0 {
                return None;
            }
        }
        if node_ref.as_ref().clips_children() {
            clip = clip.intersection(&LogicalRect::new(
                node.map_to_window(node.geometry().origin),
                node.geometry().size,
            ))?;
        }
        parent = node.parent_item(ParentItemTraversalMode::FindAllParents);
    }
    Some(clip)
}

pub(super) fn prepare<B: target_pixel_buffer::TargetPixelBuffer>(
    cache: &RefCell<Cache>,
    renderer: &mut SceneBuilder<'_, RenderToBuffer<'_, B>>,
    components: &[(ItemTreeWeak, LogicalPoint)],
    post_render: &dyn Fn(&mut dyn ItemRenderer),
    screen_size: LogicalSize,
) -> Vec<PhysicalRect> {
    evaluate_no_tracking(|| {
        let mut cache = cache.borrow_mut();
        cache.checked = None;
        cache.clear = false;
        let was_eligible = cache.entry.as_ref()?.eligible;
        cache.entry.as_mut().unwrap().eligible = false;
        let entry = cache.entry.as_ref()?;
        let item = entry.item.upgrade()?;
        let item_ref = item.borrow();
        let layer =
            i_slint_core::items::ItemRef::downcast_pin::<i_slint_core::items::Layer>(item_ref)?;
        if !layer.cache_rendering_hint()
            || components.len() != 1
            || !renderer.processor.buffer.can_copy_rect()
        {
            return None;
        }
        let factor = renderer.scale_factor;
        let offset = item
            .map_to_window(item.geometry().origin)
            .to_vector()
            .cast::<f32>()
            * factor;
        let logical_viewport = viewport(&item, screen_size)?;
        let visible = (logical_viewport.cast() * factor).round().cast::<i16>();
        let mut probe = SceneBuilder::new(
            renderer.rotation.screen_size,
            factor,
            renderer.window,
            Occlusion(false),
            RenderingRotation::NoRotation,
            #[cfg(feature = "systemfonts")]
            renderer.text_layout_cache,
        );
        probe.current_state.clip = logical_viewport;
        probe.occlusion_after = Some(item.clone());
        for (component, origin) in components {
            let component = ItemTreeWeak::upgrade(component)?;
            i_slint_core::item_rendering::render_component_items(
                &component,
                &mut probe,
                *origin,
                &renderer.window.window_adapter(),
            );
        }
        post_render(&mut probe);
        let clear = probe.occlusion_seen && !probe.processor.0;
        let same = was_eligible
            && clear
            && entry.scale == factor.get()
            && entry.size == item.geometry().size
            && entry.viewport == visible
            && !entry.tracker.is_dirty();
        let delta = offset - entry.offset;
        let zones = entry.regions.clone();
        cache.checked = Some(item.downgrade());
        cache.clear = clear;
        if !same
            || delta.x.abs() > 0.0001
            || (delta.y - delta.y.round()).abs() > 0.0001
            || delta.y.abs() >= visible.height() as f32
        {
            return None;
        }
        let dy = delta.y.round() as i16;
        // Stationary artwork updates must repaint instead of retaining old pixels.
        if dy == 0 {
            return None;
        }
        let mut retained: Vec<_> = zones
            .into_iter()
            .filter_map(|zone| zone.translate(euclid::vec2(0, dy)).intersection(&visible))
            .collect();
        retained.sort_unstable_by_key(|r| (r.min_y(), r.min_x()));
        if dy > 0 {
            retained.reverse();
        }
        for rect in &retained {
            renderer
                .processor
                .buffer
                .copy_rect(rect.translate(euclid::vec2(0, -dy)), dy);
        }
        Some(retained)
    })
    .unwrap_or_default()
}

impl<T: ProcessScene> SceneBuilder<'_, T> {
    pub(super) fn track_scroll_layer(&mut self, item: &ItemRc, size: LogicalSize) -> bool {
        let Some(cache) = self.scroll_cache else {
            return false;
        };
        if self.rotation.orientation != RenderingRotation::NoRotation
            || self.current_state.alpha != 1.0
        {
            return false;
        }
        let Some(clip) = self
            .current_state
            .clip
            .intersection(&LogicalRect::from_size(size))
        else {
            return true;
        };
        let factor = self.scale_factor;
        let visible = (clip.translate(self.current_state.offset.to_vector()).cast() * factor)
            .round()
            .cast::<i16>();
        if visible.is_empty() {
            return true;
        }
        let mut state = cache.borrow_mut();
        let eligible = state.checked.as_ref() == Some(&item.downgrade()) && state.clear;
        let old = state.entry.take();
        drop(state);
        let mut entry = old
            .filter(|entry| entry.item.upgrade().as_ref() == Some(item))
            .unwrap_or_else(|| Entry {
                item: item.downgrade(),
                viewport: visible,
                offset: Default::default(),
                scale: factor.get(),
                size,
                eligible: false,
                tracker: Box::pin(PropertyTracker::default()),
                regions: Vec::new(),
                proofs: Vec::new(),
            });
        for proof in &mut entry.proofs {
            proof.used = false;
        }
        self.processor
            .start_scroll_capture(core::mem::take(&mut entry.proofs));
        let adapter = self.window.window_adapter();
        self.scroll_cache = None;
        entry.tracker.as_ref().evaluate(|| {
            i_slint_core::item_rendering::render_item_children(
                self,
                item.item_tree(),
                item.index() as isize,
                &adapter,
            )
        });
        self.scroll_cache = Some(cache);
        let mut capture = self.processor.finish_scroll_capture();
        capture.proofs.retain(|proof| proof.used);
        entry.proofs = capture.proofs;
        entry.regions = capture
            .regions
            .into_iter()
            .filter_map(|rect| rect.intersection(&visible))
            .collect();
        entry.viewport = visible;
        entry.offset = self.current_state.offset.to_vector().cast::<f32>() * factor;
        entry.scale = factor.get();
        entry.size = size;
        entry.eligible = eligible;
        cache.borrow_mut().entry = Some(entry);
        true
    }
}

struct Occlusion(bool);
impl ProcessScene for Occlusion {
    fn process_scene_texture(&mut self, _: PhysicalRect, t: SceneTexture<'static>) {
        self.0 |= t.extra.alpha > 0;
    }
    fn process_target_texture(
        &mut self,
        _: &target_pixel_buffer::DrawTextureArgs,
        _: PhysicalRect,
    ) {
        self.0 = true;
    }
    fn process_rectangle(
        &mut self,
        args: &target_pixel_buffer::DrawRectangleArgs,
        clip: PhysicalRect,
    ) {
        process_rectangle_impl(self, args, &clip, ScaleFactor::new(1.0));
    }
    fn process_simple_rectangle(&mut self, _: PhysicalRect, c: PremultipliedRgbaColor) {
        self.0 |= c.alpha > 0;
    }
    fn process_rounded_rectangle(&mut self, _: PhysicalRect, _: RoundedRectangle) {
        self.0 = true;
    }
    fn process_linear_gradient(&mut self, _: PhysicalRect, _: LinearGradientCommand) {
        self.0 = true;
    }
    fn process_radial_gradient(&mut self, _: PhysicalRect, _: RadialGradientCommand) {
        self.0 = true;
    }
    fn process_conic_gradient(&mut self, _: PhysicalRect, _: ConicGradientCommand) {
        self.0 = true;
    }
    #[cfg(feature = "path")]
    fn process_filled_path(
        &mut self,
        _: PhysicalRect,
        _: PhysicalRect,
        _: Vec<path::Command>,
        _: PremultipliedRgbaColor,
    ) {
        self.0 = true;
    }
    #[cfg(feature = "path")]
    fn process_stroked_path(
        &mut self,
        _: PhysicalRect,
        _: PhysicalRect,
        _: Vec<path::Command>,
        _: PremultipliedRgbaColor,
        _: f32,
        _: i_slint_core::items::LineCap,
        _: i_slint_core::items::LineJoin,
        _: f32,
    ) {
        self.0 = true;
    }
}
