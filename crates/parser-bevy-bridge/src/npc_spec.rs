//! `npc_spec` — deterministic NPC population, 8-dir person flats, captions,
//! crime guards and quest NPC placement (M15,
//! REQ-L99-047 · DoD §f).
//!
//! **Canonical chain** (pinned executable spec, MIT v1.1.1 `0038a1e6…`):
//!
//! ```text
//! Person.SetResource → AssignRace (Person.cs:572-578) → AssignGender (:582-587)
//!   → AssignHUDFace (:589-598) → AssignDisplayName (:622-628) → AssignHomeTown
//! AssignDisplayName: DFRandom.srand(nameSeed) + NameHelper.FullName(bank, gender)
//! caption: GetLocalizedText(flatID.ToString(), TextFlats), fallback = race name (:377-382)
//! ```
//!
//! **Independent axes (spec §2.2 finding):** `race` comes from the
//! faction or from the subclimate (`DFLocation.ClimateSettings.People`,
//! `MapsFile.cs:449-519`); `nameBank` comes from the **region**
//! (`regionRaces`, one byte per region, `MapsFile.cs:66-75` exposed by
//! `FormulaHelper.cs:3104`). A Nord in Wrothgarian Mountains is named with the
//! Breton bank. [`Population::npcs`] exposes both axes separately.
//!
//! **Determinism (declared M15 decision):** DFU rolls gender and face with
//! `UnityEngine.Random` (not seedable) and the default `nameSeed` is
//! `DateTime.Now.Millisecond` (`Person.cs:591,623`). So that “same seed ⇒
//! same population” is verifiable (spec H6), this module derives
//! gender, `faceIndex`, flat and `nameSeed` from a [`DfRandom`] seeded with the
//! population seed; the name follows the exact DFU chain
//! (`srand(nameSeed)` + `FullName`). The deviation is in HH-1.
//!
//! **Flats (pack mandate §2.3 d):** `TEXTURE.256` (M12 keeps it) is the
//! billboard file; `flat_id = (archive << 7) | record`
//! (`DFU:API/FlatsFile.cs` `GetFlatID`). The geometry of the 8 directions is
//! **H2 `NEGATIVE`** (0 evidence in the corpus): here only the
//! direction 0..7 and the record's frame count are modeled; the render decides the frame.
//!
//! **Captions (Person.cs:377-382):** DFU resolves the text through
//! `TextManager.GetLocalizedText(flatID, TextCollections.TextFlats)`, which is
//! a DFU StringTable (`TextManager.cs:36,271` `Internal_Flats`), **not**
//! `TEXT.RSC`. Since the real `flatID` (`256 << 7 = 32768`) does not exist in
//! `TEXT.RSC` (max id 9999), the implemented chain is:
//! 1. `TEXT.RSC[flat_id]` if it existed (literal DFU path);
//! 2. `TEXT.RSC[occupation.text_id]` — retail ids verified against the file
//!    (`4442 priest`, `4446 assassin`, `4653 mage`, `4657 merchant`);
//! 3. race name (`Person.cs:382`), which is not `TEXT.RSC` text.
//!
//! Without `TEXT.RSC` there is no retail caption: the probe exits with code 2
//! before inventing text.
//!
//! **Guards (SpawnCityGuards.cs:17-20,45-56):** the
//! `spawncityguards (immediate)?` verb only returns a boolean to
//! `PlayerEntity.SpawnCityGuards(immediateSpawn)`; radius, guard count and
//! thresholds live outside the corpus (H4). [`spawn_city_guards`] fixes the
//! **contract** (threshold, quota, positions) as a declared M15 decision.
//!
//! **Place/CreateNpcAt (PlaceNpc.cs:76-105, CreateNpcAt.cs:28-52):**
//! [`create_npc_at`] reserves the site (SiteLink); [`place_npc`] rejects
//! nonexistent symbols, rejects `individual && atHome` and validates that the
//! coordinate falls inside the city.

use crate::city_spec::CityLayout;
use daggerfall_parser_rs::dfrandom::DfRandom;
use daggerfall_parser_rs::namegen::{BankType, Gender, NameGen, NameGenError};
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_parser_rs::texture::{RecordKind, TextureArchive};

/// Person billboard file required by the pack (§2.3 d).
pub const PERSON_FLAT_ARCHIVE: u16 = 256;
/// Billboard directions modeled (H2: the geometry is `NEGATIVE`).
pub const FLAT_DIRECTIONS: u8 = 8;
/// HUD portrait faces (`Person.cs:35` `faceCount = 10`).
pub const FACE_COUNT: u8 = 10;
/// NPCs per city population (M15 decision; no figure in the corpus).
pub const CITY_POPULATION: usize = 24;
/// Crime threshold to call the guards (H4: `PlayerEntity` absent
/// from the corpus ⇒ declared M15 decision).
pub const GUARD_CRIME_THRESHOLD: u8 = 2;
/// Guards per activation (H4: quota not specified in the corpus).
pub const GUARDS_PER_SPAWN: usize = 4;
/// Default `marker` of `place npc` (`PlaceNpc.cs:32-38`, default −1).
pub const PLACE_NPC_DEFAULT_MARKER: i32 = -1;

