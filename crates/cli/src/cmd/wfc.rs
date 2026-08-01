//! Wave Function Collapse (WFC) World Map Generator.
//!
//! Generates highly realistic procedural world maps using constraint propagation
//! over biomes. Includes terminal coloring visualizers, self-contained interactive
//! HTML webapp generation, and robust integration with the TTRPG world state.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use crate::rich_output as r;

// ═══════════════════════════════════════════════════════════════════════════
// WFC Core Engine Models
// ═══════════════════════════════════════════════════════════════════════════

/// Represents the biomes supported by the map generator.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Biome {
    Ocean,
    Coast,
    Beach,
    Plains,
    Forest,
    Mountain,
    Snow,
}

impl Biome {
    /// List of all possible biomes.
    pub fn all() -> &'static [Biome] {
        &[
            Biome::Ocean,
            Biome::Coast,
            Biome::Beach,
            Biome::Plains,
            Biome::Forest,
            Biome::Mountain,
            Biome::Snow,
        ]
    }

    /// Get valid neighbors for a given biome.
    pub fn valid_neighbors(self) -> &'static [Biome] {
        match self {
            Biome::Ocean => &[Biome::Ocean, Biome::Coast],
            Biome::Coast => &[Biome::Ocean, Biome::Coast, Biome::Beach],
            Biome::Beach => &[Biome::Coast, Biome::Beach, Biome::Plains],
            Biome::Plains => &[Biome::Beach, Biome::Plains, Biome::Forest, Biome::Mountain],
            Biome::Forest => &[Biome::Plains, Biome::Forest, Biome::Mountain],
            Biome::Mountain => &[Biome::Plains, Biome::Forest, Biome::Mountain, Biome::Snow],
            Biome::Snow => &[Biome::Mountain, Biome::Snow],
        }
    }

    /// Retrieve the color code and char symbol representation for terminal output.
    pub fn representation(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Biome::Ocean => ("~", "\x1b[38;5;27m", "\x1b[48;5;18m"), // Deep Blue
            Biome::Coast => (".", "\x1b[38;5;39m", "\x1b[48;5;24m"), // Light Blue
            Biome::Beach => ("▒", "\x1b[38;5;220m", "\x1b[48;5;136m"), // Yellow Sand
            Biome::Plains => ("░", "\x1b[38;5;118m", "\x1b[48;5;28m"), // Light Green Grass
            Biome::Forest => ("♣", "\x1b[38;5;22m", "\x1b[48;5;22m"), // Dark Green
            Biome::Mountain => ("▲", "\x1b[38;5;244m", "\x1b[48;5;240m"), // Grey Rocks
            Biome::Snow => ("*", "\x1b[38;5;255m", "\x1b[48;5;252m"), // White Peak
        }
    }

    /// Get default weight for choice sampling (higher means more common).
    pub fn weight(self) -> f32 {
        match self {
            Biome::Ocean => 4.0,
            Biome::Coast => 2.0,
            Biome::Beach => 1.5,
            Biome::Plains => 3.5,
            Biome::Forest => 2.5,
            Biome::Mountain => 1.5,
            Biome::Snow => 1.0,
        }
    }

    pub fn to_string_label(self) -> &'static str {
        match self {
            Biome::Ocean => "Ocean",
            Biome::Coast => "Coast",
            Biome::Beach => "Beach",
            Biome::Plains => "Plains",
            Biome::Forest => "Forest",
            Biome::Mountain => "Mountain",
            Biome::Snow => "Snow",
        }
    }
}

/// A cell in the WFC 2D grid.
#[derive(Clone, Debug)]
pub struct WfcCell {
    pub x: usize,
    pub y: usize,
    /// The remaining possibilities for this cell. If len == 1, it's collapsed.
    pub superposition: Vec<Biome>,
}

impl WfcCell {
    pub fn new(x: usize, y: usize) -> Self {
        Self {
            x,
            y,
            superposition: Biome::all().to_vec(),
        }
    }

    /// Entropy calculation (number of remaining possibilities).
    pub fn entropy(&self) -> usize {
        self.superposition.len()
    }

    pub fn is_collapsed(&self) -> bool {
        self.superposition.len() == 1
    }

    pub fn collapsed_value(&self) -> Option<Biome> {
        if self.is_collapsed() {
            Some(self.superposition[0])
        } else {
            None
        }
    }
}

/// The main Wave Function Collapse 2D solver.
pub struct WfcGrid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Vec<WfcCell>>,
}

impl WfcGrid {
    pub fn new(width: usize, height: usize) -> Self {
        let mut cells = Vec::with_capacity(height);
        for y in 0..height {
            let mut row = Vec::with_capacity(width);
            for x in 0..width {
                row.push(WfcCell::new(x, y));
            }
            cells.push(row);
        }
        Self {
            width,
            height,
            cells,
        }
    }

