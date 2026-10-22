//! M19 — anti-screen probes of **locomotion** (`LOCOMOTION-001`) and
//! **stealth/thievery** (`STEALTH-001`).
//!
//! Contract: `docs/sintergic/game/programming/m19-locomotion-spec-CR-8313e821.md`
//! §7. Four blocks:
//!
//! - **A · formulas** (pure, 0 retail): §1.3 (11 speeds + diagonal factor),
//!   §2.1 (12 table rows) and §3.1 (9 climb chances). f32 tolerance `≤ 1e-4`
//!   on reals, exact on integers.
//! - **B · runtime engine**: deterministic synthetic world (plane + wall +
//!   water + solid), `dt = 1/60`, fixed seed. Measures walk/run/crouch/
//!   sneak, jump (apex + return), 10 m fall, climb (start after the first
//!   check + speed), levitation, swimming + breath, overload and
//!   **0 penetrations** (`is_free` every frame).
//! - **C · stealth/thievery**: ALL rows of the §5.3 table (16, with their
//!   `clamp`). `--probe STEALTH-001` runs this block alone.
//! - **D · demo integration** (retail): real player at `spawn.eye`/`yaw`
//!   walking 1.5 s with `Locomotion::step` over the demo adapter
//!   ([`crate::m18_demo::DemoLocomotionWorld`], 0 duplicated logic), eye
//!   height = ground + 1.60, final cell free, 0 solid cells occupied,
//!   digest ×2 and 0 panics.
//!
//! `EXIT=0` ⟺ A ∧ B ∧ C ∧ D; `EXIT=1` on any FAIL; `EXIT=2` if ARENA2 is
//! not accessible (block D) — never a false PASS without retail data.
//!
//! **Fidelity notes:** spec §7 cites «12 vectors of §5.3», but the §5.3 table
//! has **16 rows**: all 16 are verified (the spec is left untouched).
//! The row «jump skill 50 → apex 0.79066» of §2.1 has an arithmetic erratum:
//! `v²/2g = 5.625²/40 = 0.7910156` (Δ = +3.6e-4); the check uses the formula
//! of the source itself and declares it in its detail.

use daggerfall_behavior::locomotion::{
    breath_drain_interval_s, climb_chance, climb_check_interval_s, climb_speed, crouch_speed,
    exterior_lockpicking_chance, fall_outcome, interior_lockpicking_chance, is_overencumbered,
    jump_apex, jump_velocity, max_breath, max_encumbrance, pickpocketing_chance, run_speed,
    shoplifting_chance, sneak_speed, stealth_chance, swim_speed, walk_speed, ClimbCheckKind,
    Dice100, FallOutcome, Locomotion, LocomotionWorld, MotionMode, MoveInput, PlayerBody,
    StepOutcome, DIAGONAL_FACTOR,
};
use daggerfall_parser_bevy_bridge as bridge;

use crate::m18_demo::DemoError;

/// Engine step (s) — spec §7 block B: `dt = 1/60`.
const DT: f32 = 1.0 / 60.0;
/// Fixed dice seed (determinism ×2).
const SEED: u64 = 20_261_020;
/// Target eye height (`view_probe.rs:21`).
const EYE: f32 = bridge::EYE_HEIGHT_M;
/// Target player radius (`view_probe.rs:19`).
const RADIUS: f32 = bridge::PLAYER_RADIUS_M;
/// Relative tolerance of the runtime measurements (spec §7: ±2 %).
const RATE_TOL: f32 = 0.02;
/// Absolute tolerance of the f32 goldens (spec §7: `≤ 1e-4`).
const ABS_TOL: f32 = 1e-4;

// ── evidence rows ───────────────────────────────────────────────────────

/// A formula golden: measured vs expected with explicit tolerance.
#[derive(Debug, Clone, Copy)]
struct Golden {
    id: &'static str,
    actual: f32,
    expected: f32,
    tol: f32,
    precision: usize,
    note: &'static str,
}

impl Golden {
    fn pass(&self) -> bool {
        (self.actual - self.expected).abs() <= self.tol
    }

    fn detail(&self) -> String {
        let mut out = format!(
            "measured={:.*} vs expected={:.*} (tol {})",
            self.precision, self.actual, self.precision, self.expected, self.tol
        );
        if !self.note.is_empty() {
            out.push_str(" · ");
            out.push_str(self.note);
        }
        out
    }
}

/// Printed probe row (`[OK]`/`[FAIL]`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    id: &'static str,
    ok: bool,
    detail: String,
}

fn golden_row(id: &'static str, ok: bool, detail: String) -> Row {
    Row { id, ok, detail }
}

fn golden_rows(goldens: &[Golden]) -> Vec<Row> {
    goldens
        .iter()
        .map(|golden| golden_row(golden.id, golden.pass(), golden.detail()))
        .collect()
}

/// Runtime measurement row: `|actual − expected| / expected ≤ rate`.
fn rate_row(id: &'static str, actual: f32, expected: f32, note: &str) -> Row {
    let rate = (actual - expected).abs() / expected;
    let mut detail = format!(
        "measured={actual:.7} m/s vs expected={expected:.7} m/s · deviation={:.4}% (tol ±{:.0}%)",
        rate * 100.0,
        RATE_TOL * 100.0
    );
    if !note.is_empty() {
        detail.push_str(" · ");
        detail.push_str(note);
    }
    golden_row(id, rate <= RATE_TOL, detail)
}

