// src/spatial.rs
//! Spatial Grid — uniform grid for O(1) spatial queries

use crate::archetype::EntityId;
use crate::entity::EntityHandle;
use crate::interning::InternedStr;
use crate::storage::FieldIndex;
use crate::world::World;

#[derive(Clone, Debug)]
pub struct SpatialConfig {
    pub cell_size: f64,
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl SpatialConfig {
    pub fn new(cell_size: f64, min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self { cell_size, min_x, min_y, max_x, max_y }
    }

    fn cols(&self) -> usize { ((self.max_x - self.min_x) / self.cell_size).ceil().max(1.0) as usize }
    fn rows(&self) -> usize { ((self.max_y - self.min_y) / self.cell_size).ceil().max(1.0) as usize }
    fn total_cells(&self) -> usize { self.cols() * self.rows() }

    fn pos_to_cell(&self, x: f64, y: f64) -> (i32, i32) {
        let cx = ((x - self.min_x) / self.cell_size).floor() as i32;
        let cy = ((y - self.min_y) / self.cell_size).floor() as i32;
        (cx.max(0).min(self.cols() as i32 - 1), cy.max(0).min(self.rows() as i32 - 1))
    }

    fn cell_index(&self, cx: i32, cy: i32) -> usize {
        let cols = self.cols() as i32;
        if cx < 0 || cy < 0 || cx >= cols || cy >= self.rows() as i32 { return usize::MAX; }
        (cy as usize) * (cols as usize) + (cx as usize)
    }
}

impl Default for SpatialConfig {
    fn default() -> Self {
        Self::new(50.0, 0.0, 0.0, 1000.0, 1000.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SpatialEntry {
    pub entity_index: u32,
    pub x: f64,
    pub y: f64,
}

pub struct SpatialGrid {
    config: SpatialConfig,
    cell_ranges: Vec<(u32, u32)>,
    entries: Vec<SpatialEntry>,
    /// Cached: (arch_idx, field_x, field_y, inner_indices)
    cached_sources: Vec<SpatialSource>,
    cache_valid: bool,
}

struct SpatialSource {
    arch_idx: usize,
    field_x: FieldIndex,
    field_y: FieldIndex,
}

impl SpatialGrid {
    pub fn new(config: SpatialConfig) -> Self {
        let total = config.total_cells();
        Self {
            config,
            cell_ranges: vec![(0, 0); total],
            entries: Vec::new(),
            cached_sources: Vec::new(),
            cache_valid: false,
        }
    }

    /// Cache which archetypes have PosX/PosY
    pub fn cache_sources(&mut self, world: &World) {
        self.cached_sources.clear();
        let pos_x_id = match world.interner().find("PosX") { Some(id) => id, None => return };
        let pos_y_id = match world.interner().find("PosY") { Some(id) => id, None => return };

        for (arch_idx, arch) in world.archetypes.iter().enumerate() {
            if let (Some(fx), Some(fy)) = (arch.schema.find_field(pos_x_id), arch.schema.find_field(pos_y_id)) {
                self.cached_sources.push(SpatialSource { arch_idx, field_x: fx, field_y: fy });
            }
        }
        self.cache_valid = true;
    }

    /// Rebuild grid from world data
    pub fn rebuild(&mut self, world: &World) {
        if !self.cache_valid { self.cache_sources(world); }

        let total_cells = self.config.total_cells();
        self.entries.clear();
        let mut cell_counts = vec![0u32; total_cells];

        // Gather entries
        for src in &self.cached_sources {
            let arch = &world.archetypes[src.arch_idx];
            for (_, eid, offset) in arch.alive_iter() {
                let x = world.storage.read_float(offset, src.field_x);
                let y = world.storage.read_float(offset, src.field_y);
                let (cx, cy) = self.config.pos_to_cell(x, y);
                let cell_idx = self.config.cell_index(cx, cy);
                if cell_idx < total_cells {
                    cell_counts[cell_idx] += 1;
                    self.entries.push(SpatialEntry { entity_index: eid.index, x, y });
                }
            }
        }

        // Counting sort
        let total_entries = self.entries.len();
        let mut starts = vec![0u32; total_cells];
        let mut running = 0u32;
        for i in 0..total_cells {
            starts[i] = running;
            running += cell_counts[i];
        }

        let mut sorted = vec![SpatialEntry { entity_index: 0, x: 0.0, y: 0.0 }; total_entries];
        let mut write_pos = starts.clone();

        for entry in &self.entries {
            let (cx, cy) = self.config.pos_to_cell(entry.x, entry.y);
            let cell_idx = self.config.cell_index(cx, cy);
            if cell_idx < total_cells {
                let pos = write_pos[cell_idx] as usize;
                if pos < sorted.len() {
                    sorted[pos] = *entry;
                    write_pos[cell_idx] += 1;
                }
            }
        }

        self.entries = sorted;
        if self.cell_ranges.len() != total_cells {
            self.cell_ranges.resize(total_cells, (0, 0));
        }
        for i in 0..total_cells {
            self.cell_ranges[i] = (starts[i], starts[i] + cell_counts[i]);
        }
    }

    pub fn query_radius(&self, x: f64, y: f64, radius: f64) -> Vec<SpatialEntry> {
        let r2 = radius * radius;
        let mut result = Vec::new();
        let (min_cx, min_cy) = self.config.pos_to_cell(x - radius, y - radius);
        let (max_cx, max_cy) = self.config.pos_to_cell(x + radius, y + radius);

        for cy in min_cy..=max_cy {
            for cx in min_cx..=max_cx {
                let cell_idx = self.config.cell_index(cx, cy);
                if cell_idx >= self.cell_ranges.len() { continue; }
                let (start, end) = self.cell_ranges[cell_idx];
                for i in (start as usize)..(end as usize).min(self.entries.len()) {
                    let e = &self.entries[i];
                    let dx = e.x - x; let dy = e.y - y;
                    if dx*dx + dy*dy <= r2 { result.push(*e); }
                }
            }
        }
        result
    }

    pub fn query_rect(&self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Vec<SpatialEntry> {
        let mut result = Vec::new();
        let (min_cx, min_cy) = self.config.pos_to_cell(min_x, min_y);
        let (max_cx, max_cy) = self.config.pos_to_cell(max_x, max_y);

        for cy in min_cy..=max_cy {
            for cx in min_cx..=max_cx {
                let cell_idx = self.config.cell_index(cx, cy);
                if cell_idx >= self.cell_ranges.len() { continue; }
                let (start, end) = self.cell_ranges[cell_idx];
                for i in (start as usize)..(end as usize).min(self.entries.len()) {
                    let e = &self.entries[i];
                    if e.x >= min_x && e.x <= max_x && e.y >= min_y && e.y <= max_y {
                        result.push(*e);
                    }
                }
            }
        }
        result
    }

    pub fn query_nearest(&self, x: f64, y: f64, max_radius: f64) -> Option<(SpatialEntry, f64)> {
        let mut best: Option<SpatialEntry> = None;
        let mut best_dist2 = max_radius * max_radius;
        let max_ring = (max_radius / self.config.cell_size).ceil() as i32 + 1;
        let (ccx, ccy) = self.config.pos_to_cell(x, y);

        for ring in 0..=max_ring {
            if ring > 1 {
                let ring_min = ((ring - 1) as f64) * self.config.cell_size;
                if ring_min * ring_min > best_dist2 { break; }
            }
            for dy in -(ring as i32)..=(ring as i32) {
                for dx in -(ring as i32)..=(ring as i32) {
                    if ring > 0 && dx.abs() != ring as i32 && dy.abs() != ring as i32 { continue; }
                    let cell_idx = self.config.cell_index(ccx + dx, ccy + dy);
                    if cell_idx >= self.cell_ranges.len() { continue; }
                    let (start, end) = self.cell_ranges[cell_idx];
                    for i in (start as usize)..(end as usize).min(self.entries.len()) {
                        let e = &self.entries[i];
                        let ddx = e.x - x; let ddy = e.y - y;
                        let d2 = ddx*ddx + ddy*ddy;
                        if d2 < best_dist2 { best_dist2 = d2; best = Some(*e); }
                    }
                }
            }
        }
        best.map(|e| (e, best_dist2.sqrt()))
    }

    pub fn query_k_nearest(&self, x: f64, y: f64, k: usize, max_radius: f64) -> Vec<(SpatialEntry, f64)> {
        let entries = self.query_radius(x, y, max_radius);
        let mut with_dist: Vec<_> = entries.into_iter().map(|e| {
            let dx = e.x - x; let dy = e.y - y;
            (e, (dx*dx + dy*dy).sqrt())
        }).collect();
        with_dist.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        with_dist.truncate(k);
        with_dist
    }

    pub fn entity_count(&self) -> usize { self.entries.len() }
    pub fn avg_entities_per_cell(&self) -> f64 {
        let occ = self.cell_ranges.iter().filter(|(s, e)| e > s).count();
        if occ == 0 { 0.0 } else { self.entries.len() as f64 / occ as f64 }
    }
    pub fn max_entities_in_cell(&self) -> usize {
        self.cell_ranges.iter().map(|(s, e)| (e - s) as usize).max().unwrap_or(0)
    }
    pub fn invalidate_cache(&mut self) { self.cache_valid = false; }
}

pub trait WorldSpatialExt {
    fn create_spatial_grid(&self, config: SpatialConfig) -> SpatialGrid;
    fn rebuild_spatial_grid(&self, grid: &mut SpatialGrid);
}

impl WorldSpatialExt for World {
    fn create_spatial_grid(&self, config: SpatialConfig) -> SpatialGrid {
        let mut grid = SpatialGrid::new(config);
        grid.cache_sources(self);
        grid
    }
    fn rebuild_spatial_grid(&self, grid: &mut SpatialGrid) {
        grid.rebuild(self);
    }
}