    /// Perform a full Wave Function Collapse algorithm.
    /// Returns true on success, false if a contradiction occurred.
    pub fn collapse(&mut self, seed: u64) -> bool {
        let mut rng = SimpleRng::new(seed);

        loop {
            // Find the cell with the lowest entropy > 1
            let mut min_entropy = usize::MAX;
            let mut candidates = Vec::new();

            for y in 0..self.height {
                for x in 0..self.width {
                    let ent = self.cells[y][x].entropy();
                    if ent > 1 {
                        if ent < min_entropy {
                            min_entropy = ent;
                            candidates.clear();
                            candidates.push((x, y));
                        } else if ent == min_entropy {
                            candidates.push((x, y));
                        }
                    }
                }
            }

            // If no cell has entropy > 1, the collapse is complete!
            if candidates.is_empty() {
                // Check if any cells reached 0 possibilities (contradiction)
                for y in 0..self.height {
                    for x in 0..self.width {
                        if self.cells[y][x].entropy() == 0 {
                            return false;
                        }
                    }
                }
                return true;
            }

            // Pick a candidate randomly
            let idx = rng.next_range(0, candidates.len());
            let (cx, cy) = candidates[idx];

            // Collapse this cell
            let choices = &self.cells[cy][cx].superposition;
            if choices.is_empty() {
                return false; // Contradiction
            }

            // Pick based on biome weights
            let chosen = self.sample_weighted_biome(choices, &mut rng);
            self.cells[cy][cx].superposition = vec![chosen];

            // Propagate constraints from this collapsed cell
            if !self.propagate(cx, cy) {
                return false; // Contradiction occurred during propagation
            }
        }
    }

    /// Sample a biome from options based on its natural weights.
    fn sample_weighted_biome(&self, options: &[Biome], rng: &mut SimpleRng) -> Biome {
        let mut total_weight = 0.0;
        for &b in options {
            total_weight += b.weight();
        }

        let roll = rng.next_f32() * total_weight;
        let mut running = 0.0;
        for &b in options {
            running += b.weight();
            if roll <= running {
                return b;
            }
        }
        options[0]
    }

    /// Propagate constraint changes through neighboring cells.
    pub fn propagate(&mut self, start_x: usize, start_y: usize) -> bool {
        let mut queue = VecDeque::new();
        queue.push_back((start_x, start_y));

        let mut in_queue = HashSet::new();
        in_queue.insert((start_x, start_y));

        while let Some((x, y)) = queue.pop_front() {
            in_queue.remove(&(x, y));

            let current_allowed = &self.cells[y][x].superposition.clone();
            if current_allowed.is_empty() {
                return false; // Contradiction
            }

            // Calculate overall set of allowed neighbors for the current cell's superposed biomes
            let mut allowed_neighbors_set = HashSet::new();
            for &b in current_allowed {
                for &nb in b.valid_neighbors() {
                    allowed_neighbors_set.insert(nb);
                }
            }

            // Check all 4-way neighbors
            let neighbors = self.get_neighbors(x, y);
            for (nx, ny) in neighbors {
                let neighbor_cell = &mut self.cells[ny][nx];
                let old_len = neighbor_cell.superposition.len();

                // Retain only choices that are valid neighbors of at least one biome in current_allowed
                neighbor_cell
                    .superposition
                    .retain(|b| allowed_neighbors_set.contains(b));

                if neighbor_cell.superposition.is_empty() {
                    return false; // Contradiction
                }

                if neighbor_cell.superposition.len() < old_len {
                    if !in_queue.contains(&(nx, ny)) {
                        queue.push_back((nx, ny));
                        in_queue.insert((nx, ny));
                    }
                }
            }
        }
        true
    }

    /// Retrieve valid 4-way neighbor coordinates.
    pub fn get_neighbors(&self, x: usize, y: usize) -> Vec<(usize, usize)> {
        let mut res = Vec::new();
        if x > 0 {
            res.push((x - 1, y));
        }
        if x + 1 < self.width {
            res.push((x + 1, y));
        }
        if y > 0 {
            res.push((x, y - 1));
        }
        if y + 1 < self.height {
            res.push((x, y + 1));
        }
        res
    }