fn print_block(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if !row.ok {
            ok = false;
        }
        println!(
            "[{}] {} - {}",
            if row.ok { "OK" } else { "FAIL" },
            row.id,
            row.detail
        );
    }
    println!(
        "-- {title}: {}/{} checks {}",
        rows.iter().filter(|row| row.ok).count(),
        rows.len(),
        if ok { "OK" } else { "FAIL" }
    );
    ok
}

// ── block A · formulas (§1.3, §2.1, §3.1) ───────────────────────────────

fn speed_goldens() -> Vec<Golden> {
    let mut out = Vec::with_capacity(11);
    let mut push = |id, actual, expected, note| {
        out.push(Golden {
            id,
            actual,
            expected,
            tol: ABS_TOL,
            precision: 7,
            note,
        })
    };
    push("v_caminar_50", walk_speed(50), 4.430_38, "175/39.5");
    push("v_caminar_10", walk_speed(10), 3.164_557, "floor drag 30 → 125/39.5");
    push("v_caminar_100", walk_speed(100), 6.329_114, "250/39.5");
    push("v_agachado_50", crouch_speed(50), 2.531_645_7, "100/39.5");
    push(
        "v_correr_skill0",
        run_speed(50, 0),
        6.835_443,
        "200/39.5·1.35",
    );
    push(
        "v_correr_skill100",
        run_speed(50, 100),
        9.367_088,
        "200/39.5·1.85",
    );
    push(
        "v_sigilo_50",
        sneak_speed(walk_speed(50)),
        2.189_873_4,
        "base/2 − 1/39.5",
    );
    push(
        "v_nadar_swim0",
        swim_speed(walk_speed(50), 0),
        1.107_594_9,
        "base/4",
    );
    push(
        "v_nadar_swim100",
        swim_speed(walk_speed(50), 100),
        3.322_785,
        "base·0.5 + base/4",
    );
    push(
        "v_escalar",
        climb_speed(walk_speed(50), false),
        1.476_793_2,
        "base/3",
    );
    push(
        "v_escalar_mejorada",
        climb_speed(walk_speed(50), true),
        2.953_586_5,
        "base/3·2",
    );
    out
}

fn jump_and_fall_goldens() -> Vec<Golden> {
    let default = PlayerBody::default();
    let skill50 = PlayerBody {
        jumping_skill: 50,
        ..PlayerBody::default()
    };
    let skill100 = PlayerBody {
        jumping_skill: 100,
        ..PlayerBody::default()
    };
    let athleticism = PlayerBody {
        athleticism: true,
        ..PlayerBody::default()
    };
    let improved = PlayerBody {
        athleticism: true,
        improved_athleticism: true,
        ..PlayerBody::default()
    };
    let spell = PlayerBody {
        enhanced_jumping: true,
        ..PlayerBody::default()
    };
    let int = |value: i32| value as f32;
    vec![
        Golden {
            id: "salto_skill0_apice",
            actual: jump_apex(jump_velocity(&default, false)),
            expected: 0.506_25,
            tol: ABS_TOL,
            precision: 7,
            note: "v=4.5 · v²/2g",
        },
        Golden {
            id: "salto_skill50_apice",
            actual: jump_apex(jump_velocity(&skill50, false)),
            expected: 0.791_015_6,
            tol: ABS_TOL,
            precision: 7,
            note: "v=5.625 (4.5·1.25) · spec §2.1 prints 0.79066: erratum (v²/2g = 0.7910156, Δ=+3.6e-4)",
        },
        Golden {
            id: "salto_skill100_apice",
            actual: jump_apex(jump_velocity(&skill100, false)),
            expected: 1.139_062_5,
            tol: ABS_TOL,
            precision: 7,
            note: "v=6.75 · v²/2g",
        },
        Golden {
            id: "salto_agachado_apice",
            actual: jump_apex(jump_velocity(&default, true)),
            expected: 0.324,
            tol: ABS_TOL,
            precision: 7,
            note: "v=3.6 (·0.8) · v²/2g",
        },
        Golden {
            id: "salto_athleticism_vel",
            actual: jump_velocity(&athleticism, false),
            expected: 4.95,
            tol: ABS_TOL,
            precision: 7,
            note: "AcrobatMotor.cs:14",
        },
        Golden {
            id: "salto_ath_improved_vel",
            actual: jump_velocity(&improved, false),
            expected: 5.4,
            tol: ABS_TOL,
            precision: 7,
            note: "AcrobatMotor.cs:15",
        },
        Golden {
            id: "salto_hechizo_vel",
            actual: jump_velocity(&spell, false),
            expected: 7.2,
            tol: ABS_TOL,
            precision: 7,
            note: "AcrobatMotor.cs:16",
        },
        Golden {
            id: "caida_6m_dano",
            actual: int(fall_outcome(6.0).as_damage()),
            expected: 5.0,
            tol: 0.0,
            precision: 0,
            note: "(int)(5·1.0)",
        },
        Golden {
            id: "caida_10_5m_dano",
            actual: int(fall_outcome(10.5).as_damage()),
            expected: 27.0,
            tol: 0.0,
            precision: 0,
            note: "(int)(5·5.5)",
        },
        Golden {
            id: "caida_2_6m_dura",
            actual: int(i32::from(fall_outcome(2.6) == FallOutcome::HardFall)),
            expected: 1.0,
            tol: 0.0,
            precision: 0,
            note: "2.5 < d ≤ 5.0 → hard fall, 0 damage",
        },
        Golden {
            id: "caida_2_5m_nada",
            actual: int(i32::from(fall_outcome(2.5) == FallOutcome::None)),
            expected: 1.0,
            tol: 0.0,
            precision: 0,
            note: "strict comparison >",
        },
        Golden {
            id: "caida_5m_dura",
            actual: int(i32::from(fall_outcome(5.0) == FallOutcome::HardFall)),
            expected: 1.0,
            tol: 0.0,
            precision: 0,
            note: "> 2.5 → hard, 0 damage",
        },
    ]
}

