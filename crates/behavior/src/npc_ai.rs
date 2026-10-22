//! M26: [`npc_ai`] — city population and observable urban routine
//! (SPEC-M26-NPC-AI §1-§2, REQ-L99-059).
//!
//! **Scope.** The caps and probabilities of §1, the units and the state
//! machine `SeekingTile → MovingForward` of §2, and the deterministic routine
//! that consumes the navgrid. Pure: 0 Bevy, 0 I/O, 0 global RNG — the rolls of
//! a population enter as parameters ([`roll_appearance`]) and the routine
//! carries its own seeded generator ([`Walker::new`]).
//!
//! **Declared N-4 (SPEC §5).** The DFU source does **not** assign a
//! destination per building nor model entering/leaving interiors: the mobile
//! walks the navgrid by tile weight (`MobilePersonMotor.cs:303-375`) and at
//! night the population is recycled (`PopulationManager.cs:218`). The
//! building→street→building routine is **this target's contract**, not DFU
//! fidelity; here it is implemented as a weighted walk (streets with high
//! weight) and is not attributed to the source.
//!
//! **Source of the constants** (`0038a1e6`, MIT, executable spec):
//! `PopulationManager.cs:31-38,97-109,216-218` ·
//! `CityNavigation.cs:32-36,86-96` · `MobilePersonMotor.cs:32-36,185-193,
//! 303-375` · `MobilePersonNPC.cs:29,144-156`.
//!
//! **Real population cap (SPEC §1, not `clamp(N·24,16,96)`):** the division
//! `(width·height)/16` is **integer**; blocks 1..15 ⇒ quotient 0 ⇒
//! `Clamp(0,1,4)=1` ⇒ 24. The clamp acts on the quotient.

use crate::determinism::{fnv1a64, SplitMix64};

/// Classic units per navigation tile (`CityNavigation.cs:32`).
pub const CLASSIC_UNITS_PER_TILE: u32 = 64;
/// `MeshReader.GlobalScale` (`CityNavigation.cs:33`).
pub const GLOBAL_SCALE: f32 = 0.025;
/// Side of one navgrid cell in meters: `64 · 0.025` (`CityNavigation.cs:32-33`).
pub const CELL_METERS: f32 = 1.6;
/// Cells per block side (`CityNavigation.cs:35`).
pub const CELLS_PER_BLOCK: usize = 64;
/// Cells per block (`blockSize`, `CityNavigation.cs:36`).
pub const CELLS_PER_BLOCK_AREA: usize = 4096;
/// NPCs per 16 blocks (`PopulationManager.cs:35`).
pub const POPULATION_PER_16_BLOCKS: usize = 24;
/// `Clamp` of the block quotient (`PopulationManager.cs:97-99`).
pub const POPULATION_BLOCK_CLAMP: (usize, usize) = (1, 4);
/// Population ticks per second (`PopulationManager.cs:31`).
pub const TICKS_PER_SECOND: u32 = 10;
/// Tick interval in ms (`1/10 s`, `PopulationManager.cs:106-109`).
pub const TICK_MS: u32 = 100;
/// Margin outside the locality rect in world units
/// (`PopulationManager.cs:34`).
pub const MAX_PLAYER_DISTANCE_OUTSIDE_RECT: u32 = 2500;
/// Navgrid sampling radius in cells (`PopulationManager.cs:36,151`).
pub const NAV_GRID_SPAWN_RADIUS: i32 = 96;
/// Recycle distance in scene meters (`PopulationManager.cs:37`).
pub const RECYCLE_DISTANCE_METERS: f32 = 150.0;
/// Visible population range in meters (`PopulationManager.cs:38`).
pub const ALLOW_VISIBLE_POP_RANGE_METERS: f32 = 120.0;
/// `SeekCount > 4` ⇒ recycle (`PopulationManager.cs:216`).
pub const SEEK_COUNT_RECYCLE_ABOVE: u32 = 4;
/// NPC speed in scene (`MobilePersonMotor.cs:33,370`).
pub const NPC_SPEED_METERS_PER_SECOND: f32 = 1.3;
/// `randomChangeChance` (`MobilePersonMotor.cs:35`).
pub const RANDOM_CHANGE_CHANCE: f32 = 0.025;
/// `tileDowngradeChance` (`MobilePersonMotor.cs:36`).
pub const TILE_DOWNGRADE_CHANCE: f32 = 0.20;
/// `idleDistance` in meters (`MobilePersonMotor.cs:32`).
pub const IDLE_DISTANCE_METERS: f32 = 2.5;
/// Arrival threshold at the destination cell (`MobilePersonMotor.cs:373`).
pub const ARRIVAL_EPSILON_METERS: f32 = 0.1;
/// `Random.Range(0, 32) == 0` ⇒ guard (`MobilePersonNPC.cs:144-148`).
pub const GUARD_ONE_IN: u32 = 32;
/// Outfit variants (`MobilePersonNPC.cs:29,156`).
pub const OUTFIT_VARIANTS: u8 = 4;
/// Genders (`MobilePersonNPC.cs:154`).
pub const GENDERS: u8 = 2;