    /// Terminal colored display helper.
    pub fn print_terminal(&self) {
        for y in 0..self.height {
            let mut row_str = String::new();
            for x in 0..self.width {
                if let Some(biome) = self.cells[y][x].collapsed_value() {
                    let (symbol, fg, _bg) = biome.representation();
                    row_str.push_str(&format!("{fg}{symbol}\x1b[0m"));
                } else {
                    row_str.push_str("?");
                }
            }
            println!("{}", row_str);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Custom Simple LCG Pseudo-Random Number Generator (no dependencies)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    pub fn new(seed: u64) -> Self {
        let initial = if seed == 0 { 123456789 } else { seed };
        Self { state: initial }
    }

    /// Standard LCG iteration.
    pub fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    pub fn next_f32(&mut self) -> f32 {
        let val = self.next();
        (val & 0xFFFFFF) as f32 / 16777216.0
    }

    pub fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        let diff = max - min;
        let roll = self.next() as usize;
        min + (roll % diff)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Self-Contained Interactive HTML Webapp Template
// ═══════════════════════════════════════════════════════════════════════════

const WEBAPP_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RoCo AI — Interactive Wave Function Collapse map Creator</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            background-color: #0d1117;
            color: #c9d1d9;
            display: flex;
            height: 100vh;
            overflow: hidden;
        }
        #sidebar {
            width: 320px;
            background-color: #161b22;
            border-right: 1px solid #30363d;
            padding: 20px;
            display: flex;
            flex-direction: column;
            gap: 16px;
            overflow-y: auto;
        }
        #main-view {
            flex-grow: 1;
            display: flex;
            flex-direction: column;
            position: relative;
        }
        #toolbar {
            height: 60px;
            background-color: #161b22;
            border-bottom: 1px solid #30363d;
            display: flex;
            align-items: center;
            padding: 0 20px;
            justify-content: space-between;
        }
        #canvas-container {
            flex-grow: 1;
            position: relative;
            overflow: auto;
            background-color: #07090e;
            display: flex;
            align-items: center;
            justify-content: center;
        }
        canvas {
            display: block;
            box-shadow: 0 4px 20px rgba(0,0,0,0.5);
            cursor: grab;
        }
        canvas:active {
            cursor: grabbing;
        }
        h1 {
            font-size: 1.2rem;
            color: #58a6ff;
            margin-bottom: 8px;
            display: flex;
            align-items: center;
            gap: 8px;
        }
        .section-title {
            font-size: 0.9rem;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            color: #8b949e;
            margin-top: 10px;
            border-bottom: 1px solid #21262d;
            padding-bottom: 4px;
        }
        button {
            background-color: #21262d;
            border: 1px solid #30363d;
            color: #c9d1d9;
            padding: 8px 12px;
            border-radius: 6px;
            cursor: pointer;
            font-weight: 600;
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 6px;
            transition: all 0.2s;
        }
        button:hover {
            background-color: #30363d;
            border-color: #8b949e;
        }
        button.primary {
            background-color: #238636;
            border-color: #2ea043;
            color: #ffffff;
        }
        button.primary:hover {
            background-color: #2ea043;
        }
        .control-group {
            display: flex;
            flex-direction: column;
            gap: 6px;
        }
        label {
            font-size: 0.85rem;
            color: #8b949e;
        }
        select, input {
            background-color: #0d1117;
            border: 1px solid #30363d;
            color: #c9d1d9;
            padding: 8px;
            border-radius: 6px;
            outline: none;
            width: 100%;
        }
        .legend-item {
            display: flex;
            align-items: center;
            gap: 8px;
            font-size: 0.85rem;
            padding: 4px 0;
        }
        .color-box {
            width: 16px;
            height: 16px;
            border-radius: 4px;
            border: 1px solid rgba(255,255,255,0.1);
        }
        #inspector {
            background-color: #161b22;
            border: 1px solid #30363d;
            border-radius: 8px;
            padding: 12px;
            margin-top: auto;
            font-size: 0.85rem;
        }
        #inspector h3 {
            color: #58a6ff;
            margin-bottom: 6px;
            font-size: 0.9rem;
        }
        #stats {
            font-size: 0.8rem;
            color: #8b949e;
        }
    </style>