/// Race of a population (`FactionFile.FactionRaces` bounded to the values
/// reachable through `ClimateSettings.People`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Race {
    /// `People = Breton`.
    Breton,
    /// `People = Redguard`.
    Redguard,
    /// `People = Nord`.
    Nord,
}

impl Race {
    /// Retail race name (caption fallback, `Person.cs:382`).
    pub const fn name(self) -> &'static str {
        match self {
            Race::Breton => "Breton",
            Race::Redguard => "Redguard",
            Race::Nord => "Nord",
        }
    }
}

/// Region with its name bank (`regionNames` + `regionRaces`,
/// `MapsFile.cs:51-75`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionInfo {
    /// Index 0…61 (order of `regionNames`).
    pub index: u8,
    /// Region name.
    pub name: &'static str,
    /// Region name bank (`regionRaces ∈ {0,1}`).
    pub bank: BankType,
}

/// Closed table of the 62 regions (`MapsFile.cs:51-75`).
///
/// The names are reproduced from the **MIT Daggerfall Unity** corpus already
/// versioned in `reference/dfu-spec/` (see `NOTICE`, Daggerfall
/// Unity section): no new game byte is added (INV-L99-6).
pub const REGIONS: [RegionInfo; 62] = [
    RegionInfo { index: 0, name: "Alik'r Desert", bank: BankType::Redguard },
    RegionInfo { index: 1, name: "Dragontail Mountains", bank: BankType::Redguard },
    RegionInfo { index: 2, name: "Glenpoint Foothills", bank: BankType::Breton },
    RegionInfo { index: 3, name: "Daggerfall Bluffs", bank: BankType::Breton },
    RegionInfo { index: 4, name: "Yeorth Burrowland", bank: BankType::Breton },
    RegionInfo { index: 5, name: "Dwynnen", bank: BankType::Breton },
    RegionInfo { index: 6, name: "Ravennian Forest", bank: BankType::Breton },
    RegionInfo { index: 7, name: "Devilrock", bank: BankType::Redguard },
    RegionInfo { index: 8, name: "Malekna Forest", bank: BankType::Redguard },
    RegionInfo { index: 9, name: "Isle of Balfiera", bank: BankType::Breton },
    RegionInfo { index: 10, name: "Bantha", bank: BankType::Redguard },
    RegionInfo { index: 11, name: "Dak'fron", bank: BankType::Redguard },
    RegionInfo { index: 12, name: "Islands in the Western Iliac Bay", bank: BankType::Breton },
    RegionInfo { index: 13, name: "Tamarilyn Point", bank: BankType::Breton },
    RegionInfo { index: 14, name: "Lainlyn Cliffs", bank: BankType::Redguard },
    RegionInfo { index: 15, name: "Bjoulsae River", bank: BankType::Breton },
    RegionInfo { index: 16, name: "Wrothgarian Mountains", bank: BankType::Breton },
    RegionInfo { index: 17, name: "Daggerfall", bank: BankType::Breton },
    RegionInfo { index: 18, name: "Glenpoint", bank: BankType::Breton },
    RegionInfo { index: 19, name: "Betony", bank: BankType::Breton },
    RegionInfo { index: 20, name: "Sentinel", bank: BankType::Redguard },
    RegionInfo { index: 21, name: "Anticlere", bank: BankType::Breton },
    RegionInfo { index: 22, name: "Lainlyn", bank: BankType::Redguard },
    RegionInfo { index: 23, name: "Wayrest", bank: BankType::Breton },
    RegionInfo { index: 24, name: "Gen Tem High Rock village", bank: BankType::Breton },
    RegionInfo { index: 25, name: "Gen Rai Hammerfell village", bank: BankType::Redguard },
    RegionInfo { index: 26, name: "Orsinium Area", bank: BankType::Breton },
    RegionInfo { index: 27, name: "Skeffington Wood", bank: BankType::Breton },
    RegionInfo { index: 28, name: "Hammerfell bay coast", bank: BankType::Redguard },
    RegionInfo { index: 29, name: "Hammerfell sea coast", bank: BankType::Redguard },
    RegionInfo { index: 30, name: "High Rock bay coast", bank: BankType::Breton },
    RegionInfo { index: 31, name: "High Rock sea coast", bank: BankType::Breton },
    RegionInfo { index: 32, name: "Northmoor", bank: BankType::Breton },
    RegionInfo { index: 33, name: "Menevia", bank: BankType::Breton },
    RegionInfo { index: 34, name: "Alcaire", bank: BankType::Breton },
    RegionInfo { index: 35, name: "Koegria", bank: BankType::Breton },
    RegionInfo { index: 36, name: "Bhoriane", bank: BankType::Breton },
    RegionInfo { index: 37, name: "Kambria", bank: BankType::Breton },
    RegionInfo { index: 38, name: "Phrygias", bank: BankType::Breton },
    RegionInfo { index: 39, name: "Urvaius", bank: BankType::Breton },
    RegionInfo { index: 40, name: "Ykalon", bank: BankType::Breton },
    RegionInfo { index: 41, name: "Daenia", bank: BankType::Breton },
    RegionInfo { index: 42, name: "Shalgora", bank: BankType::Breton },
    RegionInfo { index: 43, name: "Abibon-Gora", bank: BankType::Redguard },
    RegionInfo { index: 44, name: "Kairou", bank: BankType::Redguard },
    RegionInfo { index: 45, name: "Pothago", bank: BankType::Redguard },
    RegionInfo { index: 46, name: "Myrkwasa", bank: BankType::Redguard },
    RegionInfo { index: 47, name: "Ayasofya", bank: BankType::Redguard },
    RegionInfo { index: 48, name: "Tigonus", bank: BankType::Redguard },
    RegionInfo { index: 49, name: "Kozanset", bank: BankType::Redguard },
    RegionInfo { index: 50, name: "Satakalaam", bank: BankType::Redguard },
    RegionInfo { index: 51, name: "Totambu", bank: BankType::Redguard },
    RegionInfo { index: 52, name: "Mournoth", bank: BankType::Redguard },
    RegionInfo { index: 53, name: "Ephesus", bank: BankType::Redguard },
    RegionInfo { index: 54, name: "Santaki", bank: BankType::Redguard },
    RegionInfo { index: 55, name: "Antiphyllos", bank: BankType::Redguard },
    RegionInfo { index: 56, name: "Bergama", bank: BankType::Redguard },
    RegionInfo { index: 57, name: "Gavaudon", bank: BankType::Breton },
    RegionInfo { index: 58, name: "Tulune", bank: BankType::Breton },
    RegionInfo { index: 59, name: "Glenumbra Moors", bank: BankType::Breton },
    RegionInfo { index: 60, name: "Ilessan Hills", bank: BankType::Breton },
    RegionInfo { index: 61, name: "Cybiades", bank: BankType::Redguard },
];