/// `maxPopulation` of a locality with `width × height` RMB blocks
/// (`PopulationManager.cs:97-99`): `Clamp((w·h)/16, 1, 4) · 24`, with
/// **integer** division on the quotient (SPEC §1).
pub fn max_population(block_width: usize, block_height: usize) -> usize {
    let blocks = block_width.saturating_mul(block_height) / 16;
    blocks.clamp(POPULATION_BLOCK_CLAMP.0, POPULATION_BLOCK_CLAMP.1) * POPULATION_PER_16_BLOCKS
}

/// Interval of one population tick in milliseconds (`1000 / 10`).
pub const fn tick_millis() -> u32 {
    TICK_MS
}

/// Navgrid side in cells of a city of `blocks × blocks` blocks
/// (`NavGridWidth = cityWidth · 64`, `CityNavigation.cs:86-96`).
pub fn nav_grid_side(blocks: usize) -> usize {
    blocks * CELLS_PER_BLOCK
}

/// Meters per navgrid cell (`64 · 0.025`).
pub const fn cell_meters() -> f32 {
    CELL_METERS
}

/// Cells covered by a distance in meters.
pub fn meters_to_cells(meters: f32) -> f32 {
    meters / CELL_METERS
}

/// Seconds an NPC needs to cross `cells` cells at 1.3 m/s.
pub fn seconds_for_cells(cells: f32) -> f32 {
    cells * CELL_METERS / NPC_SPEED_METERS_PER_SECOND
}

/// Margin around the locality expressed in cells
/// (`2500 u · 0.025 = 62.5 m` ⇒ `39.0625` cells).
pub fn margin_cells() -> f32 {
    meters_to_cells(MAX_PLAYER_DISTANCE_OUTSIDE_RECT as f32 * GLOBAL_SCALE)
}

/// NPC gender (`MobilePersonNPC.cs:154`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    /// Male.
    Male,
    /// Female.
    Female,
}

impl Gender {
    /// `1/2` draw over an already taken roll (`roll % 2`).
    pub const fn draw(roll: u32) -> Self {
        if roll.is_multiple_of(2) {
            Self::Male
        } else {
            Self::Female
        }
    }
}

/// Drawn appearance of a city NPC (§2.2 C1-C6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appearance {
    /// `Random.Range(0, 32) == 0`.
    pub guard: bool,
    /// Gender.
    pub gender: Gender,
    /// Outfit `0..3`.
    pub outfit: u8,
}

/// Applies the NPC draw branches (`MobilePersonNPC.cs:144-156`): a guard
/// **forces** male and outfit 0, so that
/// `P(guard ∧ male ∧ outfit 0) = P(guard) = 1/32` (the branches are not
/// independent, SPEC §2.2 C4).
pub fn roll_appearance(guard_roll: u32, gender_roll: u32, outfit_roll: u32) -> Appearance {
    if guard_roll.is_multiple_of(GUARD_ONE_IN) {
        return Appearance {
            guard: true,
            gender: Gender::Male,
            outfit: 0,
        };
    }
    Appearance {
        guard: false,
        gender: Gender::draw(gender_roll),
        outfit: (outfit_roll % u32::from(OUTFIT_VARIANTS)) as u8,
    }
}

/// Expected guards in a population of `count` NPCs (`count/32`).
pub fn expected_guards(count: usize) -> f32 {
    count as f32 / GUARD_ONE_IN as f32
}