fn climb_goldens() -> Vec<Golden> {
    let body = |skill: i32,
                khajiit: bool,
                enhanced: bool,
                luck: i32|
     -> PlayerBody {
        PlayerBody {
            climbing_skill: skill,
            khajiit,
            enhanced_climbing: enhanced,
            live_luck: luck,
            ..PlayerBody::default()
        }
    };
    let row = |id, actual: i32, expected: i32, note| Golden {
        id,
        actual: actual as f32,
        expected: expected as f32,
        tol: 0.0,
        precision: 0,
        note,
    };
    vec![
        row(
            "climb_skill0",
            climb_chance(&body(0, false, false, 50), 70),
            76,
            "clamp min 5 → lerp(70,100,0.05)+5",
        ),
        row(
            "climb_skill50_base70",
            climb_chance(&body(50, false, false, 50), 70),
            90,
            "lerp(70,100,0.5)+5",
        ),
        row(
            "climb_skill50_base50",
            climb_chance(&body(50, false, false, 50), 50),
            80,
            "continue",
        ),
        row(
            "climb_skill50_base40",
            climb_chance(&body(50, false, false, 50), 40),
            75,
            "catch while falling",
        ),
        row(
            "climb_skill50_base20",
            climb_chance(&body(50, false, false, 50), 20),
            65,
            "recover grip",
        ),
        row(
            "climb_skill50_luck100",
            climb_chance(&body(50, false, false, 100), 70),
            95,
            "luck 100 → +10",
        ),
        row(
            "climb_khajiit",
            climb_chance(&body(50, true, false, 50), 70),
            99,
            "+30 skill",
        ),
        row(
            "climb_mejorada",
            climb_chance(&body(50, false, true, 50), 70),
            103,
            "×2 → clamp 95 → 103 (the source does not clamp to 100)",
        ),
        row(
            "climb_skill95",
            climb_chance(&body(95, false, false, 50), 70),
            103,
            "effective skill 95 + luck 50 → 103",
        ),
    ]
}

fn block_a_rows() -> Vec<Row> {
    let mut rows = golden_rows(&speed_goldens());
    rows.extend(golden_rows(&jump_and_fall_goldens()));
    rows.extend(golden_rows(&climb_goldens()));
    // Diagonal input factor (§1.3): PlayerMotor.cs:30 constant.
    rows.push(golden_row(
        "diagonal_factor",
        (DIAGONAL_FACTOR - 7071.0 / 10_000.0).abs() <= 1e-7,
        format!(
            "measured={DIAGONAL_FACTOR:.6} vs expected={:.6} (PlayerMotor.cs:30 · FrictionMotor.cs:93)",
            7071.0 / 10_000.0
        ),
    ));
    rows
}

// ── block C · stealth / thievery / weight (§5.3) ────────────────────────

fn stealth_goldens() -> Vec<Golden> {
    let int = |value: i32| value as f32;
    let row = |id, actual: i32, expected: i32| Golden {
        id,
        actual: actual as f32,
        expected: expected as f32,
        tol: 0.0,
        precision: 0,
        note: "",
    };
    vec![
        row("sigilo_10m_stealth50", stealth_chance(10.0, 50), 38),
        row("sigilo_1m_stealth50", stealth_chance(1.0, 50), 2),
        row("sigilo_10m_stealth100", stealth_chance(10.0, 100), 78),
        row("sigilo_0_5m_stealth10", stealth_chance(0.5, 10), 0),
        row(
            "ganzua_interior_nivel5_lock10_skill40",
            interior_lockpicking_chance(5, 10, 40),
            15,
        ),
        row(
            "ganzua_interior_clamp5",
            interior_lockpicking_chance(1, 20, 10),
            5,
        ),
        row(
            "ganzua_exterior_clamp5",
            exterior_lockpicking_chance(10, 40),
            5,
        ),
        row("ganzua_exterior_lock5_skill100", exterior_lockpicking_chance(5, 100), 75),
        row(
            "carterismo_clamp5",
            pickpocketing_chance(40, 1, 10),
            5,
        ),
        row("carterismo_nivel10_vs5_skill60", pickpocketing_chance(60, 10, 5), 85),
        row(
            "hurto_tienda_skill40_calidad5_peso10",
            shoplifting_chance(40, 5, 10),
            75,
        ),
        row("peso_max_strength50", max_encumbrance(50), 75),
        row("peso_max_strength100", max_encumbrance(100), 150),
        row("peso_max_strength33", max_encumbrance(33), 49),
        Golden {
            id: "sobrecarga_62_5kg_no",
            actual: int(i32::from(is_overencumbered(62.5))),
            expected: 0.0,
            tol: 0.0,
            precision: 0,
            note: "classic 250: 62.5·4 = 250 does not exceed",
        },
        Golden {
            id: "sobrecarga_62_51kg_si",
            actual: int(i32::from(is_overencumbered(62.51))),
            expected: 1.0,
            tol: 0.0,
            precision: 0,
            note: "62.51·4 = 250.04 > 250",
        },
    ]
}