/// Region by index (0…61).
pub fn region(index: usize) -> Option<&'static RegionInfo> {
    REGIONS.get(index)
}

/// Race of the wandering subclimate NPCs (`ClimateSettings.People`,
/// `MapsFile.cs:449-519`; default = Breton).
pub fn subclimate_race(climate: u8) -> Race {
    match climate {
        223 | 228 | 229 | 230 | 231 | 232 => Race::Breton,
        224 | 225 | 227 => Race::Redguard,
        226 => Race::Nord,
        _ => Race::Breton,
    }
}

/// City occupation with its `TEXT.RSC` retail record.
///
/// The four ids exist in the retail build and their plain text is exactly
/// [`Occupation::text`] (verified against `TEXT.RSC` on 2026-10-20); the probe
/// re-verifies equality against the user's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occupation {
    /// Stable key.
    pub key: &'static str,
    /// `TEXT.RSC` `TextRecordId`.
    pub text_id: u16,
    /// Expected retail text of the record.
    pub text: &'static str,
}

/// Occupations of the city population (retail `TEXT.RSC` ids).
pub const OCCUPATIONS: [Occupation; 4] = [
    Occupation { key: "merchant", text_id: 4657, text: "merchant" },
    Occupation { key: "mage", text_id: 4653, text: "mage" },
    Occupation { key: "priest", text_id: 4442, text: "priest" },
    Occupation { key: "assassin", text_id: 4446, text: "assassin" },
];

/// Source of the caption text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionSource {
    /// `TEXT.RSC[flat_id]` (literal `Person.cs:380` path).
    TextRscFlat,
    /// `TEXT.RSC[occupation.text_id]` (verified retail id).
    TextRscOccupation,
    /// Race name (`Person.cs:382`) — there is no retail text.
    RaceFallback,
}

/// Resolved caption of an NPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caption {
    /// Source of the text.
    pub source: CaptionSource,
    /// `TextRecordId` used (if `TEXT.RSC` was available).
    pub text_id: Option<u16>,
    /// Displayable text.
    pub text: String,
}

/// Reference to a person billboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatRef {
    /// `TEXTURE.{archive:03}` file.
    pub archive: u16,
    /// Record index within the file.
    pub record: u16,
    /// Record frames (`TextureRecordMeta.FrameCount`).
    pub frames: u16,
    /// Billboard direction 0…7 (H2: no geometry in the corpus).
    pub direction: u8,
}

impl FlatRef {
    /// `FlatsFile.GetFlatID(archive, record)` = `(archive << 7) + record`.
    pub const fn flat_id(&self) -> u32 {
        ((self.archive as u32) << 7) + self.record as u32
    }
}

/// City NPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcSpec {
    /// Index in the population.
    pub index: usize,
    /// Region (0…61).
    pub region: u8,
    /// Subclimate (`CLIMATE.PAK`, 223…232).
    pub subclimate: u8,
    /// Race (subclimate/faction).
    pub race: Race,
    /// Name bank (region).
    pub name_bank: BankType,
    /// Gender.
    pub gender: Gender,
    /// Name seed (`Person.nameSeed`).
    pub name_seed: u32,
    /// `displayName` = `NameHelper.FullName(bank, gender)`.
    pub name: String,
    /// HUD portrait 0…9.
    pub face_index: u8,
    /// Person flat (8-dir).
    pub flat: FlatRef,
    /// Occupation (source of the retail caption).
    pub occupation: Occupation,
    /// Resolved caption.
    pub caption: Caption,
}