</head>
<body>
    <div id="sidebar">
        <div>
            <h1>🌍 Wave Function Collapse</h1>
            <p style="font-size: 0.8rem; color: #8b949e; margin-bottom: 12px;">Procedural Infinite World Map Generator</p>
        </div>

        <div class="control-group">
            <button class="primary" id="btn-collapse">⚡ Generate Fresh Collapse</button>
        </div>

        <div class="control-group">
            <label for="select-mode">Generation Mode</label>
            <select id="select-mode">
                <option value="finite">Finite Grid (40x20)</option>
                <option value="infinite">Infinite Scroll & Expand</option>
            </select>
        </div>

        <div class="control-group">
            <label for="select-speed">Collapse Visualizer Speed</label>
            <select id="select-speed">
                <option value="instant">Instant collapse</option>
                <option value="fast">Fast Animation (10ms)</option>
                <option value="animate">Slow Superposition (50ms)</option>
            </select>
        </div>

        <div class="section-title">Biome Palette Rules</div>
        <div id="legend">
            <div class="legend-item"><div class="color-box" style="background-color: #0f1c3f;"></div> Ocean (Deep water)</div>
            <div class="legend-item"><div class="color-box" style="background-color: #1e3f66;"></div> Coast (Shallow water)</div>
            <div class="legend-item"><div class="color-box" style="background-color: #d4b26f;"></div> Beach (Sand dunes)</div>
            <div class="legend-item"><div class="color-box" style="background-color: #3f7a3f;"></div> Plains (Grass fields)</div>
            <div class="legend-item"><div class="color-box" style="background-color: #1b4d22;"></div> Forest (Thick canopy)</div>
            <div class="legend-item"><div class="color-box" style="background-color: #6e7a8a;"></div> Mountain (Grey rocks)</div>
            <div class="legend-item"><div class="color-box" style="background-color: #ffffff;"></div> Snow Peak (Glaciers)</div>
        </div>

        <div id="inspector">
            <h3>🔍 Biome Inspector</h3>
            <div id="inspect-details">Click any coordinate on the map to query procedural lore details.</div>
        </div>

        <div id="stats">
            <div>Superpositions resolved: <span id="stat-collapsed">0</span></div>
            <div>Contradiction retries: <span id="stat-retries">0</span></div>
        </div>
    </div>

    <div id="main-view">
        <div id="toolbar">
            <div style="font-weight: 600; color: #58a6ff;">🎮 Interactive Controls</div>
            <div style="font-size: 0.85rem; color: #8b949e; display: flex; gap: 16px;">
                <span>Drag map to pan</span>
                <span>Scroll to zoom</span>
                <span>Hover to highlight</span>
            </div>
        </div>
        <div id="canvas-container">
            <canvas id="map-canvas"></canvas>
        </div>
    </div>

    <script>
        const canvas = document.getElementById('map-canvas');
        const ctx = canvas.getContext('2d');
        const btnCollapse = document.getElementById('btn-collapse');
        const selectMode = document.getElementById('select-mode');
        const selectSpeed = document.getElementById('select-speed');
        const inspectDetails = document.getElementById('inspect-details');
        const statCollapsed = document.getElementById('stat-collapsed');
        const statRetries = document.getElementById('stat-retries');

        // Biome profiles
        const BIOMES = {
            'Ocean':    { color: '#0f1c3f', weight: 4.0, neighbors: ['Ocean', 'Coast'], symbol: '~', name_pool: ['Abyssal Trench', 'Siren Gulf', 'Silent Abyss', 'Azure Depths'] },
            'Coast':    { color: '#1e3f66', weight: 2.0, neighbors: ['Ocean', 'Coast', 'Beach'], symbol: '.', name_pool: ['Shallow Bay', 'Mist Coast', 'Sailor Shallows', 'Whispering Coast'] },
            'Beach':    { color: '#d4b26f', weight: 1.5, neighbors: ['Coast', 'Beach', 'Plains'], symbol: '▒', name_pool: ['Siren Dunes', 'Amber Strand', 'Golden Dunes', 'Tortuga Reach'] },
            'Plains':   { color: '#3f7a3f', weight: 3.5, neighbors: ['Beach', 'Plains', 'Forest', 'Mountain'], symbol: '░', name_pool: ['Emerald Grasslands', 'Sunken Plains', 'Whispering Glade', 'Grum Mead'] },
            'Forest':   { color: '#1b4d22', weight: 2.5, neighbors: ['Plains', 'Forest', 'Mountain'], symbol: '♣', name_pool: ['Shadow Canopy', 'Weeping Woods', 'Druid Hollow', 'Feywild Grove'] },
            'Mountain': { color: '#6e7a8a', weight: 1.5, neighbors: ['Plains', 'Forest', 'Mountain', 'Snow'], symbol: '▲', name_pool: ['Crag Ridge', 'Stonegard Spire', 'Thunderhorn Peak', 'Mist Peak'] },
            'Snow':     { color: '#ffffff', weight: 1.0, neighbors: ['Mountain', 'Snow'], symbol: '*', name_pool: ['Frostfire Peak', 'Winter Spire', 'Pale Summit', 'Frozen Grotto'] }
        };

        const BIOME_KEYS = Object.keys(BIOMES);

        // Grid state
        let mode = 'finite';
        let width = 40;
        let height = 20;
        let tileSize = 24;
        let grid = {}; // sparse DB for coordinates: "x,y" => { x, y, superposition: [] }
        let isCollapsing = false;
        let retriesCount = 0;

        // Visual panning / zooming state
        let panX = 0;
        let panY = 0;
        let zoom = 1.0;
        let isDragging = false;
        let startX, startY;

        // Seed generator for names
        function getSeedString(x, y, biome) {
            const index = Math.abs((x * 374761393) ^ (y * 668265263)) % 4;
            return BIOMES[biome].name_pool[index];
        }

        function getBiomeLore(x, y, biome) {
            const bName = getSeedString(x, y, biome);
            let desc = "";
            switch(biome) {
                case 'Ocean': desc = `Deep uncharted waters where massive sea beasts are rumored to dwell in complete darkness.`; break;
                case 'Coast': desc = `Gentle, mist-shrouded coastlines safe for merchant ships, but hazardous if storms roll in.`; break;
                case 'Beach': desc = `Warm sandy beaches scattered with old pirate wreckage and gleaming shells.`; break;
                case 'Plains': desc = `Lush expansive prairies stretching beyond the horizon, home to wandering herds.`; break;
                case 'Forest': desc = `An ancient, dense forest where sunlight barely touches the forest floor. Magic leaks through the roots.`; break;
                case 'Mountain': desc = `Imposing jagged cliffs carved by glaciers over eons. High risk of rockslides.`; break;
                case 'Snow': desc = `A freezing polar environment perpetually covered in pristine snow and sheets of ice.`; break;
            }
            return `<strong>${bName}</strong><br/><br/>Biome: ${biome}<br/>Coords: (${x}, ${y})<br/><br/><em>${desc}</em>`;
        }

        // Initialize grid
        function initGrid() {
            grid = {};
            if (mode === 'finite') {
                for (let y = 0; y < height; y++) {
                    for (let x = 0; x < width; x++) {
                        grid[`${x},${y}`] = { x, y, superposition: [...BIOME_KEYS] };
                    }
                }
            } else {
                // In infinite mode, we lazily populate on screen view
                panX = 0;
                panY = 0;
                zoom = 1.0;
                generateVisibleInfiniteChunks();
            }
            retriesCount = 0;
            statRetries.innerText = retriesCount;
            draw();
        }

        // Collapse cell superposition based on weighted random
        function sampleWeighted(options) {
            let total = 0;
            options.forEach(b => total += BIOMES[b].weight);
            const roll = Math.random() * total;
            let running = 0;
            for (let b of options) {
                running += BIOMES[b].weight;
                if (roll <= running) return b;
            }
            return options[0];
        }

        // Propagate constraints from coordinate (x, y)
        function propagate(startX, startY) {
            const queue = [[startX, startY]];
            const inQueue = new Set([`${startX},${startY}`]);

            while (queue.length > 0) {
                const [cx, cy] = queue.shift();
                inQueue.delete(`${cx},${cy}`);

                const currentCell = grid[`${cx},${cy}`];
                if (!currentCell || currentCell.superposition.length === 0) continue;

                // Allowed neighbors
                const allowedSet = new Set();
                currentCell.superposition.forEach(b => {
                    BIOMES[b].neighbors.forEach(nb => allowedSet.add(nb));
                });

                // Neighbors
                const neighbors = [
                    [cx - 1, cy], [cx + 1, cy],
                    [cx, cy - 1], [cx, cy + 1]
                ];

                for (let [nx, ny] of neighbors) {
                    const key = `${nx},${ny}`;
                    let nCell = grid[key];

                    // Only propagate to existing coordinates in finite mode
                    if (mode === 'finite') {
                        if (nx < 0 || nx >= width || ny < 0 || ny >= height) continue;
                    }

                    if (!nCell) {
                        // Infinite mode lazy allocation
                        nCell = { x: nx, y: ny, superposition: [...BIOME_KEYS] };
                        grid[key] = nCell;
                    }

                    const oldLen = nCell.superposition.length;
                    nCell.superposition = nCell.superposition.filter(b => allowedSet.has(b));

                    if (nCell.superposition.length === 0) {
                        return false; // Contradiction!
                    }

                    if (nCell.superposition.length < oldLen) {
                        if (!inQueue.has(key)) {
                            queue.push([nx, ny]);
                            inQueue.add(key);
                        }
                    }
                }
            }
            return true;
        }

        // Run full collapse
        async function runWFC() {
            if (isCollapsing) return;
            isCollapsing = true;
            btnCollapse.disabled = true;

            const speed = selectSpeed.value;

            while (true) {
                // Find uncollapsed cell with lowest entropy > 1
                let minEntropy = Infinity;
                let candidates = [];

                // Filter grid cells
                for (let key in grid) {
                    const cell = grid[key];
                    const ent = cell.superposition.length;
                    if (ent > 1) {
                        if (ent < minEntropy) {
                            minEntropy = ent;
                            candidates = [cell];
                        } else if (ent === minEntropy) {
                            candidates.push(cell);
                        }
                    }
                }

                if (candidates.length === 0) {
                    // Fully collapsed! Let's check for empty/contradiction
                    let hasError = false;
                    for (let key in grid) {
                        if (grid[key].superposition.length === 0) {
                            hasError = true;
                            break;
                        }
                    }

                    if (hasError) {
                        retriesCount++;
                        statRetries.innerText = retriesCount;
                        initGrid();
                        isCollapsing = false;
                        runWFC(); // Retry
                        return;
                    }
                    break;
                }

                // Choose a random cell from candidates
                const cell = candidates[Math.floor(Math.random() * candidates.length)];
                const chosen = sampleWeighted(cell.superposition);
                cell.superposition = [chosen];

                const success = propagate(cell.x, cell.y);
                if (!success) {
                    retriesCount++;
                    statRetries.innerText = retriesCount;
                    initGrid();
                    isCollapsing = false;
                    runWFC(); // Retry
                    return;
                }

                statCollapsed.innerText = Object.values(grid).filter(c => c.superposition.length === 1).length;

                if (speed !== 'instant') {
                    draw();
                    await new Promise(r => setTimeout(r, speed === 'fast' ? 10 : 50));
                }
            }

            draw();
            isCollapsing = false;
            btnCollapse.disabled = false;
        }

        // Infinite lazy generation
        function generateVisibleInfiniteChunks() {
            const startCol = Math.floor((-panX) / (tileSize * zoom)) - 2;
            const endCol = Math.floor((-panX + canvas.width) / (tileSize * zoom)) + 2;
            const startRow = Math.floor((-panY) / (tileSize * zoom)) - 2;
            const endRow = Math.floor((-panY + canvas.height) / (tileSize * zoom)) + 2;

            let updated = false;
            for (let y = startRow; y <= endRow; y++) {
                for (let x = startCol; x <= endCol; x++) {
                    const key = `${x},${y}`;
                    if (!grid[key]) {
                        grid[key] = { x, y, superposition: [...BIOME_KEYS] };
                        // Find any existing neighbor and propagate to set initial bounds
                        const neighbors = [[x-1, y], [x+1, y], [x, y-1], [x, y+1]];
                        neighbors.forEach(([nx, ny]) => {
                            const nCell = grid[`${nx},${ny}`];
                            if (nCell && nCell.superposition.length === 1) {
                                // Already collapsed neighbor, enforce rules immediately
                                const allowed = new Set();
                                BIOMES[nCell.superposition[0]].neighbors.forEach(nb => allowed.add(nb));
                                grid[key].superposition = grid[key].superposition.filter(b => allowed.has(b));
                            }
                        });
                        updated = true;
                    }
                }
            }
            if (updated) {
                // Propagate to resolve
                for (let y = startRow; y <= endRow; y++) {
                    for (let x = startCol; x <= endCol; x++) {
                        const cell = grid[`${x},${y}`];
                        if (cell && cell.superposition.length > 1) {
                            const chosen = sampleWeighted(cell.superposition);
                            cell.superposition = [chosen];
                            propagate(x, y);
                        }
                    }
                }
                statCollapsed.innerText = Object.values(grid).filter(c => c.superposition.length === 1).length;
            }
        }

        // Draw the map grid
        function draw() {
            ctx.fillStyle = '#07090e';
            ctx.fillRect(0, 0, canvas.width, canvas.height);

            ctx.save();
            ctx.translate(panX, panY);
            ctx.scale(zoom, zoom);

            for (let key in grid) {
                const cell = grid[key];
                const x = cell.x * tileSize;
                const y = cell.y * tileSize;

                if (cell.superposition.length === 1) {
                    ctx.fillStyle = BIOMES[cell.superposition[0]].color;
                    ctx.fillRect(x, y, tileSize, tileSize);
                } else {
                    // Superposition state representation (grey shades / patterns)
                    const ratio = cell.superposition.length / BIOME_KEYS.length;
                    ctx.fillStyle = `rgba(110, 122, 138, ${0.1 + ratio * 0.4})`;
                    ctx.fillRect(x, y, tileSize, tileSize);
                    ctx.strokeStyle = '#21262d';
                    ctx.strokeRect(x, y, tileSize, tileSize);
                }
            }

            ctx.restore();
        }

        // Mouse click inspector
        function getCellAtMouse(clientX, clientY) {
            const rect = canvas.getBoundingClientRect();
            const mouseX = clientX - rect.left;
            const mouseY = clientY - rect.top;

            const gridX = Math.floor((mouseX - panX) / (tileSize * zoom));
            const gridY = Math.floor((mouseY - panY) / (tileSize * zoom));

            return grid[`${gridX},${gridY}`];
        }

        canvas.addEventListener('mousedown', (e) => {
            const cell = getCellAtMouse(e.clientX, e.clientY);
            if (cell && cell.superposition.length === 1) {
                inspectDetails.innerHTML = getBiomeLore(cell.x, cell.y, cell.superposition[0]);
            } else if (cell) {
                inspectDetails.innerHTML = `<strong>Superposition Cell</strong><br/><br/>Possibilities: ${cell.superposition.join(', ')}`;
            }

            isDragging = true;
            startX = e.clientX - panX;
            startY = e.clientY - panY;
        });

        canvas.addEventListener('mousemove', (e) => {
            if (isDragging) {
                panX = e.clientX - startX;
                panY = e.clientY - startY;
                if (mode === 'infinite') {
                    generateVisibleInfiniteChunks();
                }
                draw();
            }
        });

        window.addEventListener('mouseup', () => {
            isDragging = false;
        });

        // Zoom wheel
        canvas.addEventListener('wheel', (e) => {
            e.preventDefault();
            const zoomFactor = 1.1;
            if (e.deltaY < 0) {
                zoom *= zoomFactor;
            } else {
                zoom /= zoomFactor;
            }
            zoom = Math.max(0.5, Math.min(zoom, 4.0));
            if (mode === 'infinite') {
                generateVisibleInfiniteChunks();
            }
            draw();
        });

        // Handle Mode change
        selectMode.addEventListener('change', (e) => {
            mode = e.target.value;
            initGrid();
        });

        btnCollapse.addEventListener('click', () => {
            initGrid();
            runWFC();
        });

        // Resize handler
        function resize() {
            canvas.width = canvas.parentElement.clientWidth;
            canvas.height = canvas.parentElement.clientHeight;
            draw();
        }
        window.addEventListener('resize', resize);

        // Bootstrap
        resize();
        initGrid();
        runWFC();
    </script>