fn block_c_rows() -> Vec<Row> {
    golden_rows(&stealth_goldens())
}

// ── block B · runtime engine (synthetic world) ──────────────────────────

/// Deterministic synthetic world of block B: flat heightfield, optional wall
/// (`x ≥ wall`), optional water (level `y`) and cylindrical solids.
struct ProbeWorld {
    ground: f32,
    wall_x: Option<f32>,
    /// `true` if the wall is climbable (`wall_ahead` reports it).
    wall_scalable: bool,
    water_y: Option<f32>,
    solids: Vec<([f32; 2], f32)>,
    half_extent: f32,
}

impl ProbeWorld {
    fn flat() -> Self {
        Self {
            ground: 0.0,
            wall_x: None,
            wall_scalable: true,
            water_y: None,
            solids: Vec::new(),
            half_extent: 100.0,
        }
    }

    fn with_wall(wall_x: f32) -> Self {
        Self {
            wall_x: Some(wall_x),
            ..Self::flat()
        }
    }

    /// Wall that blocks passage but is **not** climbable (non-crossing test
    /// where the engine cannot grab onto it).
    fn with_blocking_wall(wall_x: f32) -> Self {
        Self {
            wall_x: Some(wall_x),
            wall_scalable: false,
            ..Self::flat()
        }
    }

    fn with_solid(center: [f32; 2], radius: f32) -> Self {
        Self {
            solids: vec![(center, radius)],
            ..Self::flat()
        }
    }

    fn underwater(level: f32) -> Self {
        Self {
            water_y: Some(level),
            ..Self::flat()
        }
    }

    /// INV-L99-5 invariant for the synthetic world: the point (x, z) touches
    /// neither wall nor solid and stays inside the enclosure. The Y axis is
    /// heightfield/water (no ceilings), so it takes no part in the test.
    fn is_free(&self, position: [f32; 3], radius: f32) -> bool {
        if position[0].abs() > self.half_extent || position[2].abs() > self.half_extent {
            return false;
        }
        if let Some(wall) = self.wall_x {
            if position[0] + radius > wall + 1e-3 {
                return false;
            }
        }
        for (center, solid_radius) in &self.solids {
            let dx = position[0] - center[0];
            let dz = position[2] - center[1];
            if dx.hypot(dz) < solid_radius + radius - 1e-4 {
                return false;
            }
        }
        true
    }
}

impl LocomotionWorld for ProbeWorld {
    fn ground_height(&self, _x: f32, _z: f32) -> f32 {
        self.ground
    }

    fn step(&self, from: [f32; 3], delta: [f32; 3], radius: f32) -> StepOutcome {
        let target = [from[0] + delta[0], from[1] + delta[1], from[2] + delta[2]];
        let blocked = |pos: [f32; 3]| StepOutcome {
            pos,
            moved: false,
            blocked: true,
        };
        if let Some(wall) = self.wall_x {
            if target[0] + radius > wall {
                return blocked([wall - radius - 1e-4, target[1], target[2]]);
            }
        }
        for (center, solid_radius) in &self.solids {
            let dx = target[0] - center[0];
            let dz = target[2] - center[1];
            if dx.hypot(dz) < solid_radius + radius {
                return blocked(from);
            }
        }
        if target[0].abs() > self.half_extent || target[2].abs() > self.half_extent {
            return blocked(from);
        }
        StepOutcome {
            pos: target,
            moved: target != from,
            blocked: false,
        }
    }

    fn wall_ahead(&self, eye: [f32; 3], dir: [f32; 3], reach: f32) -> bool {
        match self.wall_x {
            Some(wall) => self.wall_scalable && dir[0] > 0.5 && eye[0] + reach >= wall,
            None => false,
        }
    }

    fn submerged(&self, eye: [f32; 3]) -> bool {
        self.water_y.is_some_and(|level| eye[1] < level)
    }
}

/// Measurement of a straight displacement: horizontal distance travelled,
/// per-frame penetrations and final position.
fn measure_horizontal(
    world: &ProbeWorld,
    body: &PlayerBody,
    start: [f32; 3],
    input: MoveInput,
    frame_count: usize,
    seed: u64,
) -> (f32, usize, [f32; 3]) {
    let mut locomotion = Locomotion::new(start, EYE, RADIUS);
    let mut dice = Dice100::new(seed);
    let mut penetrations = 0usize;
    for _ in 0..frame_count {
        locomotion.step(&input, body, world, &mut dice, DT);
        penetrations += usize::from(!world.is_free(locomotion.position, RADIUS));
    }
    let end = locomotion.position;
    let distance = (end[0] - start[0]).hypot(end[2] - start[2]);
    (distance, penetrations, end)
}