/// Population request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopulationRequest {
    /// Master population seed.
    pub seed: u32,
    /// Region index 0…61.
    pub region: u8,
    /// Subclimate 223…232.
    pub subclimate: u8,
    /// Number of NPCs.
    pub count: usize,
}

/// Deterministic population of a city.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Population {
    /// Request that generated it.
    pub request: PopulationRequest,
    /// NPCs in generation order.
    pub npcs: Vec<NpcSpec>,
}

impl Population {
    /// Canonical serialization (exact bits, fixed order) for the digest.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        fn push_str(out: &mut Vec<u8>, value: &str) {
            out.extend_from_slice(&(value.len() as u32).to_le_bytes());
            out.extend_from_slice(value.as_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(&self.request.seed.to_le_bytes());
        out.push(self.request.region);
        out.push(self.request.subclimate);
        out.extend_from_slice(&(self.npcs.len() as u32).to_le_bytes());
        for npc in &self.npcs {
            out.extend_from_slice(&(npc.index as u32).to_le_bytes());
            out.push(npc.subclimate);
            out.push(match npc.race {
                Race::Breton => 0,
                Race::Redguard => 1,
                Race::Nord => 2,
            });
            out.push(npc.name_bank.index() as u8);
            out.push(if npc.gender == Gender::Male { 0 } else { 1 });
            out.extend_from_slice(&npc.name_seed.to_le_bytes());
            push_str(&mut out, &npc.name);
            out.push(npc.face_index);
            out.extend_from_slice(&npc.flat.archive.to_le_bytes());
            out.extend_from_slice(&npc.flat.record.to_le_bytes());
            out.extend_from_slice(&npc.flat.frames.to_le_bytes());
            out.push(npc.flat.direction);
            push_str(&mut out, npc.occupation.key);
            push_str(&mut out, &npc.caption.text);
        }
        out
    }

    /// FNV-1a 64 of the canonical bytes (same seed ⇒ same integer).
    pub fn digest(&self) -> u64 {
        fnv1a64(&self.canonical_bytes())
    }
}

/// FNV-1a 64 (deterministic, 0 dependencies).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Typed NPC model error (0 panic).
#[derive(Debug, Clone, PartialEq)]
pub enum NpcError {
    /// Region outside `0…61`.
    RegionOutOfRange {
        /// Requested index.
        region: u8,
    },
    /// Empty population (no NPCs to certify).
    EmptyPopulation,
    /// `TEXTURE.256` could not be parsed.
    FlatArchive(String),
    /// The flats file has no records.
    FlatArchiveEmpty,
    /// A flat record does not resolve to an image.
    FlatNotResolvable {
        /// Requested record.
        record: u16,
    },
    /// The name could not be composed.
    Name(NameGenError),
    /// Quest symbol not present in the registry.
    UnknownNpcSymbol(String),
    /// Quest place not present in the registry.
    UnknownPlaceSymbol(String),
    /// `PlaceNpc` rejects individual NPCs that live in their house.
    IndividualAtHome(String),
    /// `marker` out of range.
    MarkerOutOfRange(i32),
    /// Coordinate outside the city.
    OutsideCity([f32; 2]),
    /// Invalid site symbol (`\w+` pattern).
    InvalidPlaceSymbol(String),
}

impl std::fmt::Display for NpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegionOutOfRange { region } => {
                write!(f, "region {region} outside 0…61")
            }
            Self::EmptyPopulation => write!(f, "empty population"),
            Self::FlatArchive(reason) => write!(f, "invalid flats file: {reason}"),
            Self::FlatArchiveEmpty => write!(f, "flats file without records"),
            Self::FlatNotResolvable { record } => {
                write!(f, "flat record {record} does not resolve to an image")
            }
            Self::Name(err) => write!(f, "name not composable: {err}"),
            Self::UnknownNpcSymbol(symbol) => write!(f, "unknown NPC symbol: {symbol}"),
            Self::UnknownPlaceSymbol(symbol) => write!(f, "unknown place symbol: {symbol}"),
            Self::IndividualAtHome(symbol) => {
                write!(f, "PlaceNpc rejects the individual NPC at home: {symbol}")
            }
            Self::MarkerOutOfRange(marker) => write!(f, "marker out of range: {marker}"),
            Self::OutsideCity(position) => {
                write!(f, "coordinate outside the city: {position:?}")
            }
            Self::InvalidPlaceSymbol(symbol) => {
                write!(f, "invalid site symbol (\\w+): {symbol}")
            }
        }
    }
}

impl std::error::Error for NpcError {}

impl From<NameGenError> for NpcError {
    fn from(err: NameGenError) -> Self {
        NpcError::Name(err)
    }
}