/// Walk direction (`MobileDirection`, weight-array order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Direction {
    /// Index 0.
    North = 0,
    /// Index 1.
    South = 1,
    /// Index 2.
    East = 2,
    /// Index 3.
    West = 3,
}

impl Direction {
    /// The four directions in index order.
    pub const ALL: [Self; 4] = [Self::North, Self::South, Self::East, Self::West];

    /// `(dx, dy)` delta of the direction.
    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::North => (0, 1),
            Self::South => (0, -1),
            Self::East => (1, 0),
            Self::West => (-1, 0),
        }
    }

    /// Direction by index 0..3.
    pub fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }
}

/// Mobile state (`MobileStates`, `MobilePersonMotor.cs:38-40`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiState {
    /// Weighs the directions and picks the next cell.
    SeekingTile,
    /// Advances to the centre of the destination cell at 1.3 m/s.
    MovingForward,
}

/// Navgrid of a city: weight per cell (`GetNavGridWeightLocal`,
/// `CityNavigation.cs:360-400`). Outside the grid the weight is 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavGrid {
    width: usize,
    height: usize,
    weights: Vec<u8>,
}

impl NavGrid {
    /// Empty grid (all cells with weight 0).
    pub fn empty(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            weights: vec![0; width * height],
        }
    }

    /// Deterministic test grid for `blocks × blocks` blocks: block-edge
    /// streets with weight 4, lots with weight 1 and buildings (deterministic
    /// per-seed pattern) with weight 0. It is the scenario of the target
    /// routine (N-4): the high weight keeps the walk along streets.
    pub fn city(blocks: usize, seed: u64) -> Self {
        let side = nav_grid_side(blocks.max(1));
        let mut grid = Self::empty(side, side);
        let mut rng = SplitMix64::new(seed);
        const STREET_STRIP: usize = 6;
        for by in 0..blocks.max(1) {
            for bx in 0..blocks.max(1) {
                for local_y in 0..CELLS_PER_BLOCK {
                    for local_x in 0..CELLS_PER_BLOCK {
                        let street = local_x < STREET_STRIP
                            || local_y < STREET_STRIP
                            || local_x >= CELLS_PER_BLOCK - STREET_STRIP
                            || local_y >= CELLS_PER_BLOCK - STREET_STRIP;
                        let weight = if street {
                            4
                        } else if rng.below(100) < 15 {
                            0
                        } else {
                            1
                        };
                        let x = bx * CELLS_PER_BLOCK + local_x;
                        let y = by * CELLS_PER_BLOCK + local_y;
                        grid.set(x as i32, y as i32, weight);
                    }
                }
            }
        }
        grid
    }

    /// Side in cells.
    pub fn side(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Cell weight (`0` outside the grid or non-walkable).
    pub fn weight(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return 0;
        }
        self.weights[y as usize * self.width + x as usize]
    }

    /// Sets the weight of a cell (ignores coordinates outside the grid).
    pub fn set(&mut self, x: i32, y: i32, weight: u8) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        let index = y as usize * self.width + x as usize;
        self.weights[index] = weight;
    }

    /// Is the cell walkable? (`IsDirectionClear`: weight > 0 and inside the grid).
    pub fn is_clear(&self, x: i32, y: i32) -> bool {
        self.weight(x, y) > 0
    }
}

/// Result of one simulation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepOutcome {
    /// A `SeekingTile → MovingForward` transition completed.
    pub started_move: bool,
    /// A `MovingForward → SeekingTile` transition completed.
    pub arrived: bool,
    /// The `SeekingTile` state found no walkable cell (`seekCount++`).
    pub seek_failed: bool,
}

/// City mobile: `SeekingTile ↔ MovingForward` state machine
/// (`MobilePersonMotor.cs:303-375`) with a seeded generator (spec R-C:
/// same seed ⇒ same path).
#[derive(Debug, Clone, PartialEq)]
pub struct Walker {
    state: AiState,
    cell: (i32, i32),
    target: (i32, i32),
    direction: Direction,
    /// Meters advanced toward `target`.
    progress: f32,
    /// Cells visited in order (includes the start).
    path: Vec<(i32, i32)>,
    /// Completed `SeekingTile → MovingForward` transitions.
    moves: u32,
    /// Consecutive failed searches (`seekCount`).
    seek_failures: u32,
    rng: SplitMix64,
}