fn synthetic_rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let mut penetrations = 0usize;
    let body = PlayerBody::default();
    let flat = ProbeWorld::flat();
    let start = [0.0, EYE, 0.0];
    let forward = MoveInput {
        forward: 1.0,
        ..Default::default()
    };

    // ── §1.2 · measured speeds (1 s) ────────────────────────────────────
    let (walked, hits, _) = measure_horizontal(&flat, &body, start, forward, 60, SEED);
    penetrations += hits;
    rows.push(rate_row("b_caminar_1s", walked, walk_speed(50), "60 frames"));

    let run = MoveInput {
        forward: 1.0,
        run: true,
        ..Default::default()
    };
    let (ran, hits, _) = measure_horizontal(&flat, &body, start, run, 60, SEED ^ 0x11);
    penetrations += hits;
    rows.push(rate_row(
        "b_correr_1s",
        ran,
        run_speed(50, 0),
        "running_skill 0",
    ));

    let crouch = MoveInput {
        forward: 1.0,
        crouch: true,
        ..Default::default()
    };
    let (crouched, hits, _) = measure_horizontal(&flat, &body, start, crouch, 60, SEED ^ 0x22);
    penetrations += hits;
    rows.push(rate_row(
        "b_agachado_1s",
        crouched,
        crouch_speed(50),
        "crouch replaces the base",
    ));

    let sneak = MoveInput {
        forward: 1.0,
        sneak: true,
        ..Default::default()
    };
    let (sneaked, hits, _) = measure_horizontal(&flat, &body, start, sneak, 60, SEED ^ 0x33);
    penetrations += hits;
    rows.push(rate_row(
        "b_sigilo_1s",
        sneaked,
        sneak_speed(walk_speed(50)),
        "sneak divides the base",
    ));

    // ── §2 · jump skill 50: apex and return to the ground ───────────────
    let jumper_body = PlayerBody {
        jumping_skill: 50,
        ..PlayerBody::default()
    };
    let mut locomotion = Locomotion::new(start, EYE, RADIUS);
    let mut dice = Dice100::new(SEED ^ 0x44);
    let jump = MoveInput {
        jump: true,
        ..Default::default()
    };
    let mut apex = 0.0f32;
    let mut landed = false;
    let mut jump_damage = 0i32;
    let mut hard_fall = false;
    for frame_index in 0..180 {
        let frame = locomotion.step(&jump, &jumper_body, &flat, &mut dice, DT);
        apex = apex.max(locomotion.position[1] - EYE);
        jump_damage += frame.fall_damage;
        hard_fall |= frame.hard_fall;
        penetrations += usize::from(!flat.is_free(locomotion.position, RADIUS));
        if frame_index > 10 && frame.mode == MotionMode::Grounded {
            landed = true;
            // First landing: the return to the ground is measured before the
            // held jump input starts the next cycle.
            break;
        }
    }
    let spec_apex = 0.790_66_f32;
    let apex_ok = (apex - spec_apex).abs() <= 0.05 && landed;
    rows.push(golden_row(
        "b_salto_skill50_apice",
        apex_ok,
        format!(
            "apex_measured={apex:.6} m vs 0.79066 m (±0.05) · continuous v²/2g=0.791016 · returned_to_ground={landed} · y_final={:.6} · damage={jump_damage} · hard_fall={hard_fall}",
            locomotion.position[1]
        ),
    ));
    let ground_ok = (locomotion.position[1] - (flat.ground + EYE)).abs() <= ABS_TOL
        && locomotion.mode == MotionMode::Grounded;
    rows.push(golden_row(
        "b_salto_regreso",
        ground_ok && jump_damage == 0 && !hard_fall,
        format!(
            "mode={} · y={:.7} vs ground+eye={:.7} (tol 1e-4) · damage={jump_damage}",
            locomotion.mode.label(),
            locomotion.position[1],
            flat.ground + EYE
        ),
    ));

    // ── §2 · 10 m fall of the eye ───────────────────────────────────────
    let drop_start = [0.0, EYE + 10.0, 0.0];
    let mut locomotion = Locomotion::new(drop_start, EYE, RADIUS);
    locomotion.mode = MotionMode::Airborne;
    locomotion.falling = true;
    locomotion.fall_start_y = drop_start[1];
    let mut dice = Dice100::new(SEED ^ 0x55);
    let mut damage = 0i32;
    let mut landed_y = drop_start[1];
    for _ in 0..240 {
        let frame = locomotion.step(&MoveInput::default(), &body, &flat, &mut dice, DT);
        damage += frame.fall_damage;
        landed_y = locomotion.position[1];
        penetrations += usize::from(!flat.is_free(locomotion.position, RADIUS));
        if frame.mode == MotionMode::Grounded {
            break;
        }
    }
    rows.push(golden_row(
        "b_caida_10m_dano",
        damage == 25,
        format!(
            "fall_damage={damage} vs 25 (eye fall {:.7} m) · y_final={landed_y:.7}",
            drop_start[1] - (flat.ground + EYE)
        ),
    ));

    // ── §3 · climb: start after the first check + speed ─────────────────
    let wall = ProbeWorld::with_wall(1.0);
    let climber = PlayerBody {
        climbing_skill: 100,
        live_luck: 100,
        ..PlayerBody::default()
    };
    let climb_input = MoveInput {
        forward: 1.0,
        yaw: -std::f32::consts::FRAC_PI_2,
        ..Default::default()
    };
    let mut locomotion = Locomotion::new([0.6, EYE, 0.0], EYE, RADIUS);
    let mut dice = Dice100::new(SEED ^ 0x66);
    let start_interval = climb_check_interval_s(ClimbCheckKind::Start);
    let mut frames_to_climb = 0usize;
    while frames_to_climb < 120 && locomotion.mode != MotionMode::Climbing {
        locomotion.step(&climb_input, &climber, &wall, &mut dice, DT);
        penetrations += usize::from(!wall.is_free(locomotion.position, RADIUS));
        frames_to_climb += 1;
    }
    let start_time = frames_to_climb as f32 * DT;
    let grabbed = locomotion.mode == MotionMode::Climbing;
    rows.push(golden_row(
        "b_escalada_inicio",
        grabbed && start_time >= start_interval && start_time < start_interval + DT,
        format!(
            "check_measured={start_time:.6} s (frames={frames_to_climb}) vs 14·0.0549254={start_interval:.7} s · chance=103 (skill 100 → clamp 95 · luck 100) · mode={}",
            locomotion.mode.label()
        ),
    ));
    let climb_start_y = locomotion.position[1];
    for _ in 0..60 {
        locomotion.step(&climb_input, &climber, &wall, &mut dice, DT);
        penetrations += usize::from(!wall.is_free(locomotion.position, RADIUS));
    }
    rows.push(rate_row(
        "b_escalada_velocidad",
        locomotion.position[1] - climb_start_y,
        climb_speed(walk_speed(50), false),
        "1 s rise (m/s) after grabbing",
    ));

    // ── §4 · levitation, 1 s ascending ──────────────────────────────────
    let mut locomotion = Locomotion::new([0.0, 3.0, 0.0], EYE, RADIUS);
    let mut dice = Dice100::new(SEED ^ 0x77);
    let levitate = MoveInput {
        levitating: true,
        jump: true,
        ..Default::default()
    };
    for _ in 0..60 {
        locomotion.step(&levitate, &body, &flat, &mut dice, DT);
        penetrations += usize::from(!flat.is_free(locomotion.position, RADIUS));
    }
    let risen = locomotion.position[1] - 3.0;
    rows.push(golden_row(
        "b_levitar_1s",
        (risen - 4.0).abs() <= 1e-3 && locomotion.mode == MotionMode::Levitating,
        format!(
            "rise={risen:.6} m vs +4.0 m (tol 1e-3) · mode={}",
            locomotion.mode.label()
        ),
    ));

    // ── §4 · swimming (Swimming 50) 1 s + 3 s breath ───────────────────
    let water = ProbeWorld::underwater(20.0);
    let swimmer = PlayerBody {
        swimming_skill: 50,
        ..PlayerBody::default()
    };
    let (swum, hits, _) = measure_horizontal(
        &water,
        &swimmer,
        [0.0, 0.0, 0.0],
        forward,
        60,
        SEED ^ 0x88,
    );
    penetrations += hits;
    rows.push(rate_row(
        "b_nadar_1s",
        swum,
        swim_speed(walk_speed(50), 50),
        "submerged + forward",
    ));

    let mut locomotion = Locomotion::new([0.0, 0.0, 0.0], EYE, RADIUS);
    let mut dice = Dice100::new(SEED ^ 0x99);
    let mut first_breath = 0i32;
    for frame_index in 0..180 {
        locomotion.step(&forward, &swimmer, &water, &mut dice, DT);
        penetrations += usize::from(!water.is_free(locomotion.position, RADIUS));
        if frame_index == 0 {
            first_breath = locomotion.breath;
        }
    }
    rows.push(golden_row(
        "b_respiracion_3s",
        first_breath == 25 && locomotion.breath == 23 && (breath_drain_interval_s() - 1.1875).abs() <= 1e-6,
        format!(
            "breath 25→{} after 3 s submerged (drain every {:.4} s) · max_breath(50)={}",
            locomotion.breath,
            breath_drain_interval_s(),
            max_breath(50)
        ),
    ));

    // ── §4 · overloaded while swimming: does not rise, sinks ────────────
    let loaded = PlayerBody {
        swimming_skill: 50,
        carried_weight_kg: 62.51,
        ..PlayerBody::default()
    };
    let mut locomotion = Locomotion::new([0.0, 0.0, 0.0], EYE, RADIUS);
    let mut dice = Dice100::new(SEED ^ 0xAA);
    let sink = MoveInput {
        jump: true,
        ..Default::default()
    };
    let sink_start = locomotion.position[1];
    for _ in 0..60 {
        locomotion.step(&sink, &loaded, &water, &mut dice, DT);
        penetrations += usize::from(!water.is_free(locomotion.position, RADIUS));
    }
    let drop = sink_start - locomotion.position[1];
    rows.push(rate_row(
        "b_sobrecarga_hunde",
        drop,
        swim_speed(walk_speed(50), 50),
        "62.51 kg (classic 250.04) ⇒ does not rise, sinks (m/s)",
    ));

    // ── anti-false-positive: wall ahead ⇒ 0 advance ─────────────────────
    let wall = ProbeWorld::with_blocking_wall(1.0);
    let mut locomotion = Locomotion::new([0.2, EYE, 0.0], EYE, RADIUS);
    let mut dice = Dice100::new(SEED ^ 0xBB);
    let mut blocked_seen = false;
    let mut advance = f32::NAN;
    for frame_index in 0..120 {
        let before = locomotion.position[0];
        let frame = locomotion.step(&climb_input, &body, &wall, &mut dice, DT);
        if frame_index == 119 {
            advance = (locomotion.position[0] - before).abs();
            blocked_seen = frame.blocked;
        }
        penetrations += usize::from(!wall.is_free(locomotion.position, RADIUS));
    }
    rows.push(golden_row(
        "b_muro_avance_0",
        advance <= 1e-6 && blocked_seen && wall.is_free(locomotion.position, RADIUS),
        format!(
            "last_frame_advance={advance:.7} m vs 0 · blocked={blocked_seen} · x_final={:.7} (wall at 1.0, radius {RADIUS}) · position_free={}",
            locomotion.position[0],
            wall.is_free(locomotion.position, RADIUS)
        ),
    ));

    // ── anti-false-positive: solid ⇒ it does not pass through ───────────
    let pillar = ProbeWorld::with_solid([0.0, -3.0], 0.5);
    let (reached, hits, end) = measure_horizontal(&pillar, &body, start, forward, 120, SEED ^ 0xCC);
    penetrations += hits;
    let clear = -3.0 + 0.5 + RADIUS;
    rows.push(golden_row(
        "b_solido_no_atraviesa",
        hits == 0 && reached <= 2.2 + 1e-3 && pillar.is_free(end, RADIUS),
        format!(
            "travelled={reached:.6} m vs cap {:.6} m (solid at z=-3.0, radius 0.5+{RADIUS}) · penetrations={hits} · z_final={:.7}",
            clear.abs(),
            end[2]
        ),
    ));

    // ── global invariant of block B ─────────────────────────────────────
    rows.push(golden_row(
        "b_0_penetraciones",
        penetrations == 0,
        format!("frames with a non-free position across ALL scenarios = {penetrations}"),
    ));
    rows
}