/// Retail data context for populating (all borrowed, 0 asset copies).
pub struct NpcWorld<'a> {
    /// Name bank (`NAMEGEN.DAT`).
    pub namegen: &'a NameGen,
    /// Person billboards (`TEXTURE.256`).
    pub person_flats: &'a TextureArchive,
    /// `TEXT.RSC` table.
    pub text: &'a TextRsc,
    /// `TEXT.RSC` bytes (the records live here).
    pub text_bytes: &'a [u8],
}

impl NpcWorld<'_> {
    /// Generates the population of a city. Same `request` ⇒ same digest.
    pub fn populate(&self, request: PopulationRequest) -> Result<Population, NpcError> {
        let region = *REGIONS
            .get(request.region as usize)
            .ok_or(NpcError::RegionOutOfRange {
                region: request.region,
            })?;
        if request.count == 0 {
            return Err(NpcError::EmptyPopulation);
        }
        let flat_count = self.person_flats.records.len();
        if flat_count == 0 {
            return Err(NpcError::FlatArchiveEmpty);
        }
        let race = subclimate_race(request.subclimate);

        let mut rng = DfRandom::new();
        rng.srand(request.seed);
        let mut npcs = Vec::with_capacity(request.count);
        for index in 0..request.count {
            // Fixed draw order (verifiable determinism); the name does NOT
            // consume this stream: it uses its own `srand(name_seed)`.
            let gender = Gender::draw(&mut rng);
            let face_index = (rng.rand() % u32::from(FACE_COUNT)) as u8;
            let record = (rng.rand() % flat_count as u32) as u16;
            let direction = (rng.rand() % u32::from(FLAT_DIRECTIONS)) as u8;
            let name_seed = rng.rand() | (rng.rand() << 15);
            let occupation = OCCUPATIONS[(rng.rand() % OCCUPATIONS.len() as u32) as usize];

            let flat = self.resolve_flat(record, direction)?;
            let name = self.namegen.full_name(region.bank, gender, name_seed)?;
            let caption = self.caption(flat.flat_id(), occupation, race);

            npcs.push(NpcSpec {
                index,
                region: request.region,
                subclimate: request.subclimate,
                race,
                name_bank: region.bank,
                gender,
                name_seed,
                name,
                face_index,
                flat,
                occupation,
                caption,
            });
        }
        Ok(Population { request, npcs })
    }

    /// Resolves a `TEXTURE.256` record to a [`FlatRef`] (0 NPC without flat).
    pub fn resolve_flat(&self, record: u16, direction: u8) -> Result<FlatRef, NpcError> {
        if direction >= FLAT_DIRECTIONS {
            return Err(NpcError::FlatNotResolvable { record });
        }
        let header = self
            .person_flats
            .records
            .get(record as usize)
            .ok_or(NpcError::FlatNotResolvable { record })?;
        let meta = match header.kind {
            RecordKind::Image(meta) => meta,
            RecordKind::Solid { .. } | RecordKind::NoImage => {
                return Err(NpcError::FlatNotResolvable { record })
            }
        };
        Ok(FlatRef {
            archive: PERSON_FLAT_ARCHIVE,
            record,
            frames: meta.frame_count,
            direction,
        })
    }

    /// Caption chain documented in the module header.
    pub fn caption(&self, flat_id: u32, occupation: Occupation, race: Race) -> Caption {
        if let Ok(id) = u16::try_from(flat_id) {
            if let Some(text) = self.text.record_ascii(self.text_bytes, id) {
                return Caption {
                    source: CaptionSource::TextRscFlat,
                    text_id: Some(id),
                    text: text.to_string(),
                };
            }
        }
        if let Some(text) = self.text.record_ascii(self.text_bytes, occupation.text_id) {
            return Caption {
                source: CaptionSource::TextRscOccupation,
                text_id: Some(occupation.text_id),
                text: text.to_string(),
            };
        }
        Caption {
            source: CaptionSource::RaceFallback,
            text_id: None,
            text: race.name().to_string(),
        }
    }
}

/// City guard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuardSpec {
    /// Index within the group.
    pub index: usize,
    /// Guard flat (same person file; H4 no armor/equipment).
    pub flat: FlatRef,
    /// Position in meters (city frame; y = 0 on the street).
    pub position: [f32; 3],
}

/// `spawncityguards [immediate]` (`SpawnCityGuards.cs:17-20,45-56`).
///
/// With `crime < GUARD_CRIME_THRESHOLD` it returns **0** guards (binary
/// evaluation of criterion d). Positions are deterministic: street
/// crossings of the synthetic layout (`CityLayout`), with no invented radius (H4).
pub fn spawn_city_guards(
    world: &NpcWorld<'_>,
    city: &CityLayout,
    crime: u8,
    immediate: bool,
) -> Result<Vec<GuardSpec>, NpcError> {
    if crime < GUARD_CRIME_THRESHOLD {
        return Ok(Vec::new());
    }
    // `immediate` does not change the quota: DFU uses it to skip the warning
    // (`SpawnCityGuards.cs:45`), not for the number of guards.
    let _ = immediate;
    let flat_count = world.person_flats.records.len();
    if flat_count == 0 {
        return Err(NpcError::FlatArchiveEmpty);
    }
    let mut guards = Vec::with_capacity(GUARDS_PER_SPAWN);
    for index in 0..GUARDS_PER_SPAWN {
        let record = ((index * 3) % flat_count) as u16;
        let flat = world.resolve_flat(record, (index as u8) % FLAT_DIRECTIONS)?;
        let gap = index % city.blocks;
        let position = [city.gap_center(0), 0.0, city.gap_center(gap)];
        guards.push(GuardSpec {
            index,
            flat,
            position,
        });
    }
    Ok(guards)
}