</body>
</html>
"#;

// ═══════════════════════════════════════════════════════════════════════════
// World Map CLI Subcommand Implementation
// ═══════════════════════════════════════════════════════════════════════════

/// Execute the `roco map` command.
pub fn cmd_map(extra: &[&str]) {
    // Parse size options or use beautiful defaults
    let width: usize = crate::parse_opt("--width", extra)
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let height: usize = crate::parse_opt("--height", extra)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let seed: u64 = crate::parse_opt("--seed", extra)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });

    r::header("🌍 Procedural WFC World Map Creator 🌍");
    println!("  Width:  {width}");
    println!("  Height: {height}");
    println!("  Seed:   {seed}\n");

    let mut grid = WfcGrid::new(width, height);

    println!(
        "{}Synthesizing wave function constraints...{}",
        r::Colors::DIM,
        r::Colors::RESET
    );

    let mut success = false;
    let mut attempt = 1;
    while attempt <= 10 {
        if grid.collapse(seed + attempt) {
            success = true;
            break;
        }
        // Regenerate grid and retry on contradiction
        grid = WfcGrid::new(width, height);
        attempt += 1;
    }

    if !success {
        r::error("WFC collapsed with too many contradictions. Try another seed.");
        return;
    }

    r::success("World map generated successfully!");
    println!();
    grid.print_terminal();
    println!();

    // ── Create & Write the self-contained HTML Webapp file ─────────────────
    let base_dir = if let Ok(dir) = std::env::var("ROCO_DIR") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(".roco")
    };
    let _ = std::fs::create_dir_all(&base_dir);
    let map_html_path = base_dir.join("wfc_map.html");

    // Replace default width/height configuration in JavaScript to match CLI arguments
    let customized_html = WEBAPP_HTML
        .replace("let width = 40;", &format!("let width = {width};"))
        .replace("let height = 20;", &format!("let height = {height};"));

    if std::fs::write(&map_html_path, customized_html).is_ok() {
        r::info(&format!(
            "HTML Interactive Webapp saved to: {}",
            map_html_path.display()
        ));
        // Automatically open map in browser
        let absolute_path = std::fs::canonicalize(&map_html_path).unwrap_or(map_html_path);
        let url = format!("file://{}", absolute_path.display());
        open_browser(&url);
    }

    // ── Export to TTRPG if requested ───────────────────────────────────────
    if extra.iter().any(|&a| a == "--ttrpg") {
        match export_map_to_ttrpg_state(&grid) {
            Ok(_) => r::success(
                "Successfully exported travelable biome regions to .roco/ttrpg_state.json!",
            ),
            Err(e) => r::error(&format!("Failed to export map to TTRPG system: {e}")),
        }
    }
}