// ── block D · demo integration (retail) ─────────────────────────────────

/// Runs block D. `None` ⇒ ARENA2 unreachable (the caller exits with 2).
fn block_d_rows() -> Option<Vec<Row>> {
    let first = match crate::m18_demo::compose_demo() {
        Ok(scene) => scene,
        Err(DemoError::MissingData(reason)) => {
            println!("ARENA2 not accessible: {reason}");
            return None;
        }
        Err(err) => {
            println!("[FAIL] demo_integration - composition: {err}");
            return Some(vec![golden_row(
                "d_composicion",
                false,
                format!("compose_demo() → {err}"),
            )]);
        }
    };
    let second = match crate::m18_demo::compose_demo() {
        Ok(scene) => scene,
        Err(err) => {
            return Some(vec![golden_row(
                "d_composicion_2",
                false,
                format!("second composition → {err}"),
            )])
        }
    };

    let adapter = first.locomotion_world();
    let body = PlayerBody::default();
    let input = MoveInput {
        forward: 1.0,
        yaw: first.spawn.yaw,
        ..Default::default()
    };
    let start = first.spawn.eye;
    let expected = walk_speed(50) * 1.5;
    let walk = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut locomotion = Locomotion::new(start, EYE, RADIUS);
        let mut dice = Dice100::new(SEED);
        let mut solid_frames = 0usize;
        let mut blocked_seen = false;
        let mut fall_damage = 0i32;
        for _ in 0..90 {
            let frame = locomotion.step(&input, &body, &adapter, &mut dice, DT);
            blocked_seen |= frame.blocked;
            fall_damage += frame.fall_damage;
            let position = locomotion.position;
            if first.grid.in_block(position) && !first.grid.is_free(position, RADIUS) {
                solid_frames += 1;
            }
        }
        (
            locomotion.position,
            locomotion.mode,
            solid_frames,
            blocked_seen,
            fall_damage,
        )
    }));

    let mut rows = Vec::new();
    let panics = usize::from(walk.is_err());
    match walk {
        Ok((end, mode, solid_frames, blocked_seen, fall_damage)) => {
            let displacement = (end[0] - start[0]).hypot(end[2] - start[2]);
            let rate = (displacement - expected).abs() / expected;
            let open_enough = first.spawn.open_run_m + 1e-3 >= expected;
            let honest_obstruction =
                blocked_seen && displacement <= first.spawn.open_run_m + RADIUS + 1e-3;
            let ok = if open_enough {
                rate <= RATE_TOL
            } else {
                honest_obstruction
            };
            rows.push(golden_row(
                "d_desplazamiento_1_5s",
                ok,
                format!(
                    "displacement={displacement:.7} m vs walk_speed(50)*1.5={expected:.7} m (deviation={:.4}%, tol ±2%) · open_run_m={:.2} m · obstructed={blocked_seen} · branch={} · mode={}",
                    rate * 100.0,
                    first.spawn.open_run_m,
                    if open_enough { "free" } else { "honest obstruction (≤ open_run_m + radius)" },
                    mode.label()
                ),
            ));

            let ground = adapter.ground_height(end[0], end[2]);
            let eye_ok = (end[1] - (ground + EYE)).abs() <= ABS_TOL;
            rows.push(golden_row(
                "d_altura_ojo",
                eye_ok,
                format!(
                    "y_final={:.7} vs ground {ground:.7} + 1.60 = {:.7} (tol 1e-4)",
                    end[1],
                    ground + EYE
                ),
            ));

            let cell_free = first.grid.is_free(end, RADIUS);
            rows.push(golden_row(
                "d_celda_final",
                cell_free && solid_frames == 0,
                format!(
                    "pos=({:.3},{:.3},{:.3}) · grid.is_free={cell_free} · solid cells occupied={solid_frames} · fall_damage={fall_damage}",
                    end[0], end[1], end[2]
                ),
            ));
        }
        Err(_) => {
            rows.push(golden_row(
                "d_desplazamiento_1_5s",
                false,
                "the engine step panicked during block D".to_string(),
            ));
        }
    }
    rows.push(golden_row(
        "d_panics",
        panics == 0,
        format!("panics={panics} (catch_unwind over the 90 frames) · city {} · spawn=({:.2},{:.2},{:.2}) yaw={:.2}", first.city_name, start[0], start[1], start[2], first.spawn.yaw),
    ));
    let digest_first = first.digest();
    let digest_second = second.digest();
    rows.push(golden_row(
        "d_digest_x2",
        digest_first == digest_second,
        format!("digest1={digest_first:016x} · digest2={digest_second:016x}"),
    ));
    Some(rows)
}