/// Quest NPC (`Person` side of `PlaceNpc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestPerson {
    /// Resource symbol.
    pub symbol: String,
    /// `Person.IsIndividualNPC`.
    pub is_individual_npc: bool,
    /// `Person.IsIndividualAtHome`.
    pub is_individual_at_home: bool,
}

/// Quest place with its coordinate already resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// Resource symbol.
    pub symbol: String,
    /// Placement coordinate (meters).
    pub position: [f32; 3],
}

/// Minimal quest registry (what `PlaceNpc` consults).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestRegistry {
    /// Registered NPCs.
    pub persons: Vec<QuestPerson>,
    /// Registered places.
    pub places: Vec<Place>,
}

impl QuestRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an NPC.
    pub fn add_person(&mut self, person: QuestPerson) {
        self.persons.push(person);
    }

    /// Adds a place.
    pub fn add_place(&mut self, place: Place) {
        self.places.push(place);
    }

    /// NPC by symbol.
    pub fn person(&self, symbol: &str) -> Option<&QuestPerson> {
        self.persons.iter().find(|person| person.symbol == symbol)
    }

    /// Place by symbol.
    pub fn place(&self, symbol: &str) -> Option<&Place> {
        self.places.iter().find(|place| place.symbol == symbol)
    }
}

/// `QuestMachine` `SiteLink` created by `create npc at` (CreateNpcAt.cs:28-52).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteLink {
    /// Ordinal position of the reserved site (quest book-keeping).
    pub site_index: usize,
}

/// `create npc at <aPlace>`: reserves the site (`\w+` pattern,
/// `CreateNpcAt.cs:28-52`).
pub fn create_npc_at(place_symbol: &str, existing_sites: usize) -> Result<SiteLink, NpcError> {
    let valid = !place_symbol.is_empty()
        && place_symbol
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if !valid {
        return Err(NpcError::InvalidPlaceSymbol(place_symbol.to_string()));
    }
    Ok(SiteLink {
        site_index: existing_sites,
    })
}

/// Result of `place npc <anNPC> at <aPlace> [marker <n>]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// NPC symbol.
    pub npc_symbol: String,
    /// Place symbol.
    pub place_symbol: String,
    /// Marker applied (`-1` if not declared).
    pub marker: i32,
    /// Final coordinate (meters, inside the city).
    pub position: [f32; 3],
    /// `Person.IsHidden` stays `false` after placing (`PlaceNpc.cs:66-116`).
    pub is_hidden: bool,
}