impl Walker {
    /// Mobile at `start` (must be a walkable cell of the grid).
    pub fn new(seed: u64, start: (i32, i32)) -> Self {
        Self {
            state: AiState::SeekingTile,
            cell: start,
            target: start,
            direction: Direction::North,
            progress: 0.0,
            path: vec![start],
            moves: 0,
            seek_failures: 0,
            rng: SplitMix64::new(seed),
        }
    }

    /// Current state.
    pub fn state(&self) -> AiState {
        self.state
    }

    /// Current cell.
    pub fn cell(&self) -> (i32, i32) {
        self.cell
    }

    /// Current direction.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Cells visited in order.
    pub fn path(&self) -> &[(i32, i32)] {
        &self.path
    }

    /// Distinct cells visited.
    pub fn cells_visited(&self) -> usize {
        let mut unique = self.path.clone();
        unique.sort_unstable();
        unique.dedup();
        unique.len()
    }

    /// `SeekingTile → MovingForward` transitions.
    pub fn moves(&self) -> u32 {
        self.moves
    }

    /// Consecutive failed searches (the `PopulationManager` recycles above
    /// [`SEEK_COUNT_RECYCLE_ABOVE`]).
    pub fn seek_failures(&self) -> u32 {
        self.seek_failures
    }

    /// FNV-1a 64 digest of the path + state (same seed ⇒ same integer).
    pub fn digest(&self) -> u64 {
        let mut bytes = Vec::with_capacity(self.path.len() * 8 + 24);
        for (x, y) in &self.path {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&y.to_le_bytes());
        }
        bytes.extend_from_slice(&self.cell.0.to_le_bytes());
        bytes.extend_from_slice(&self.cell.1.to_le_bytes());
        bytes.push(match self.direction {
            Direction::North => 0,
            Direction::South => 1,
            Direction::East => 2,
            Direction::West => 3,
        });
        bytes.push(match self.state {
            AiState::SeekingTile => 0,
            AiState::MovingForward => 1,
        });
        bytes.extend_from_slice(&self.moves.to_le_bytes());
        fnv1a64(&bytes)
    }

    /// One simulation step with `dt_seconds` (the target engine fixes `dt`,
    /// spec R-E; the population tick is 100 ms).
    pub fn step(&mut self, grid: &NavGrid, dt_seconds: f32) -> StepOutcome {
        match self.state {
            AiState::MovingForward => self.advance(dt_seconds),
            AiState::SeekingTile => self.seek(grid),
        }
    }

    fn advance(&mut self, dt_seconds: f32) -> StepOutcome {
        self.progress += NPC_SPEED_METERS_PER_SECOND * dt_seconds.max(0.0);
        if self.progress + ARRIVAL_EPSILON_METERS >= CELL_METERS {
            self.cell = self.target;
            self.progress = 0.0;
            self.path.push(self.cell);
            self.state = AiState::SeekingTile;
            return StepOutcome {
                started_move: false,
                arrived: true,
                seek_failed: false,
            };
        }
        StepOutcome {
            started_move: false,
            arrived: false,
            seek_failed: false,
        }
    }

    /// `SeekingTile` (`MobilePersonMotor.cs:303-345`), bug-for-bug with the
    /// source: the re-evaluation loop walks `Range(0, 3)` (N/S/E), never W.
    fn seek(&mut self, grid: &NavGrid) -> StepOutcome {
        let (x, y) = self.cell;
        let weights = Direction::ALL.map(|dir| {
            let (dx, dy) = dir.delta();
            grid.weight(x + dx, y + dy)
        });
        let current_weight = grid.weight(x, y);
        let (dx, dy) = self.direction.delta();
        let mut target_weight = if grid.is_clear(x + dx, y + dy) {
            weights[self.direction as usize] as i32
        } else {
            0
        };

        // Small chance of changing direction (keeps the walk shuffled:
        // `randomChangeChance = 0.025`).
        if self.rng.unit() < RANDOM_CHANGE_CHANCE {
            target_weight = 0;
        }

        if target_weight == 0 {
            let index = self.rng.below(weights.len() as u64) as usize;
            let dir = Direction::from_index(index);
            let (ndx, ndy) = dir.delta();
            if weights[index] == 0 || !grid.is_clear(x + ndx, y + ndy) {
                self.seek_failures += 1;
                return StepOutcome {
                    started_move: false,
                    arrived: false,
                    seek_failed: true,
                };
            }
            self.direction = dir;
            return self.begin_move(weights[index]);
        }

        if (target_weight as u8) < current_weight && self.rng.unit() > TILE_DOWNGRADE_CHANCE {
            // Re-evaluates in random order (3 of 4 indices, as in the source).
            let mut best_weight = target_weight;
            let mut best = self.direction;
            let mut order = [0usize, 1, 2];
            for i in (1..order.len()).rev() {
                let j = self.rng.below((i + 1) as u64) as usize;
                order.swap(i, j);
            }
            for i in order {
                let (odx, ody) = Direction::ALL[i].delta();
                if i32::from(weights[i]) > best_weight && grid.is_clear(x + odx, y + ody) {
                    best_weight = i32::from(weights[i]);
                    best = Direction::ALL[i];
                }
            }
            self.direction = best;
        }
        self.begin_move(weights[self.direction as usize])
    }

    fn begin_move(&mut self, weight: u8) -> StepOutcome {
        debug_assert!(weight > 0, "destination cell not walkable");
        let (dx, dy) = self.direction.delta();
        self.target = (self.cell.0 + dx, self.cell.1 + dy);
        self.progress = 0.0;
        self.seek_failures = 0;
        self.moves += 1;
        self.state = AiState::MovingForward;
        StepOutcome {
            started_move: true,
            arrived: false,
            seek_failed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn population_caps_match_spec_family_a() {
        assert_eq!(max_population(1, 1), 24);
        assert_eq!(max_population(16, 1), 24);
        assert_eq!(max_population(32, 1), 48);
        assert_eq!(max_population(48, 1), 72);
        assert_eq!(max_population(64, 1), 96);
        assert_eq!(max_population(1024, 1), 96);
        assert_eq!(tick_millis(), 100);
    }

    #[test]
    fn navigation_units_match_spec_family_b() {
        assert!((cell_meters() - 1.6).abs() < 1e-6);
        assert!((seconds_for_cells(1.0) - 1.230_769_2).abs() < 1e-5);
        assert!((seconds_for_cells(10.0) - 12.307_692).abs() < 1e-4);
        assert_eq!(nav_grid_side(8), 512);
        assert!((margin_cells() - 39.0625).abs() < 1e-6);
        assert_eq!((METERS_MARGIN_CHECK * 1000.0) as i32, 62_500);
    }

    /// `2500 u · 0.025 = 62.5 m`.
    const METERS_MARGIN_CHECK: f32 = MAX_PLAYER_DISTANCE_OUTSIDE_RECT as f32 * GLOBAL_SCALE;

    #[test]
    fn appearance_branches_guard_forces_male_outfit_zero() {
        let guard = roll_appearance(32, 1, 3);
        assert_eq!(
            guard,
            Appearance {
                guard: true,
                gender: Gender::Male,
                outfit: 0
            }
        );
        let civilian = roll_appearance(1, 1, 3);
        assert_eq!(
            civilian,
            Appearance {
                guard: false,
                gender: Gender::Female,
                outfit: 3
            }
        );
        assert!((expected_guards(96) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn walker_is_deterministic_and_moves_across_cells() {
        let grid = NavGrid::city(1, 7);
        let mut a = Walker::new(11, (3, 32));
        let mut b = Walker::new(11, (3, 32));
        for _ in 0..600 {
            a.step(&grid, 0.1);
            b.step(&grid, 0.1);
        }
        assert_eq!(a.digest(), b.digest());
        assert!(a.moves() > 0);
        assert!(a.cells_visited() >= 10);
    }

    #[test]
    fn walker_never_leaves_the_grid() {
        let grid = NavGrid::city(2, 5);
        let (w, h) = grid.side();
        let mut walker = Walker::new(3, (3, 3));
        for _ in 0..2_000 {
            walker.step(&grid, 0.1);
            let (x, y) = walker.cell();
            assert!(x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h);
            assert!(grid.is_clear(x, y));
        }
    }

    #[test]
    fn empty_grid_stalls_without_panicking() {
        let grid = NavGrid::empty(8, 8);
        let mut walker = Walker::new(1, (2, 2));
        let outcome = walker.step(&grid, 0.1);
        assert!(outcome.seek_failed);
        assert_eq!(walker.state(), AiState::SeekingTile);
    }
}