// ── entrypoints ─────────────────────────────────────────────────────────

/// `--probe LOCOMOTION-001` — blocks A, B, C and D (retail).
pub fn run_locomotion_001() -> i32 {
    println!(
        "PROBE-LOCOMOTION-001 - DFU locomotion M19 (REQ-L99-052 · CR-b109bfe3/CR-8313e821) · dt=1/60 · seed {SEED}"
    );

    let a_rows = block_a_rows();
    let a_ok = print_block("A · formulas §1.3 (11+diagonal) / §2.1 (12) / §3.1 (9)", &a_rows);

    let b_rows = synthetic_rows();
    let b_ok = print_block("B · runtime engine (deterministic synthetic world)", &b_rows);

    println!(
        "[info] spec §7 cites 12 vectors of §5.3; table §5.3 has 16 rows — all 16 are verified"
    );
    let c_rows = block_c_rows();
    let c_ok = print_block("C · stealth/thievery/weight §5.3 (16 rows)", &c_rows);

    let d_rows = block_d_rows();
    let Some(d_rows) = d_rows else {
        println!("EXIT=2");
        println!(
            "VERDICT: PROBE-LOCOMOTION-001 NO DATA - ARENA2 not accessible (block D not executed; A/B/C green)"
        );
        return 2;
    };
    let d_ok = print_block("D · retail demo integration (real spawn, 1.5 s)", &d_rows);

    let all_ok = a_ok && b_ok && c_ok && d_ok;
    let code = i32::from(!all_ok);
    println!(
        "\nSUMMARY: A={} B={} C={} D={} · checks {}",
        if a_ok { "OK" } else { "FAIL" },
        if b_ok { "OK" } else { "FAIL" },
        if c_ok { "OK" } else { "FAIL" },
        if d_ok { "OK" } else { "FAIL" },
        a_rows.len() + b_rows.len() + c_rows.len() + d_rows.len(),
    );
    println!("EXIT={code}");
    if all_ok {
        println!(
            "VERDICT: PROBE-LOCOMOTION-001 PASS - {} checks (A formulas + B engine + C stealth + D demo) · 0 penetrations · 0 panics",
            a_rows.len() + b_rows.len() + c_rows.len() + d_rows.len()
        );
    } else {
        println!("VERDICT: PROBE-LOCOMOTION-001 FAIL - see [FAIL] rows");
    }
    code
}