/// `place npc <anNPC> at <aPlace> [marker <n>]` (`PlaceNpc.cs:76-105`).
pub fn place_npc(
    registry: &QuestRegistry,
    npc_symbol: &str,
    place_symbol: &str,
    marker: i32,
    city: &CityLayout,
) -> Result<Placement, NpcError> {
    let person = registry
        .person(npc_symbol)
        .ok_or_else(|| NpcError::UnknownNpcSymbol(npc_symbol.to_string()))?;
    let place = registry
        .place(place_symbol)
        .ok_or_else(|| NpcError::UnknownPlaceSymbol(place_symbol.to_string()))?;
    if person.is_individual_npc && person.is_individual_at_home {
        return Err(NpcError::IndividualAtHome(npc_symbol.to_string()));
    }
    if marker < PLACE_NPC_DEFAULT_MARKER {
        return Err(NpcError::MarkerOutOfRange(marker));
    }
    let [x, _, z] = place.position;
    let total = city.total_m();
    if !(0.0..=total).contains(&x) || !(0.0..=total).contains(&z) {
        return Err(NpcError::OutsideCity([x, z]));
    }
    Ok(Placement {
        npc_symbol: npc_symbol.to_string(),
        place_symbol: place_symbol.to_string(),
        marker,
        position: place.position,
        is_hidden: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::texture::TextureArchive;

    fn namegen() -> NameGen {
        // Minimal bank: 6 sets per bank with 2 parts each.
        let sets: [&[&str]; 6] = [
            &["Agr", "Bed"],
            &["ore", "win"],
            &["Chry", "Theo"],
            &["stanna", "dore"],
            &["Wick", "Moor"],
            &["ing", "wing"],
        ];
        let empty: [&[&str]; 0] = [];
        let banks: [&[&[&str]]; 11] = [
            &sets, &sets, &empty, &empty, &empty, &empty, &empty, &empty, &empty, &empty, &empty,
        ];
        // Reuses the synthetic `namegen` fixture through its parser.
        let mut header = vec![0u8; 528];
        let mut body = Vec::new();
        let mut cursor = 528usize;
        for (bank, sets) in banks.iter().enumerate() {
            if sets.is_empty() {
                continue;
            }
            for (set, parts) in sets.iter().enumerate() {
                let record = bank * 48;
                header[record + set * 4..record + set * 4 + 4]
                    .copy_from_slice(&(cursor as u32).to_le_bytes());
                header[record + 24 + set * 4..record + 24 + set * 4 + 4]
                    .copy_from_slice(&(parts.len() as u32).to_le_bytes());
                for part in parts.iter() {
                    let mut slot = [0u8; 10];
                    slot[..part.len()].copy_from_slice(part.as_bytes());
                    body.extend_from_slice(&slot);
                    cursor += 10;
                }
            }
        }
        header.extend_from_slice(&body);
        NameGen::parse(&header).expect("fixture NAMEGEN")
    }

    /// Synthetic `TEXTURE.256`: one image record of 4 frames.
    fn flats() -> (Vec<u8>, TextureArchive) {
        let mut file = Vec::new();
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(b"people\0");
        file.resize(26, 0);
        let record_at: i32 = 26 + 20;
        file.extend_from_slice(&0u16.to_le_bytes());
        file.extend_from_slice(&record_at.to_le_bytes());
        file.extend_from_slice(&0u16.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0i64.to_le_bytes());
        // TextureRecord 28 B: 10×10 px, no compression, 4 frames.
        file.extend_from_slice(&0i16.to_le_bytes()); // offset_x
        file.extend_from_slice(&0i16.to_le_bytes()); // offset_y
        file.extend_from_slice(&10i16.to_le_bytes()); // width
        file.extend_from_slice(&10i16.to_le_bytes()); // height
        file.extend_from_slice(&0u16.to_le_bytes()); // compression
        file.extend_from_slice(&28u32.to_le_bytes()); // record size
        file.extend_from_slice(&28u32.to_le_bytes()); // data offset
        file.extend_from_slice(&1u16.to_le_bytes()); // is_normal
        file.extend_from_slice(&4u16.to_le_bytes()); // frame count
        file.extend_from_slice(&0u16.to_le_bytes()); // unknown1
        file.extend_from_slice(&256i16.to_le_bytes()); // x_scale
        file.extend_from_slice(&256i16.to_le_bytes()); // y_scale
        let archive = TextureArchive::parse(&file).expect("fixture TEXTURE");
        (file, archive)
    }

    /// Synthetic `TEXT.RSC` with two real occupations.
    fn text_rsc() -> (Vec<u8>, TextRsc) {
        let records: [(u16, &str); 4] = [
            (4442, "priest"),
            (4446, "assassin"),
            (4653, "mage"),
            (4657, "merchant"),
        ];
        let header_length = ((records.len() + 1) * 6) as u16;
        let mut file = Vec::new();
        file.extend_from_slice(&header_length.to_le_bytes());
        let mut cursor = 2 + records.len() * 6;
        for (id, text) in records {
            file.extend_from_slice(&id.to_le_bytes());
            file.extend_from_slice(&(cursor as u32).to_le_bytes());
            cursor += text.len() + 1;
        }
        for (_, text) in records {
            file.extend_from_slice(text.as_bytes());
            file.push(0xFE);
        }
        let rsc = TextRsc::parse(&file).expect("fixture RSC");
        (file, rsc)
    }

    struct Fixture {
        namegen: NameGen,
        _flat_bytes: Vec<u8>,
        flats: TextureArchive,
        text_bytes: Vec<u8>,
        text: TextRsc,
    }

    impl Fixture {
        fn new() -> Self {
            let (flat_bytes, flats) = flats();
            let (text_bytes, text) = text_rsc();
            Self {
                namegen: namegen(),
                _flat_bytes: flat_bytes,
                flats,
                text_bytes,
                text,
            }
        }

        fn world(&self) -> NpcWorld<'_> {
            NpcWorld {
                namegen: &self.namegen,
                person_flats: &self.flats,
                text: &self.text,
                text_bytes: &self.text_bytes,
            }
        }
    }

    fn request(seed: u32) -> PopulationRequest {
        PopulationRequest {
            seed,
            region: 17,
            subclimate: 231,
            count: 8,
        }
    }

    /// Region/subclimate: two independent axes (Nord + Breton bank).
    #[test]
    fn race_and_bank_are_independent_axes() {
        let wrothgarian = region(16).unwrap();
        assert_eq!(wrothgarian.name, "Wrothgarian Mountains");
        assert_eq!(wrothgarian.bank, BankType::Breton);
        assert_eq!(subclimate_race(226), Race::Nord);
        assert_eq!(subclimate_race(224), Race::Redguard);
        assert_eq!(subclimate_race(231), Race::Breton);
        assert_eq!(subclimate_race(0), Race::Breton);
        assert_eq!(REGIONS.len(), 62);
        assert!(region(62).is_none());
    }

    /// Same seed ×2 ⇒ identical digest; different seed ⇒ different digest.
    #[test]
    fn population_digest_is_stable_per_seed() {
        let fixture = Fixture::new();
        let world = fixture.world();
        let a = world.populate(request(20261020)).unwrap();
        let b = world.populate(request(20261020)).unwrap();
        assert_eq!(a.digest(), b.digest());
        assert_eq!(a, b);
        let other = world.populate(request(7)).unwrap();
        assert_ne!(a.digest(), other.digest());
    }

    /// 0 NPC without a resolvable flat + non-empty name + `TEXT.RSC` caption.
    #[test]
    fn every_npc_has_flat_name_and_retail_caption() {
        let fixture = Fixture::new();
        let world = fixture.world();
        let population = world.populate(request(1)).unwrap();
        assert_eq!(population.npcs.len(), 8);
        for npc in &population.npcs {
            assert!(npc.flat.record < fixture.flats.records.len() as u16);
            assert!(npc.flat.direction < FLAT_DIRECTIONS);
            assert!(!npc.name.is_empty());
            assert_eq!(npc.caption.source, CaptionSource::TextRscOccupation);
            assert_eq!(
                npc.caption.text,
                OCCUPATIONS
                    .iter()
                    .find(|occupation| occupation.text_id == npc.caption.text_id.unwrap())
                    .unwrap()
                    .text
            );
        }
    }

    /// Without `TEXT.RSC` records the caption degrades to the race name (never
    /// invented text).
    #[test]
    fn missing_text_rsc_falls_back_to_race_name() {
        let fixture = Fixture::new();
        let empty_rsc = TextRsc {
            header_length: 6,
            records: Vec::new(),
        };
        let world = NpcWorld {
            namegen: &fixture.namegen,
            person_flats: &fixture.flats,
            text: &empty_rsc,
            text_bytes: &fixture.text_bytes,
        };
        let caption = world.caption(32768, OCCUPATIONS[0], Race::Nord);
        assert_eq!(caption.source, CaptionSource::RaceFallback);
        assert_eq!(caption.text, "Nord");
        assert!(caption.text_id.is_none());
    }

    /// Guards: 0 below the threshold, full quota at/above it.
    #[test]
    fn guards_spawn_only_at_or_above_threshold() {
        let fixture = Fixture::new();
        let world = fixture.world();
        let city = CityLayout::new(3);
        assert!(spawn_city_guards(&world, &city, 0, true).unwrap().is_empty());
        assert!(spawn_city_guards(&world, &city, 1, false)
            .unwrap()
            .is_empty());
        let guards = spawn_city_guards(&world, &city, GUARD_CRIME_THRESHOLD, true).unwrap();
        assert_eq!(guards.len(), GUARDS_PER_SPAWN);
        for guard in &guards {
            assert!(guard.position[0] >= 0.0 && guard.position[0] <= city.total_m());
            assert!(guard.position[2] >= 0.0 && guard.position[2] <= city.total_m());
        }
    }

    /// `create npc at` reserves a site with the `\w+` pattern; `place npc` validates
    /// symbols, rejects individual-at-home and coordinates outside the city.
    #[test]
    fn place_npc_follows_placenpc_contract() {
        let city = CityLayout::new(3);
        assert_eq!(create_npc_at("aPlace", 0), Ok(SiteLink { site_index: 0 }));
        assert!(create_npc_at("bad symbol", 0).is_err());
        assert!(create_npc_at("", 0).is_err());

        let mut registry = QuestRegistry::new();
        registry.add_person(QuestPerson {
            symbol: "anNPC".into(),
            is_individual_npc: false,
            is_individual_at_home: false,
        });
        registry.add_person(QuestPerson {
            symbol: "homeNPC".into(),
            is_individual_npc: true,
            is_individual_at_home: true,
        });
        registry.add_place(Place {
            symbol: "aPlace".into(),
            position: [10.0, 0.0, 10.0],
        });
        registry.add_place(Place {
            symbol: "farPlace".into(),
            position: [10_000.0, 0.0, 10.0],
        });

        let placed = place_npc(
            &registry,
            "anNPC",
            "aPlace",
            PLACE_NPC_DEFAULT_MARKER,
            &city,
        )
        .unwrap();
        assert!(!placed.is_hidden);
        assert_eq!(placed.marker, -1);

        assert_eq!(
            place_npc(&registry, "nope", "aPlace", -1, &city),
            Err(NpcError::UnknownNpcSymbol("nope".into()))
        );
        assert_eq!(
            place_npc(&registry, "anNPC", "nope", -1, &city),
            Err(NpcError::UnknownPlaceSymbol("nope".into()))
        );
        assert_eq!(
            place_npc(&registry, "homeNPC", "aPlace", -1, &city),
            Err(NpcError::IndividualAtHome("homeNPC".into()))
        );
        assert!(matches!(
            place_npc(&registry, "anNPC", "farPlace", -1, &city),
            Err(NpcError::OutsideCity(_))
        ));
    }

    /// A flat record outside the file does not resolve (0 panic).
    #[test]
    fn unresolvable_flat_is_an_error() {
        let fixture = Fixture::new();
        let world = fixture.world();
        assert_eq!(
            world.resolve_flat(99, 0),
            Err(NpcError::FlatNotResolvable { record: 99 })
        );
        assert_eq!(
            world.resolve_flat(0, FLAT_DIRECTIONS),
            Err(NpcError::FlatNotResolvable { record: 0 })
        );
    }
}