/// Open browser helper.
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
}

// ═══════════════════════════════════════════════════════════════════════════
// TTRPG Export System
// ═══════════════════════════════════════════════════════════════════════════

/// Export the WFC map into RoCo's TTRPG state system.
fn export_map_to_ttrpg_state(grid: &WfcGrid) -> Result<(), String> {
    // 1. Cluster adjacent tiles of the same biome using Breadth-First Search
    let mut visited = HashSet::new();
    let mut clusters: Vec<(Biome, Vec<(usize, usize)>)> = Vec::new();

    for y in 0..grid.height {
        for x in 0..grid.width {
            if visited.contains(&(x, y)) {
                continue;
            }
            let biome = match grid.cells[y][x].collapsed_value() {
                Some(b) => b,
                None => continue,
            };

            // Start a new cluster
            let mut cluster_coords = Vec::new();
            let mut queue = VecDeque::new();
            queue.push_back((x, y));
            visited.insert((x, y));

            while let Some((cx, cy)) = queue.pop_front() {
                cluster_coords.push((cx, cy));

                // Check 4-way neighbors of the same biome
                let neighbors = grid.get_neighbors(cx, cy);
                for (nx, ny) in neighbors {
                    if !visited.contains(&(nx, ny)) {
                        if let Some(nb_biome) = grid.cells[ny][nx].collapsed_value() {
                            if nb_biome == biome {
                                visited.insert((nx, ny));
                                queue.push_back((nx, ny));
                            }
                        }
                    }
                }
            }

            clusters.push((biome, cluster_coords));
        }
    }

    // Sort clusters by size (largest first)
    clusters.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    // Map cluster coordinate sets to unique region IDs
    let mut coord_to_region_id: HashMap<(usize, usize), String> = HashMap::new();
    let mut regions = Vec::new();

    for (cluster_index, (biome, coords)) in clusters.iter().enumerate() {
        // Limit total exported regions to keep gameplay tightly structured and readable
        if cluster_index >= 20 {
            break;
        }

        let region_id = format!("region_{cluster_index}");
        for &coord in coords {
            coord_to_region_id.insert(coord, region_id.clone());
        }

        let region_name = get_biome_cluster_name(biome, cluster_index);
        let description = format!(
            "A magnificent, procedurally synthesized area spanning coordinates from ({}, {}) to ({}, {}). This region features characteristics of the {} biome.",
            coords.first().unwrap().0, coords.first().unwrap().1,
            coords.last().unwrap().0, coords.last().unwrap().1,
            biome.to_string_label()
        );

        regions.push((region_id, region_name, description, biome, coords.clone()));
    }

    // 2. Load the current TTRPG state or create default
    let mut ttrpg_state = crate::cmd::ttrpg::TtrpgState::load_or_default();

    // Clear existing locations and rebuild with our new procedurally mapped continent
    ttrpg_state.world.locations.clear();

    // Rebuild world locations and establish bidirectional exit transitions
    for (id, name, desc, _biome, coords) in &regions {
        // Collect exit connections (adjacent coordinates pointing to other region IDs)
        let mut exits = HashMap::new();

        for &coord in coords {
            let neighbors = grid.get_neighbors(coord.0, coord.1);
            for (nx, ny) in neighbors {
                if let Some(other_id) = coord_to_region_id.get(&(nx, ny)) {
                    if other_id != id {
                        // Locate the other region's name
                        if let Some(other_region) = regions.iter().find(|r| r.0 == *other_id) {
                            let other_name = &other_region.1;
                            // Determine directional exit vector
                            let dir = if nx > coord.0 {
                                "east"
                            } else if nx < coord.0 {
                                "west"
                            } else if ny > coord.1 {
                                "south"
                            } else {
                                "north"
                            };
                            exits.insert(dir.to_string(), other_name.clone());
                        }
                    }
                }
            }
        }

        ttrpg_state
            .world
            .locations
            .push(crate::cmd::ttrpg::TtrpgLocation {
                name: name.clone(),
                description: desc.clone(),
                exits,
            });
    }

    // Update active player's location to the first mapped region so they don't spawn in limbo
    if let Some(first_region) = regions.first() {
        ttrpg_state.current_location = first_region.1.clone();
    }

    // Save TTRPG state back to disk
    ttrpg_state.save()
}

/// Retrieve thematic names for biome clusters.
fn get_biome_cluster_name(biome: &Biome, index: usize) -> String {
    let prefixes = [
        "Glimmering",
        "Whispering",
        "Sunken",
        "Frostfire",
        "Ironclad",
        "Emerald",
        "Shining",
        "Shattered",
        "Verdant",
        "Ashen",
    ];
    let pf = prefixes[index % prefixes.len()];

    match biome {
        Biome::Ocean => format!("The {pf} Ocean"),
        Biome::Coast => format!("The {pf} Bay"),
        Biome::Beach => format!("The {pf} Strand"),
        Biome::Plains => format!("The {pf} Plains"),
        Biome::Forest => format!("The {pf} Woods"),
        Biome::Mountain => format!("The {pf} Range"),
        Biome::Snow => format!("The {pf} Peak"),
    }
}