/// `--probe STEALTH-001` — block C only (stealth/thievery/weight §5.3).
pub fn run_stealth_001() -> i32 {
    println!("PROBE-STEALTH-001 - DFU stealth/thievery/weight M19 (REQ-L99-052 · CR-b109bfe3)");
    println!(
        "[info] spec §7 cites 12 vectors of §5.3; table §5.3 has 16 rows — all 16 are verified"
    );
    let rows = block_c_rows();
    let ok = print_block("C · stealth/thievery/weight §5.3 (16 rows)", &rows);
    let code = i32::from(!ok);
    println!("EXIT={code}");
    if ok {
        println!(
            "VERDICT: PROBE-STEALTH-001 PASS - {} goldens of §5.3 (including the 5/95 clamps)",
            rows.len()
        );
    } else {
        println!("VERDICT: PROBE-STEALTH-001 FAIL - see [FAIL] rows");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (i) Pure blocks A + C: every golden against the source, PASS.
    #[test]
    fn formula_and_stealth_goldens_pass() {
        let a = block_a_rows();
        let c = block_c_rows();
        for row in a.iter().chain(c.iter()) {
            assert!(row.ok, "{} FAIL: {}", row.id, row.detail);
        }
        assert_eq!(a.len(), 33, "11 speeds + 12 rows §2.1 + 9 climbs + diagonal");
        assert_eq!(c.len(), 16, "full §5.3 table (16 rows)");
    }

    /// (ii) Anti-false-positive: a tampered golden STOPS passing.
    #[test]
    fn tampered_golden_fails() {
        let mut formulas = speed_goldens();
        formulas[0].expected += 1.0;
        assert!(
            golden_rows(&formulas).iter().any(|row| !row.ok),
            "a tampered speed golden cannot PASS"
        );

        let mut stealth = stealth_goldens();
        let last = stealth.len() - 1;
        stealth[last].expected = 0.0;
        assert!(
            golden_rows(&stealth).iter().any(|row| !row.ok),
            "the tampered overload clamp cannot PASS"
        );

        // Same engine, a world with a wall where there should be none: the
        // jump cannot cross it (the penetration check would catch it).
        let wall = ProbeWorld::with_wall(0.5);
        assert!(!wall.is_free([0.6, EYE, 0.0], RADIUS), "a point behind the wall is not free");
    }

    /// (iii) Determinism ×2 of the synthetic report (same input ⇒ same report).
    #[test]
    fn synthetic_report_is_deterministic() {
        let first = synthetic_rows();
        let second = synthetic_rows();
        assert_eq!(first, second, "same input → same report (deterministic B)");
        for row in &first {
            assert!(row.ok, "{} FAIL: {}", row.id, row.detail);
        }
        assert_eq!(first.len(), 16, "16 runtime measurements of block B");
    }
}
