//! Original sound effects — **`DAGGER.SND`** (REQ-L99-046, M14).
//! *Clean-room* implementation from spec, 0 bytes of Daggerfall
//! Unity copied, 0 packed assets (INV-L99-6).
//!
//! **Spec source:**
//! - UESP *Daggerfall Mod:BSA file formats*: `DAGGER.SND` is Daggerfall's only
//!   WAVE audio file, in **BSA type 0x0200** format (directory
//!   with numeric IDs) — we reuse [`crate::bsa`] (already validated against
//!   `ARCH3D.BSA`/`BLOCKS.BSA` in M6).
//! - DFWorkshop *Sounds & Movies*: each clip is **raw 8-bit unsigned mono
//!   PCM audio at ~11025 Hz** (no embedded RIFF/WAVE container; the
//!   WAVE header is synthesized on playback).
//! - *dispatch*: `SoundClips` is played **by positional directory index**, not
//!   by `RecordID`. DFU `GetAudioClip(SoundClips soundClip)` forwards
//!   `(int)soundClip` to `GetAudioClip(int soundIndex)` and then to
//!   `SndFile.GetSound(soundIndex)`, which reads `bsaFile.GetRecordProxy(sound)`
//!   by position (`DFU:Assets/Scripts/SoundReader.cs:126-132`, `:59-101`;
//!   `DFU:Assets/Scripts/API/SndFile.cs:158-176`). `SoundClips.cs:30`
//!   (`ArrowShoot = 3`) and `:137` (`FallDamage = 91`) are therefore
//!   **directory indices**. The entry's own `RecordID` is metadata used only
//!   for display/replacement (`SoundReader.cs:74-75`); the retail directory is
//!   **not** sorted by `RecordID` (index 3 carries `RecordID` 6, index 91
//!   carries 359), so dispatching by id would return a different clip.
//!
//! **Finding verified today (ARENA2 read-only):** the actual file present on
//! this machine is **`DAGGER.SND`** (`fbb6195b…86f3e2`, 7,661,766 B), **not**
//! `SOUND.BSA` (absent, negative finding R-D2). `DAGGER.SND` parses as BSA
//! 0x0200 with 459 records: raw packed data from offset 4 + directory
//! `(RecordID, DataSize)` at the end; the invariant `4 + ΣDataSize == directory_offset`
//! holds exactly (7,658,094). Each record is 8-bit unsigned mono PCM.
//!
//! Honest degradation: if the file is missing from the user's ARENA2 (Mode A /
//! PathChecker), no audio is fabricated — [`SndError::Missing`] is reported.

use crate::bsa;

/// Default sample rate of Daggerfall PCM (DFWorkshop Sounds & Movies).
pub const SAMPLE_RATE: u32 = 11025;
/// Channels (mono).
pub const CHANNELS: u16 = 1;
/// Bits per sample (8-bit unsigned).
pub const BITS_PER_SAMPLE: u16 = 8;

/// Errors of the effects pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SndError {
    /// The file/underlying BSA does not parse.
    BadContainer(bsa::BsaError),
    /// The directory is not 0x0200 (numeric IDs).
    NotNumbered,
    /// No directory entry at the requested positional index (`index >= len`) —
    /// the same rejection as DFU `SndFile.IsValidIndex`
    /// (`DFU:Assets/Scripts/API/SndFile.cs:237-243`: `sound < 0 || sound >= bsaFile.Count`).
    IndexOutOfRange { index: usize, count: usize },
    /// No directory entry carries the requested `RecordID` — the same
    /// "not found" as DFU `SndFile.GetRecordIndex` returning `-1`
    /// (`DFU:Assets/Scripts/API/SndFile.cs:117-127`).
    MissingRecordId { record_id: u32, count: usize },
    /// The record exists but is empty (no PCM).
    EmptyClip { index: usize },
}

impl std::fmt::Display for SndError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadContainer(e) => write!(f, "invalid DAGGER.SND container: {e}"),
            Self::NotNumbered => write!(f, "directory is not 0x0200 (numeric)"),
            Self::IndexOutOfRange { index, count } => {
                write!(f, "sound index {index} out of range (file has {count} records)")
            }
            Self::MissingRecordId { record_id, count } => {
                write!(f, "RecordID {record_id} not found in the file ({count} records)")
            }
            Self::EmptyClip { index } => {
                write!(f, "sound index {index} is empty (0 PCM bytes)")
            }
        }
    }
}

impl std::error::Error for SndError {}

impl From<bsa::BsaError> for SndError {
    fn from(e: bsa::BsaError) -> Self {
        Self::BadContainer(e)
    }
}

/// A decoded clip: raw 8-bit unsigned mono PCM (no header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundClip {
    /// Positional index in the BSA 0x0200 directory — the key DFU dispatches
    /// `SoundClips` by (`SoundReader.cs:59-101` → `SndFile.cs:158`).
    pub index: usize,
    /// Directory `RecordID` of the entry at [`Self::index`] — metadata only
    /// (clip naming / `SoundReplacement`, `SoundReader.cs:66,71`); it is **not**
    /// the `SoundClips` discriminant and the retail directory is not sorted by it.
    pub record_id: u32,
    /// Number of samples (= PCM bytes, 8-bit mono).
    pub samples: usize,
    /// Raw 8-bit unsigned PCM (record payload).
    pub pcm: Vec<u8>,
}

impl SoundClip {
    /// Synthesizes a 44-byte RIFF/WAVE header for this clip
    /// (8-bit unsigned mono PCM at [`SAMPLE_RATE`] Hz) — the same thing DFU
    /// does when materializing an `AudioClip` from the raw PCM.
    pub fn wave_header(&self) -> Vec<u8> {
        let data_len = self.pcm.len() as u32;
        let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8);
        let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
        let mut h = Vec::with_capacity(44 + self.pcm.len());
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(36 + data_len).to_le_bytes());
        h.extend_from_slice(b"WAVE");
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes()); // PCM
        h.extend_from_slice(&CHANNELS.to_le_bytes());
        h.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        h.extend_from_slice(&byte_rate.to_le_bytes());
        h.extend_from_slice(&block_align.to_le_bytes());
        h.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data_len.to_le_bytes());
        h.extend_from_slice(&self.pcm);
        h
    }
}

/// Effect bank: `DAGGER.SND` parsed as BSA 0x0200.
#[derive(Debug)]
pub struct SoundBank {
    archive: bsa::BsaArchive,
    /// `record_id` -> directory index (first match, like DFU
    /// `SndFile.GetRecordIndex`, `SndFile.cs:117-127`). Used **only** by the
    /// explicit id lookups — never by index dispatch.
    by_id: std::collections::HashMap<u32, usize>,
}

impl SoundBank {
    /// Parses the bank from the complete `DAGGER.SND` buffer (read-only).
    pub fn parse(data: &[u8]) -> Result<Self, SndError> {
        let archive = bsa::parse(data)?;
        if archive.directory_type != bsa::DirectoryType::Number8 {
            return Err(SndError::NotNumbered);
        }
        let mut by_id = std::collections::HashMap::with_capacity(archive.entries.len());
        for (idx, entry) in archive.entries.iter().enumerate() {
            if let bsa::Entry::Numbered { record_id, .. } = entry {
                // First occurrence wins, mirroring `SndFile.GetRecordIndex`
                // (`SndFile.cs:117-127`, returns the first index with that id).
                by_id.entry(*record_id).or_insert(idx);
            }
        }
        Ok(Self { archive, by_id })
    }

    /// Number of records in the bank.
    pub fn len(&self) -> usize {
        self.archive.entries.len()
    }

    /// `true` if there are no records.
    pub fn is_empty(&self) -> bool {
        self.archive.entries.is_empty()
    }

    /// Decodes the clip at positional directory `index` — the DFU dispatch
    /// path (`SoundReader.cs:59-101` → `SndFile.GetSound(index)`,
    /// `SndFile.cs:158-176`, which reads `bsaFile.GetRecordProxy(sound)` by
    /// position). `Err(IndexOutOfRange)` when `index >= len`, matching DFU
    /// `SndFile.IsValidIndex` (`SndFile.cs:237-243`); it never falls back to a
    /// `RecordID` lookup. The entry's `record_id` is preserved on the clip as
    /// metadata only.
    pub fn decode(&self, data: &[u8], index: usize) -> Result<SoundClip, SndError> {
        if index >= self.archive.entries.len() {
            return Err(SndError::IndexOutOfRange {
                index,
                count: self.archive.entries.len(),
            });
        }
        let record_id = match &self.archive.entries[index] {
            bsa::Entry::Numbered { record_id, .. } => *record_id,
            // Unreachable after `parse` enforces Number8, but kept honest
            // rather than assuming the directory shape.
            bsa::Entry::Named { .. } => return Err(SndError::NotNumbered),
        };
        let pcm = bsa::entry_data(&self.archive, data, index)?.to_vec();
        if pcm.is_empty() {
            return Err(SndError::EmptyClip { index });
        }
        let samples = pcm.len();
        Ok(SoundClip { index, record_id, samples, pcm })
    }

    /// Decodes by `RecordID` instead of directory index. **Not** the DFU
    /// dispatch path — `SoundClips` is positional (`SoundReader.cs:59-101`);
    /// DFU only converts an id to a position when its caller genuinely holds
    /// an id (`GetAudioClip(uint soundID)` → `GetRecordIndex`,
    /// `SoundReader.cs:113-119`). Provided under this explicit name so no
    /// caller can confuse it with [`Self::decode`], and used by the probe as
    /// the negative control that shows id dispatch selects a different entry.
    pub fn decode_by_record_id(&self, data: &[u8], record_id: u32) -> Result<SoundClip, SndError> {
        let index = self
            .index_of_record_id(record_id)
            .ok_or(SndError::MissingRecordId { record_id, count: self.len() })?;
        self.decode(data, index)
    }

    /// Directory index of the first entry whose `RecordID == record_id`
    /// (DFU `SndFile.GetRecordIndex`, `SndFile.cs:117-127`); `None` where DFU
    /// would return `-1`.
    pub fn index_of_record_id(&self, record_id: u32) -> Option<usize> {
        self.by_id.get(&record_id).copied()
    }

    /// `record_id` of the record at position `index` (0-based, directory
    /// order) — the metadata DFU exposes as `GetSoundID`/`GetRecordId`
    /// (`SoundReader.cs:139-146`, `BsaFile.cs:254`).
    pub fn record_id_at(&self, index: usize) -> Option<u32> {
        match self.archive.entries.get(index) {
            Some(bsa::Entry::Numbered { record_id, .. }) => Some(*record_id),
            _ => None,
        }
    }
}

/// DFU `SoundClips` catalogue (`reference/dfu-spec/Assets/Scripts/SoundClips.cs`,
/// MIT, pinned): **375 members / 374 distinct values**. Each discriminant is a
/// **positional directory index** into `DAGGER.SND`, never a `RecordID`: DFU
/// forwards `(int)soundClip` to `GetAudioClip(int soundIndex)` and then to
/// `SndFile.GetSound(soundIndex)` = `bsaFile.GetRecordProxy(sound)` by position
/// (`SoundReader.cs:126-132`, `:59-101`; `SndFile.cs:158-176`). The entry's own
/// `RecordID` is metadata used only for display/replacement
/// (`SoundReader.cs:74-75`).
///
/// **375 DFU members, 374 variants:** `EnemyOrcAttack` and
/// `EnemyOrcShamanAttack` share the discriminant 138 and Rust rejects two
/// variants with the same value, so `EnemyOrcAttack` carries it and the second
/// name lives only in [`SOUND_CLIP_CATALOG`] flagged `alias: true`.
///
/// References: `DFU:SoundClips.cs:30` (`ArrowShoot`), `:137` (`FallDamage`);
/// usage `DFU:EnemyAttack.cs:110`, `DFU:EnemyMotor.cs:1409`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SoundClips {
    None = -1,
    SpookyHigh = 0,
    SpookyMid = 1,
    SpookyLow = 2,
    ArrowShoot = 3,
    ArrowHit = 4,
    Invalid = 5,
    AmbientCrickets = 6,
    PlayerDoorBash = 7,
    ArenaFireDaemon = 11,
    AmbientCreepyBirdLaughs = 13,
    AmbientCreepyBirdCall = 14,
    BodyFall = 15,
    Ignite = 16,
    ActivateGears = 19,
    ActivateDoorClose = 20,
    ArenaFootstep1 = 22,
    ArenaFootstep2 = 23,
    DungeonDoorClose = 24,
    DungeonDoorOpen = 25,
    AmbientDripShortHigh = 26,
    AmbientDripLongHigh = 27,
    AmbientDrums = 28,
    AmbientWindMoan2 = 29,
    ArenaHitSound = 30,
    ArenaFanfareLevelUp = 32,
    ArenaFanfareStaffOfChaos = 33,
    ArenaFireDaemon2 = 36,
    ArenaGhost = 38,
    ArenaGhoul = 39,
    ActivateGrind = 40,
    ArenaHellHound = 41,
    ArgonianMalePain3 = 42,
    BretonFemalePain1 = 43,
    BretonFemalePain2 = 44,
    BretonFemalePain3 = 45,
    RedguardFemalePain1 = 46,
    RedguardFemalePain2 = 47,
    RedguardFemalePain3 = 48,
    NordFemalePain1 = 49,
    NordFemalePain2 = 50,
    NordFemalePain3 = 51,
    DarkElfFemalePain1 = 52,
    DarkElfFemalePain2 = 53,
    DarkElfFemalePain3 = 54,
    HighElfFemalePain1 = 55,
    HighElfFemalePain2 = 56,
    HighElfFemalePain3 = 57,
    WoodElfFemalePain1 = 58,
    WoodElfFemalePain2 = 59,
    WoodElfFemalePain3 = 60,
    KhajiitFemalePain1 = 61,
    KhajiitFemalePain2 = 62,
    AmbientDripShort = 63,
    AmbientDripLong = 64,
    AmbientWindMoan = 65,
    AmbientWindMoanDeep = 66,
    AmbientDoorOpen = 67,
    AmbientGrind = 68,
    AmbientStrumming = 69,
    AmbientWindBlow1 = 70,
    AmbientWindBlow1a = 71,
    AmbientWindBlow1b = 72,
    AmbientMonsterRoar = 73,
    AmbientGoldPieces = 74,
    AmbientBirdCall = 75,
    AmbientDoorClose = 76,
    FanfareSolo = 77,
    DrawWeapon = 78,
    CastSpell1 = 81,
    CastSpell2 = 82,
    CastSpell3 = 83,
    CastSpell4 = 84,
    CastSpell5 = 85,
    SpellImpactMagic = 86,
    SpellImpactPoison = 87,
    SpellImpactShock = 88,
    SpellImpactFire = 89,
    SpellImpactCold = 90,
    FallDamage = 91,
    FallHard = 92,
    NormalDoorClose = 93,
    NormalDoorOpen = 94,
    LongSlam = 95,
    LevelUp = 96,
    HorseClop = 97,
    Spooky2 = 98,
    AnimalHorse = 99,
    AnimalDog = 100,
    AnimalCat = 101,
    AnimalPig = 102,
    AnimalCow = 103,
    HorseAndCart = 104,
    SwingLowPitch = 105,
    SwingHighPitch = 106,
    Bells = 107,
    Hit1 = 108,
    Hit2 = 109,
    Hit3 = 110,
    Hit4 = 111,
    Hit5 = 112,
    AmbientDistantHowl = 113,
    AmbientWaterBubbles = 114,
    EnemyRatMove = 115,
    EnemyRatBark = 116,
    EnemyRatAttack = 117,
    EnemyImpMove = 118,
    EnemyImpBark = 119,
    EnemyImpAttack = 120,
    EnemySprigganMove = 121,
    EnemySprigganBark = 122,
    EnemySprigganAttack = 123,
    EnemyGiantBatMove = 124,
    EnemyGiantBatBark = 125,
    EnemyGiantBatAttack = 126,
    EnemyBearMove = 127,
    EnemyBearBark = 128,
    EnemyBearAttack = 129,
    EnemyTigerMove = 130,
    EnemyTigerBark = 131,
    EnemyTigerAttack = 132,
    EnemySpiderMove = 133,
    EnemySpiderBark = 134,
    EnemySpiderAttack = 135,
    EnemyOrcMove = 136,
    EnemyOrcBark = 137,
    EnemyOrcAttack = 138,
    EnemyCentaurMove = 139,
    EnemyCentaurBark = 140,
    EnemyCentaurAttack = 141,
    EnemyWerewolfMove = 142,
    EnemyWerewolfBark = 143,
    EnemyWerewolfAttack = 144,
    EnemyNymphMove = 145,
    EnemyNymphBark = 146,
    EnemyNymphAttack = 147,
    EnemyEelMove = 148,
    EnemyEelBark = 149,
    EnemyEelAttack = 150,
    EnemyOrcSergeantMove = 151,
    EnemyOrcSergeantBark = 152,
    EnemyOrcSergeantAttack = 153,
    EnemyHarpyMove = 154,
    EnemyHarpyBark = 155,
    EnemyHarpyAttack = 156,
    EnemyWereboarMove = 157,
    EnemyWereboarBark = 158,
    EnemyWereboarAttack = 159,
    EnemySkeletonMove = 160,
    EnemySkeletonBark = 161,
    EnemySkeletonAttack = 162,
    EnemyGiantMove = 163,
    EnemyGiantBark = 164,
    EnemyGiantAttack = 165,
    EnemyZombieMove = 166,
    EnemyZombieBark = 167,
    EnemyZombieAttack = 168,
    EnemyGhostMove = 169,
    EnemyGhostBark = 170,
    EnemyGhostAttack = 171,
    EnemyMummyMove = 172,
    EnemyMummyBark = 173,
    EnemyMummyAttack = 174,
    EnemyScorpionMove = 175,
    EnemyScorpionBark = 176,
    EnemyScorpionAttack = 177,
    EnemyOrcShamanMove = 178,
    EnemyOrcShamanBark = 179,
    EnemyGargoyleMove = 181,
    EnemyGargoyleBark = 182,
    EnemyGargoyleAttack = 180,
    EnemyWraithMove = 183,
    EnemyWraithBark = 184,
    EnemyWraithAttack = 185,
    EnemyOrcWarlordMove = 186,
    EnemyOrcWarlordBark = 187,
    EnemyOrcWarlordAttack = 188,
    EnemyFrostDaedraMove = 189,
    EnemyFrostDaedraBark = 190,
    EnemyFrostDaedraAttack = 191,
    EnemyFireDaedraMove = 192,
    EnemyFireDaedraBark = 193,
    EnemyFireDaedraAttack = 194,
    EnemyLesserDaedraMove = 195,
    EnemyLesserDaedraBark = 196,
    EnemyLesserDaedraAttack = 197,
    EnemyFemaleVampireMove = 198,
    EnemyFemaleVampireBark = 199,
    EnemyFemaleVampireAttack = 200,
    EnemySeducerMove = 201,
    EnemySeducerBark = 202,
    EnemySeducerAttack = 203,
    EnemyVampireMove = 204,
    EnemyVampireBark = 205,
    EnemyVampireAttack = 206,
    EnemyDaedraLordMove = 207,
    EnemyDaedraLordBark = 208,
    EnemyDaedraLordAttack = 209,
    EnemyLichMove = 210,
    EnemyLichBark = 211,
    EnemyLichAttack = 212,
    EnemyLichKingMove = 213,
    EnemyLichKingBark = 214,
    EnemyLichKingAttack = 215,
    EnemyFaeryDragonMove = 216,
    EnemyFaeryDragonBark = 217,
    EnemyFaeryDragonAttack = 218,
    EnemyFireAtronachMove = 219,
    EnemyFireAtronachBark = 220,
    EnemyFireAtronachAttack = 221,
    EnemyIronAtronachMove = 222,
    EnemyIronAtronachBark = 223,
    EnemyIronAtronachAttack = 224,
    EnemyFleshAtronachMove = 225,
    EnemyFleshAtronachBark = 226,
    EnemyFleshAtronachAttack = 227,
    EnemyIceAtronachMove = 228,
    EnemyIceAtronachBark = 229,
    EnemyIceAtronachAttack = 230,
    EnemyCentaur2Move = 231,
    EnemyCentaur2Bark = 232,
    EnemyCentaur2Attack = 233,
    EnemyImp2Move = 234,
    EnemyImp2Bark = 235,
    EnemyImp2Attack = 236,
    EnemyDreughMove = 237,
    EnemyDreughBark = 238,
    EnemyDreughAttack = 239,
    EnemyLamiaMove = 240,
    EnemyLamiaBark = 241,
    EnemyLamiaAttack = 242,
    EnemyHumanMove = 243,
    EnemyHumanBark = 244,
    EnemyHumanAttack = 245,
    HorseClop2 = 298,
    PainSound = 299,
    DiceRoll = 300,
    ArenaMinotaur = 301,
    HaltWeak = 302,
    ArenaHomonculus = 305,
    ArenaIceGolem = 307,
    FootstepMetal = 308,
    PlayerFootstepStone1 = 309,
    PlayerFootstepOutside1 = 310,
    PlayerFootstepSnow1 = 311,
    PlayerSwimming = 312,
    PlayerFootstepWood1 = 313,
    ArenaGoblin = 314,
    ActivateLockUnlock = 316,
    ArenaSkeleton = 318,
    ArenaLich = 319,
    ArenaLizardMan = 320,
    ArenaGhoul2 = 321,
    ActivateOpenGate = 325,
    ArenaOpenDoor = 326,
    ArenaOrc = 327,
    ActivateRatchet = 328,
    ArenaRat = 329,
    PlayerFootstepStone2 = 330,
    PlayerFootstepOutside2 = 331,
    PlayerFootstepSnow2 = 332,
    PlayerFootstepWood2 = 333,
    SplashSmallLow = 334,
    HighPitchWail = 335,
    HighPitchScream = 336,
    ArenaSpider = 337,
    ArenaFootstep3 = 340,
    SplashLarge = 342,
    ArenaStoneGolem = 344,
    ArenaStopThief = 345,
    SplashSmall = 346,
    SwingMediumPitch = 347,
    StormLightningShort = 348,
    StormLightningThunder = 349,
    StormThunderRoll = 350,
    ArenaTroll = 351,
    SwingMediumPitch2 = 353,
    ArenaVampire = 354,
    ArenaWolf = 357,
    ArenaWraith = 358,
    ArenaZombie = 359,
    ButtonClick = 360,
    GoldPieces = 361,
    PageTurn = 362,
    ParchmentScratching = 363,
    MakeItem = 364,
    MakePotion = 365,
    RaceBreton = 366,
    RaceRedguard = 367,
    RaceNord = 368,
    RaceDarkElf = 369,
    RaceHighElf = 370,
    RaceWoodElf = 371,
    RaceKhajiit = 372,
    RaceArgonian = 373,
    SelectClassDrums = 374,
    DiceRoll2 = 376,
    EquipShortBlade = 377,
    EquipLongBlade = 378,
    EquipTwoHandedBlade = 379,
    EquipStaff = 380,
    EquipClothing = 381,
    EquipJewellery = 383,
    OpenBook = 384,
    SnoringByFire = 385,
    MaleGasp = 386,
    FemaleGasp = 387,
    BlowingWindIntro = 388,
    AmbientRaining = 389,
    BretonMalePain1 = 390,
    BretonMalePain2 = 391,
    BretonMalePain3 = 392,
    RedguardMalePain1 = 393,
    RedguardMalePain2 = 394,
    RedguardMalePain3 = 395,
    NordMalePain1 = 396,
    NordMalePain2 = 397,
    NordMalePain3 = 398,
    DarkElfMalePain1 = 399,
    DarkElfMalePain2 = 400,
    DarkElfMalePain3 = 401,
    HighElfMalePain1 = 402,
    HighElfMalePain2 = 403,
    HighElfMalePain3 = 404,
    WoodElfMalePain1 = 405,
    WoodElfMalePain2 = 406,
    WoodElfMalePain3 = 407,
    KhajiitMalePain1 = 408,
    KhajiitMalePain2 = 409,
    KhajiitMalePain3 = 410,
    ArgonianMalePain1 = 411,
    ArgonianMalePain2 = 412,
    EquipMaceOrHammer = 413,
    EquipFlail = 414,
    EquipAxe = 415,
    EquipBow = 416,
    EquipLeather = 417,
    EquipChain = 418,
    EquipPlate = 419,
    Burning = 420,
    LongBeep = 421,
    ArenaIronGolem = 422,
    SpookyHigh2 = 423,
    KhajiitFemalePain3 = 424,
    ArgonianFemalePain1 = 425,
    ArgonianFemalePain2 = 426,
    ArgonianFemalePain3 = 427,
    Parry1 = 428,
    Parry2 = 429,
    Parry3 = 430,
    Parry4 = 431,
    Parry5 = 432,
    Parry6 = 433,
    Parry7 = 434,
    Parry8 = 435,
    Parry9 = 436,
    BirdCall1 = 437,
    BirdCall2 = 438,
    WaterGentle = 439,
    WaterRough = 440,
    AmbientPeople1 = 441,
    AmbientPeople2 = 442,
    AmbientPeople3 = 443,
    AmbientPeople4 = 444,
    AmbientPeople5 = 445,
    AmbientPeople6 = 446,
    AmbientPeople7 = 447,
    AmbientPeople8 = 448,
    AmbientPeople9 = 449,
    AmbientPeople10 = 450,
    Vengeance = 455,
    Halt = 456,
    Halt2 = 457,
    Groan = 458,
}

impl SoundClips {
    /// The positional `DAGGER.SND` directory index this clip is played from —
    /// exactly the discriminant DFU passes to `GetAudioClip(int soundIndex)`.
    /// `None` for the DFU sentinel `None = -1`, which is not a directory entry.
    pub const fn index(self) -> Option<usize> {
        let discriminant = self as i32;
        if discriminant < 0 {
            None
        } else {
            Some(discriminant as usize)
        }
    }

    /// DFU member name of this variant, verbatim (the alias resolves to the
    /// name of the variant that owns the discriminant).
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::SpookyHigh => "SpookyHigh",
            Self::SpookyMid => "SpookyMid",
            Self::SpookyLow => "SpookyLow",
            Self::ArrowShoot => "ArrowShoot",
            Self::ArrowHit => "ArrowHit",
            Self::Invalid => "Invalid",
            Self::AmbientCrickets => "AmbientCrickets",
            Self::PlayerDoorBash => "PlayerDoorBash",
            Self::ArenaFireDaemon => "ArenaFireDaemon",
            Self::AmbientCreepyBirdLaughs => "AmbientCreepyBirdLaughs",
            Self::AmbientCreepyBirdCall => "AmbientCreepyBirdCall",
            Self::BodyFall => "BodyFall",
            Self::Ignite => "Ignite",
            Self::ActivateGears => "ActivateGears",
            Self::ActivateDoorClose => "ActivateDoorClose",
            Self::ArenaFootstep1 => "ArenaFootstep1",
            Self::ArenaFootstep2 => "ArenaFootstep2",
            Self::DungeonDoorClose => "DungeonDoorClose",
            Self::DungeonDoorOpen => "DungeonDoorOpen",
            Self::AmbientDripShortHigh => "AmbientDripShortHigh",
            Self::AmbientDripLongHigh => "AmbientDripLongHigh",
            Self::AmbientDrums => "AmbientDrums",
            Self::AmbientWindMoan2 => "AmbientWindMoan2",
            Self::ArenaHitSound => "ArenaHitSound",
            Self::ArenaFanfareLevelUp => "ArenaFanfareLevelUp",
            Self::ArenaFanfareStaffOfChaos => "ArenaFanfareStaffOfChaos",
            Self::ArenaFireDaemon2 => "ArenaFireDaemon2",
            Self::ArenaGhost => "ArenaGhost",
            Self::ArenaGhoul => "ArenaGhoul",
            Self::ActivateGrind => "ActivateGrind",
            Self::ArenaHellHound => "ArenaHellHound",
            Self::ArgonianMalePain3 => "ArgonianMalePain3",
            Self::BretonFemalePain1 => "BretonFemalePain1",
            Self::BretonFemalePain2 => "BretonFemalePain2",
            Self::BretonFemalePain3 => "BretonFemalePain3",
            Self::RedguardFemalePain1 => "RedguardFemalePain1",
            Self::RedguardFemalePain2 => "RedguardFemalePain2",
            Self::RedguardFemalePain3 => "RedguardFemalePain3",
            Self::NordFemalePain1 => "NordFemalePain1",
            Self::NordFemalePain2 => "NordFemalePain2",
            Self::NordFemalePain3 => "NordFemalePain3",
            Self::DarkElfFemalePain1 => "DarkElfFemalePain1",
            Self::DarkElfFemalePain2 => "DarkElfFemalePain2",
            Self::DarkElfFemalePain3 => "DarkElfFemalePain3",
            Self::HighElfFemalePain1 => "HighElfFemalePain1",
            Self::HighElfFemalePain2 => "HighElfFemalePain2",
            Self::HighElfFemalePain3 => "HighElfFemalePain3",
            Self::WoodElfFemalePain1 => "WoodElfFemalePain1",
            Self::WoodElfFemalePain2 => "WoodElfFemalePain2",
            Self::WoodElfFemalePain3 => "WoodElfFemalePain3",
            Self::KhajiitFemalePain1 => "KhajiitFemalePain1",
            Self::KhajiitFemalePain2 => "KhajiitFemalePain2",
            Self::AmbientDripShort => "AmbientDripShort",
            Self::AmbientDripLong => "AmbientDripLong",
            Self::AmbientWindMoan => "AmbientWindMoan",
            Self::AmbientWindMoanDeep => "AmbientWindMoanDeep",
            Self::AmbientDoorOpen => "AmbientDoorOpen",
            Self::AmbientGrind => "AmbientGrind",
            Self::AmbientStrumming => "AmbientStrumming",
            Self::AmbientWindBlow1 => "AmbientWindBlow1",
            Self::AmbientWindBlow1a => "AmbientWindBlow1a",
            Self::AmbientWindBlow1b => "AmbientWindBlow1b",
            Self::AmbientMonsterRoar => "AmbientMonsterRoar",
            Self::AmbientGoldPieces => "AmbientGoldPieces",
            Self::AmbientBirdCall => "AmbientBirdCall",
            Self::AmbientDoorClose => "AmbientDoorClose",
            Self::FanfareSolo => "FanfareSolo",
            Self::DrawWeapon => "DrawWeapon",
            Self::CastSpell1 => "CastSpell1",
            Self::CastSpell2 => "CastSpell2",
            Self::CastSpell3 => "CastSpell3",
            Self::CastSpell4 => "CastSpell4",
            Self::CastSpell5 => "CastSpell5",
            Self::SpellImpactMagic => "SpellImpactMagic",
            Self::SpellImpactPoison => "SpellImpactPoison",
            Self::SpellImpactShock => "SpellImpactShock",
            Self::SpellImpactFire => "SpellImpactFire",
            Self::SpellImpactCold => "SpellImpactCold",
            Self::FallDamage => "FallDamage",
            Self::FallHard => "FallHard",
            Self::NormalDoorClose => "NormalDoorClose",
            Self::NormalDoorOpen => "NormalDoorOpen",
            Self::LongSlam => "LongSlam",
            Self::LevelUp => "LevelUp",
            Self::HorseClop => "HorseClop",
            Self::Spooky2 => "Spooky2",
            Self::AnimalHorse => "AnimalHorse",
            Self::AnimalDog => "AnimalDog",
            Self::AnimalCat => "AnimalCat",
            Self::AnimalPig => "AnimalPig",
            Self::AnimalCow => "AnimalCow",
            Self::HorseAndCart => "HorseAndCart",
            Self::SwingLowPitch => "SwingLowPitch",
            Self::SwingHighPitch => "SwingHighPitch",
            Self::Bells => "Bells",
            Self::Hit1 => "Hit1",
            Self::Hit2 => "Hit2",
            Self::Hit3 => "Hit3",
            Self::Hit4 => "Hit4",
            Self::Hit5 => "Hit5",
            Self::AmbientDistantHowl => "AmbientDistantHowl",
            Self::AmbientWaterBubbles => "AmbientWaterBubbles",
            Self::EnemyRatMove => "EnemyRatMove",
            Self::EnemyRatBark => "EnemyRatBark",
            Self::EnemyRatAttack => "EnemyRatAttack",
            Self::EnemyImpMove => "EnemyImpMove",
            Self::EnemyImpBark => "EnemyImpBark",
            Self::EnemyImpAttack => "EnemyImpAttack",
            Self::EnemySprigganMove => "EnemySprigganMove",
            Self::EnemySprigganBark => "EnemySprigganBark",
            Self::EnemySprigganAttack => "EnemySprigganAttack",
            Self::EnemyGiantBatMove => "EnemyGiantBatMove",
            Self::EnemyGiantBatBark => "EnemyGiantBatBark",
            Self::EnemyGiantBatAttack => "EnemyGiantBatAttack",
            Self::EnemyBearMove => "EnemyBearMove",
            Self::EnemyBearBark => "EnemyBearBark",
            Self::EnemyBearAttack => "EnemyBearAttack",
            Self::EnemyTigerMove => "EnemyTigerMove",
            Self::EnemyTigerBark => "EnemyTigerBark",
            Self::EnemyTigerAttack => "EnemyTigerAttack",
            Self::EnemySpiderMove => "EnemySpiderMove",
            Self::EnemySpiderBark => "EnemySpiderBark",
            Self::EnemySpiderAttack => "EnemySpiderAttack",
            Self::EnemyOrcMove => "EnemyOrcMove",
            Self::EnemyOrcBark => "EnemyOrcBark",
            Self::EnemyOrcAttack => "EnemyOrcAttack",
            Self::EnemyCentaurMove => "EnemyCentaurMove",
            Self::EnemyCentaurBark => "EnemyCentaurBark",
            Self::EnemyCentaurAttack => "EnemyCentaurAttack",
            Self::EnemyWerewolfMove => "EnemyWerewolfMove",
            Self::EnemyWerewolfBark => "EnemyWerewolfBark",
            Self::EnemyWerewolfAttack => "EnemyWerewolfAttack",
            Self::EnemyNymphMove => "EnemyNymphMove",
            Self::EnemyNymphBark => "EnemyNymphBark",
            Self::EnemyNymphAttack => "EnemyNymphAttack",
            Self::EnemyEelMove => "EnemyEelMove",
            Self::EnemyEelBark => "EnemyEelBark",
            Self::EnemyEelAttack => "EnemyEelAttack",
            Self::EnemyOrcSergeantMove => "EnemyOrcSergeantMove",
            Self::EnemyOrcSergeantBark => "EnemyOrcSergeantBark",
            Self::EnemyOrcSergeantAttack => "EnemyOrcSergeantAttack",
            Self::EnemyHarpyMove => "EnemyHarpyMove",
            Self::EnemyHarpyBark => "EnemyHarpyBark",
            Self::EnemyHarpyAttack => "EnemyHarpyAttack",
            Self::EnemyWereboarMove => "EnemyWereboarMove",
            Self::EnemyWereboarBark => "EnemyWereboarBark",
            Self::EnemyWereboarAttack => "EnemyWereboarAttack",
            Self::EnemySkeletonMove => "EnemySkeletonMove",
            Self::EnemySkeletonBark => "EnemySkeletonBark",
            Self::EnemySkeletonAttack => "EnemySkeletonAttack",
            Self::EnemyGiantMove => "EnemyGiantMove",
            Self::EnemyGiantBark => "EnemyGiantBark",
            Self::EnemyGiantAttack => "EnemyGiantAttack",
            Self::EnemyZombieMove => "EnemyZombieMove",
            Self::EnemyZombieBark => "EnemyZombieBark",
            Self::EnemyZombieAttack => "EnemyZombieAttack",
            Self::EnemyGhostMove => "EnemyGhostMove",
            Self::EnemyGhostBark => "EnemyGhostBark",
            Self::EnemyGhostAttack => "EnemyGhostAttack",
            Self::EnemyMummyMove => "EnemyMummyMove",
            Self::EnemyMummyBark => "EnemyMummyBark",
            Self::EnemyMummyAttack => "EnemyMummyAttack",
            Self::EnemyScorpionMove => "EnemyScorpionMove",
            Self::EnemyScorpionBark => "EnemyScorpionBark",
            Self::EnemyScorpionAttack => "EnemyScorpionAttack",
            Self::EnemyOrcShamanMove => "EnemyOrcShamanMove",
            Self::EnemyOrcShamanBark => "EnemyOrcShamanBark",
            Self::EnemyGargoyleMove => "EnemyGargoyleMove",
            Self::EnemyGargoyleBark => "EnemyGargoyleBark",
            Self::EnemyGargoyleAttack => "EnemyGargoyleAttack",
            Self::EnemyWraithMove => "EnemyWraithMove",
            Self::EnemyWraithBark => "EnemyWraithBark",
            Self::EnemyWraithAttack => "EnemyWraithAttack",
            Self::EnemyOrcWarlordMove => "EnemyOrcWarlordMove",
            Self::EnemyOrcWarlordBark => "EnemyOrcWarlordBark",
            Self::EnemyOrcWarlordAttack => "EnemyOrcWarlordAttack",
            Self::EnemyFrostDaedraMove => "EnemyFrostDaedraMove",
            Self::EnemyFrostDaedraBark => "EnemyFrostDaedraBark",
            Self::EnemyFrostDaedraAttack => "EnemyFrostDaedraAttack",
            Self::EnemyFireDaedraMove => "EnemyFireDaedraMove",
            Self::EnemyFireDaedraBark => "EnemyFireDaedraBark",
            Self::EnemyFireDaedraAttack => "EnemyFireDaedraAttack",
            Self::EnemyLesserDaedraMove => "EnemyLesserDaedraMove",
            Self::EnemyLesserDaedraBark => "EnemyLesserDaedraBark",
            Self::EnemyLesserDaedraAttack => "EnemyLesserDaedraAttack",
            Self::EnemyFemaleVampireMove => "EnemyFemaleVampireMove",
            Self::EnemyFemaleVampireBark => "EnemyFemaleVampireBark",
            Self::EnemyFemaleVampireAttack => "EnemyFemaleVampireAttack",
            Self::EnemySeducerMove => "EnemySeducerMove",
            Self::EnemySeducerBark => "EnemySeducerBark",
            Self::EnemySeducerAttack => "EnemySeducerAttack",
            Self::EnemyVampireMove => "EnemyVampireMove",
            Self::EnemyVampireBark => "EnemyVampireBark",
            Self::EnemyVampireAttack => "EnemyVampireAttack",
            Self::EnemyDaedraLordMove => "EnemyDaedraLordMove",
            Self::EnemyDaedraLordBark => "EnemyDaedraLordBark",
            Self::EnemyDaedraLordAttack => "EnemyDaedraLordAttack",
            Self::EnemyLichMove => "EnemyLichMove",
            Self::EnemyLichBark => "EnemyLichBark",
            Self::EnemyLichAttack => "EnemyLichAttack",
            Self::EnemyLichKingMove => "EnemyLichKingMove",
            Self::EnemyLichKingBark => "EnemyLichKingBark",
            Self::EnemyLichKingAttack => "EnemyLichKingAttack",
            Self::EnemyFaeryDragonMove => "EnemyFaeryDragonMove",
            Self::EnemyFaeryDragonBark => "EnemyFaeryDragonBark",
            Self::EnemyFaeryDragonAttack => "EnemyFaeryDragonAttack",
            Self::EnemyFireAtronachMove => "EnemyFireAtronachMove",
            Self::EnemyFireAtronachBark => "EnemyFireAtronachBark",
            Self::EnemyFireAtronachAttack => "EnemyFireAtronachAttack",
            Self::EnemyIronAtronachMove => "EnemyIronAtronachMove",
            Self::EnemyIronAtronachBark => "EnemyIronAtronachBark",
            Self::EnemyIronAtronachAttack => "EnemyIronAtronachAttack",
            Self::EnemyFleshAtronachMove => "EnemyFleshAtronachMove",
            Self::EnemyFleshAtronachBark => "EnemyFleshAtronachBark",
            Self::EnemyFleshAtronachAttack => "EnemyFleshAtronachAttack",
            Self::EnemyIceAtronachMove => "EnemyIceAtronachMove",
            Self::EnemyIceAtronachBark => "EnemyIceAtronachBark",
            Self::EnemyIceAtronachAttack => "EnemyIceAtronachAttack",
            Self::EnemyCentaur2Move => "EnemyCentaur2Move",
            Self::EnemyCentaur2Bark => "EnemyCentaur2Bark",
            Self::EnemyCentaur2Attack => "EnemyCentaur2Attack",
            Self::EnemyImp2Move => "EnemyImp2Move",
            Self::EnemyImp2Bark => "EnemyImp2Bark",
            Self::EnemyImp2Attack => "EnemyImp2Attack",
            Self::EnemyDreughMove => "EnemyDreughMove",
            Self::EnemyDreughBark => "EnemyDreughBark",
            Self::EnemyDreughAttack => "EnemyDreughAttack",
            Self::EnemyLamiaMove => "EnemyLamiaMove",
            Self::EnemyLamiaBark => "EnemyLamiaBark",
            Self::EnemyLamiaAttack => "EnemyLamiaAttack",
            Self::EnemyHumanMove => "EnemyHumanMove",
            Self::EnemyHumanBark => "EnemyHumanBark",
            Self::EnemyHumanAttack => "EnemyHumanAttack",
            Self::HorseClop2 => "HorseClop2",
            Self::PainSound => "PainSound",
            Self::DiceRoll => "DiceRoll",
            Self::ArenaMinotaur => "ArenaMinotaur",
            Self::HaltWeak => "HaltWeak",
            Self::ArenaHomonculus => "ArenaHomonculus",
            Self::ArenaIceGolem => "ArenaIceGolem",
            Self::FootstepMetal => "FootstepMetal",
            Self::PlayerFootstepStone1 => "PlayerFootstepStone1",
            Self::PlayerFootstepOutside1 => "PlayerFootstepOutside1",
            Self::PlayerFootstepSnow1 => "PlayerFootstepSnow1",
            Self::PlayerSwimming => "PlayerSwimming",
            Self::PlayerFootstepWood1 => "PlayerFootstepWood1",
            Self::ArenaGoblin => "ArenaGoblin",
            Self::ActivateLockUnlock => "ActivateLockUnlock",
            Self::ArenaSkeleton => "ArenaSkeleton",
            Self::ArenaLich => "ArenaLich",
            Self::ArenaLizardMan => "ArenaLizardMan",
            Self::ArenaGhoul2 => "ArenaGhoul2",
            Self::ActivateOpenGate => "ActivateOpenGate",
            Self::ArenaOpenDoor => "ArenaOpenDoor",
            Self::ArenaOrc => "ArenaOrc",
            Self::ActivateRatchet => "ActivateRatchet",
            Self::ArenaRat => "ArenaRat",
            Self::PlayerFootstepStone2 => "PlayerFootstepStone2",
            Self::PlayerFootstepOutside2 => "PlayerFootstepOutside2",
            Self::PlayerFootstepSnow2 => "PlayerFootstepSnow2",
            Self::PlayerFootstepWood2 => "PlayerFootstepWood2",
            Self::SplashSmallLow => "SplashSmallLow",
            Self::HighPitchWail => "HighPitchWail",
            Self::HighPitchScream => "HighPitchScream",
            Self::ArenaSpider => "ArenaSpider",
            Self::ArenaFootstep3 => "ArenaFootstep3",
            Self::SplashLarge => "SplashLarge",
            Self::ArenaStoneGolem => "ArenaStoneGolem",
            Self::ArenaStopThief => "ArenaStopThief",
            Self::SplashSmall => "SplashSmall",
            Self::SwingMediumPitch => "SwingMediumPitch",
            Self::StormLightningShort => "StormLightningShort",
            Self::StormLightningThunder => "StormLightningThunder",
            Self::StormThunderRoll => "StormThunderRoll",
            Self::ArenaTroll => "ArenaTroll",
            Self::SwingMediumPitch2 => "SwingMediumPitch2",
            Self::ArenaVampire => "ArenaVampire",
            Self::ArenaWolf => "ArenaWolf",
            Self::ArenaWraith => "ArenaWraith",
            Self::ArenaZombie => "ArenaZombie",
            Self::ButtonClick => "ButtonClick",
            Self::GoldPieces => "GoldPieces",
            Self::PageTurn => "PageTurn",
            Self::ParchmentScratching => "ParchmentScratching",
            Self::MakeItem => "MakeItem",
            Self::MakePotion => "MakePotion",
            Self::RaceBreton => "RaceBreton",
            Self::RaceRedguard => "RaceRedguard",
            Self::RaceNord => "RaceNord",
            Self::RaceDarkElf => "RaceDarkElf",
            Self::RaceHighElf => "RaceHighElf",
            Self::RaceWoodElf => "RaceWoodElf",
            Self::RaceKhajiit => "RaceKhajiit",
            Self::RaceArgonian => "RaceArgonian",
            Self::SelectClassDrums => "SelectClassDrums",
            Self::DiceRoll2 => "DiceRoll2",
            Self::EquipShortBlade => "EquipShortBlade",
            Self::EquipLongBlade => "EquipLongBlade",
            Self::EquipTwoHandedBlade => "EquipTwoHandedBlade",
            Self::EquipStaff => "EquipStaff",
            Self::EquipClothing => "EquipClothing",
            Self::EquipJewellery => "EquipJewellery",
            Self::OpenBook => "OpenBook",
            Self::SnoringByFire => "SnoringByFire",
            Self::MaleGasp => "MaleGasp",
            Self::FemaleGasp => "FemaleGasp",
            Self::BlowingWindIntro => "BlowingWindIntro",
            Self::AmbientRaining => "AmbientRaining",
            Self::BretonMalePain1 => "BretonMalePain1",
            Self::BretonMalePain2 => "BretonMalePain2",
            Self::BretonMalePain3 => "BretonMalePain3",
            Self::RedguardMalePain1 => "RedguardMalePain1",
            Self::RedguardMalePain2 => "RedguardMalePain2",
            Self::RedguardMalePain3 => "RedguardMalePain3",
            Self::NordMalePain1 => "NordMalePain1",
            Self::NordMalePain2 => "NordMalePain2",
            Self::NordMalePain3 => "NordMalePain3",
            Self::DarkElfMalePain1 => "DarkElfMalePain1",
            Self::DarkElfMalePain2 => "DarkElfMalePain2",
            Self::DarkElfMalePain3 => "DarkElfMalePain3",
            Self::HighElfMalePain1 => "HighElfMalePain1",
            Self::HighElfMalePain2 => "HighElfMalePain2",
            Self::HighElfMalePain3 => "HighElfMalePain3",
            Self::WoodElfMalePain1 => "WoodElfMalePain1",
            Self::WoodElfMalePain2 => "WoodElfMalePain2",
            Self::WoodElfMalePain3 => "WoodElfMalePain3",
            Self::KhajiitMalePain1 => "KhajiitMalePain1",
            Self::KhajiitMalePain2 => "KhajiitMalePain2",
            Self::KhajiitMalePain3 => "KhajiitMalePain3",
            Self::ArgonianMalePain1 => "ArgonianMalePain1",
            Self::ArgonianMalePain2 => "ArgonianMalePain2",
            Self::EquipMaceOrHammer => "EquipMaceOrHammer",
            Self::EquipFlail => "EquipFlail",
            Self::EquipAxe => "EquipAxe",
            Self::EquipBow => "EquipBow",
            Self::EquipLeather => "EquipLeather",
            Self::EquipChain => "EquipChain",
            Self::EquipPlate => "EquipPlate",
            Self::Burning => "Burning",
            Self::LongBeep => "LongBeep",
            Self::ArenaIronGolem => "ArenaIronGolem",
            Self::SpookyHigh2 => "SpookyHigh2",
            Self::KhajiitFemalePain3 => "KhajiitFemalePain3",
            Self::ArgonianFemalePain1 => "ArgonianFemalePain1",
            Self::ArgonianFemalePain2 => "ArgonianFemalePain2",
            Self::ArgonianFemalePain3 => "ArgonianFemalePain3",
            Self::Parry1 => "Parry1",
            Self::Parry2 => "Parry2",
            Self::Parry3 => "Parry3",
            Self::Parry4 => "Parry4",
            Self::Parry5 => "Parry5",
            Self::Parry6 => "Parry6",
            Self::Parry7 => "Parry7",
            Self::Parry8 => "Parry8",
            Self::Parry9 => "Parry9",
            Self::BirdCall1 => "BirdCall1",
            Self::BirdCall2 => "BirdCall2",
            Self::WaterGentle => "WaterGentle",
            Self::WaterRough => "WaterRough",
            Self::AmbientPeople1 => "AmbientPeople1",
            Self::AmbientPeople2 => "AmbientPeople2",
            Self::AmbientPeople3 => "AmbientPeople3",
            Self::AmbientPeople4 => "AmbientPeople4",
            Self::AmbientPeople5 => "AmbientPeople5",
            Self::AmbientPeople6 => "AmbientPeople6",
            Self::AmbientPeople7 => "AmbientPeople7",
            Self::AmbientPeople8 => "AmbientPeople8",
            Self::AmbientPeople9 => "AmbientPeople9",
            Self::AmbientPeople10 => "AmbientPeople10",
            Self::Vengeance => "Vengeance",
            Self::Halt => "Halt",
            Self::Halt2 => "Halt2",
            Self::Groan => "Groan",
        }
    }

    /// Catalogue entries that resolve to this variant: one normally, two for
    /// the aliased discriminant 138.
    pub fn definitions(self) -> impl Iterator<Item = &'static SoundClipDef> {
        SOUND_CLIP_CATALOG.iter().filter(move |def| def.clip == self)
    }
}

/// One entry of the DFU `SoundClips` catalogue, in DFU enum order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundClipDef {
    /// DFU member name, verbatim.
    pub name: &'static str,
    /// Variant carrying the discriminant (the alias resolves to its target).
    pub clip: SoundClips,
    /// `true` for the extra name of a duplicated discriminant
    /// (`EnemyOrcShamanAttack = 138` shares `EnemyOrcAttack = 138`).
    pub alias: bool,
}

/// Builds one catalogue entry; keeps the 375-row table readable.
const fn clip_def(name: &'static str, clip: SoundClips, alias: bool) -> SoundClipDef {
    SoundClipDef { name, clip, alias }
}

/// All **375** DFU members in enum order. The duplicated discriminant 138
/// appears twice (`EnemyOrcAttack` and the flagged `EnemyOrcShamanAttack`);
/// every other value appears once.
pub const SOUND_CLIP_CATALOG: &[SoundClipDef] = &[
    clip_def("None", SoundClips::None, false),
    clip_def("SpookyHigh", SoundClips::SpookyHigh, false),
    clip_def("SpookyMid", SoundClips::SpookyMid, false),
    clip_def("SpookyLow", SoundClips::SpookyLow, false),
    clip_def("ArrowShoot", SoundClips::ArrowShoot, false),
    clip_def("ArrowHit", SoundClips::ArrowHit, false),
    clip_def("Invalid", SoundClips::Invalid, false),
    clip_def("AmbientCrickets", SoundClips::AmbientCrickets, false),
    clip_def("PlayerDoorBash", SoundClips::PlayerDoorBash, false),
    clip_def("ArenaFireDaemon", SoundClips::ArenaFireDaemon, false),
    clip_def("AmbientCreepyBirdLaughs", SoundClips::AmbientCreepyBirdLaughs, false),
    clip_def("AmbientCreepyBirdCall", SoundClips::AmbientCreepyBirdCall, false),
    clip_def("BodyFall", SoundClips::BodyFall, false),
    clip_def("Ignite", SoundClips::Ignite, false),
    clip_def("ActivateGears", SoundClips::ActivateGears, false),
    clip_def("ActivateDoorClose", SoundClips::ActivateDoorClose, false),
    clip_def("ArenaFootstep1", SoundClips::ArenaFootstep1, false),
    clip_def("ArenaFootstep2", SoundClips::ArenaFootstep2, false),
    clip_def("DungeonDoorClose", SoundClips::DungeonDoorClose, false),
    clip_def("DungeonDoorOpen", SoundClips::DungeonDoorOpen, false),
    clip_def("AmbientDripShortHigh", SoundClips::AmbientDripShortHigh, false),
    clip_def("AmbientDripLongHigh", SoundClips::AmbientDripLongHigh, false),
    clip_def("AmbientDrums", SoundClips::AmbientDrums, false),
    clip_def("AmbientWindMoan2", SoundClips::AmbientWindMoan2, false),
    clip_def("ArenaHitSound", SoundClips::ArenaHitSound, false),
    clip_def("ArenaFanfareLevelUp", SoundClips::ArenaFanfareLevelUp, false),
    clip_def("ArenaFanfareStaffOfChaos", SoundClips::ArenaFanfareStaffOfChaos, false),
    clip_def("ArenaFireDaemon2", SoundClips::ArenaFireDaemon2, false),
    clip_def("ArenaGhost", SoundClips::ArenaGhost, false),
    clip_def("ArenaGhoul", SoundClips::ArenaGhoul, false),
    clip_def("ActivateGrind", SoundClips::ActivateGrind, false),
    clip_def("ArenaHellHound", SoundClips::ArenaHellHound, false),
    clip_def("ArgonianMalePain3", SoundClips::ArgonianMalePain3, false),
    clip_def("BretonFemalePain1", SoundClips::BretonFemalePain1, false),
    clip_def("BretonFemalePain2", SoundClips::BretonFemalePain2, false),
    clip_def("BretonFemalePain3", SoundClips::BretonFemalePain3, false),
    clip_def("RedguardFemalePain1", SoundClips::RedguardFemalePain1, false),
    clip_def("RedguardFemalePain2", SoundClips::RedguardFemalePain2, false),
    clip_def("RedguardFemalePain3", SoundClips::RedguardFemalePain3, false),
    clip_def("NordFemalePain1", SoundClips::NordFemalePain1, false),
    clip_def("NordFemalePain2", SoundClips::NordFemalePain2, false),
    clip_def("NordFemalePain3", SoundClips::NordFemalePain3, false),
    clip_def("DarkElfFemalePain1", SoundClips::DarkElfFemalePain1, false),
    clip_def("DarkElfFemalePain2", SoundClips::DarkElfFemalePain2, false),
    clip_def("DarkElfFemalePain3", SoundClips::DarkElfFemalePain3, false),
    clip_def("HighElfFemalePain1", SoundClips::HighElfFemalePain1, false),
    clip_def("HighElfFemalePain2", SoundClips::HighElfFemalePain2, false),
    clip_def("HighElfFemalePain3", SoundClips::HighElfFemalePain3, false),
    clip_def("WoodElfFemalePain1", SoundClips::WoodElfFemalePain1, false),
    clip_def("WoodElfFemalePain2", SoundClips::WoodElfFemalePain2, false),
    clip_def("WoodElfFemalePain3", SoundClips::WoodElfFemalePain3, false),
    clip_def("KhajiitFemalePain1", SoundClips::KhajiitFemalePain1, false),
    clip_def("KhajiitFemalePain2", SoundClips::KhajiitFemalePain2, false),
    clip_def("AmbientDripShort", SoundClips::AmbientDripShort, false),
    clip_def("AmbientDripLong", SoundClips::AmbientDripLong, false),
    clip_def("AmbientWindMoan", SoundClips::AmbientWindMoan, false),
    clip_def("AmbientWindMoanDeep", SoundClips::AmbientWindMoanDeep, false),
    clip_def("AmbientDoorOpen", SoundClips::AmbientDoorOpen, false),
    clip_def("AmbientGrind", SoundClips::AmbientGrind, false),
    clip_def("AmbientStrumming", SoundClips::AmbientStrumming, false),
    clip_def("AmbientWindBlow1", SoundClips::AmbientWindBlow1, false),
    clip_def("AmbientWindBlow1a", SoundClips::AmbientWindBlow1a, false),
    clip_def("AmbientWindBlow1b", SoundClips::AmbientWindBlow1b, false),
    clip_def("AmbientMonsterRoar", SoundClips::AmbientMonsterRoar, false),
    clip_def("AmbientGoldPieces", SoundClips::AmbientGoldPieces, false),
    clip_def("AmbientBirdCall", SoundClips::AmbientBirdCall, false),
    clip_def("AmbientDoorClose", SoundClips::AmbientDoorClose, false),
    clip_def("FanfareSolo", SoundClips::FanfareSolo, false),
    clip_def("DrawWeapon", SoundClips::DrawWeapon, false),
    clip_def("CastSpell1", SoundClips::CastSpell1, false),
    clip_def("CastSpell2", SoundClips::CastSpell2, false),
    clip_def("CastSpell3", SoundClips::CastSpell3, false),
    clip_def("CastSpell4", SoundClips::CastSpell4, false),
    clip_def("CastSpell5", SoundClips::CastSpell5, false),
    clip_def("SpellImpactMagic", SoundClips::SpellImpactMagic, false),
    clip_def("SpellImpactPoison", SoundClips::SpellImpactPoison, false),
    clip_def("SpellImpactShock", SoundClips::SpellImpactShock, false),
    clip_def("SpellImpactFire", SoundClips::SpellImpactFire, false),
    clip_def("SpellImpactCold", SoundClips::SpellImpactCold, false),
    clip_def("FallDamage", SoundClips::FallDamage, false),
    clip_def("FallHard", SoundClips::FallHard, false),
    clip_def("NormalDoorClose", SoundClips::NormalDoorClose, false),
    clip_def("NormalDoorOpen", SoundClips::NormalDoorOpen, false),
    clip_def("LongSlam", SoundClips::LongSlam, false),
    clip_def("LevelUp", SoundClips::LevelUp, false),
    clip_def("HorseClop", SoundClips::HorseClop, false),
    clip_def("Spooky2", SoundClips::Spooky2, false),
    clip_def("AnimalHorse", SoundClips::AnimalHorse, false),
    clip_def("AnimalDog", SoundClips::AnimalDog, false),
    clip_def("AnimalCat", SoundClips::AnimalCat, false),
    clip_def("AnimalPig", SoundClips::AnimalPig, false),
    clip_def("AnimalCow", SoundClips::AnimalCow, false),
    clip_def("HorseAndCart", SoundClips::HorseAndCart, false),
    clip_def("SwingLowPitch", SoundClips::SwingLowPitch, false),
    clip_def("SwingHighPitch", SoundClips::SwingHighPitch, false),
    clip_def("Bells", SoundClips::Bells, false),
    clip_def("Hit1", SoundClips::Hit1, false),
    clip_def("Hit2", SoundClips::Hit2, false),
    clip_def("Hit3", SoundClips::Hit3, false),
    clip_def("Hit4", SoundClips::Hit4, false),
    clip_def("Hit5", SoundClips::Hit5, false),
    clip_def("AmbientDistantHowl", SoundClips::AmbientDistantHowl, false),
    clip_def("AmbientWaterBubbles", SoundClips::AmbientWaterBubbles, false),
    clip_def("EnemyRatMove", SoundClips::EnemyRatMove, false),
    clip_def("EnemyRatBark", SoundClips::EnemyRatBark, false),
    clip_def("EnemyRatAttack", SoundClips::EnemyRatAttack, false),
    clip_def("EnemyImpMove", SoundClips::EnemyImpMove, false),
    clip_def("EnemyImpBark", SoundClips::EnemyImpBark, false),
    clip_def("EnemyImpAttack", SoundClips::EnemyImpAttack, false),
    clip_def("EnemySprigganMove", SoundClips::EnemySprigganMove, false),
    clip_def("EnemySprigganBark", SoundClips::EnemySprigganBark, false),
    clip_def("EnemySprigganAttack", SoundClips::EnemySprigganAttack, false),
    clip_def("EnemyGiantBatMove", SoundClips::EnemyGiantBatMove, false),
    clip_def("EnemyGiantBatBark", SoundClips::EnemyGiantBatBark, false),
    clip_def("EnemyGiantBatAttack", SoundClips::EnemyGiantBatAttack, false),
    clip_def("EnemyBearMove", SoundClips::EnemyBearMove, false),
    clip_def("EnemyBearBark", SoundClips::EnemyBearBark, false),
    clip_def("EnemyBearAttack", SoundClips::EnemyBearAttack, false),
    clip_def("EnemyTigerMove", SoundClips::EnemyTigerMove, false),
    clip_def("EnemyTigerBark", SoundClips::EnemyTigerBark, false),
    clip_def("EnemyTigerAttack", SoundClips::EnemyTigerAttack, false),
    clip_def("EnemySpiderMove", SoundClips::EnemySpiderMove, false),
    clip_def("EnemySpiderBark", SoundClips::EnemySpiderBark, false),
    clip_def("EnemySpiderAttack", SoundClips::EnemySpiderAttack, false),
    clip_def("EnemyOrcMove", SoundClips::EnemyOrcMove, false),
    clip_def("EnemyOrcBark", SoundClips::EnemyOrcBark, false),
    clip_def("EnemyOrcAttack", SoundClips::EnemyOrcAttack, false),
    clip_def("EnemyCentaurMove", SoundClips::EnemyCentaurMove, false),
    clip_def("EnemyCentaurBark", SoundClips::EnemyCentaurBark, false),
    clip_def("EnemyCentaurAttack", SoundClips::EnemyCentaurAttack, false),
    clip_def("EnemyWerewolfMove", SoundClips::EnemyWerewolfMove, false),
    clip_def("EnemyWerewolfBark", SoundClips::EnemyWerewolfBark, false),
    clip_def("EnemyWerewolfAttack", SoundClips::EnemyWerewolfAttack, false),
    clip_def("EnemyNymphMove", SoundClips::EnemyNymphMove, false),
    clip_def("EnemyNymphBark", SoundClips::EnemyNymphBark, false),
    clip_def("EnemyNymphAttack", SoundClips::EnemyNymphAttack, false),
    clip_def("EnemyEelMove", SoundClips::EnemyEelMove, false),
    clip_def("EnemyEelBark", SoundClips::EnemyEelBark, false),
    clip_def("EnemyEelAttack", SoundClips::EnemyEelAttack, false),
    clip_def("EnemyOrcSergeantMove", SoundClips::EnemyOrcSergeantMove, false),
    clip_def("EnemyOrcSergeantBark", SoundClips::EnemyOrcSergeantBark, false),
    clip_def("EnemyOrcSergeantAttack", SoundClips::EnemyOrcSergeantAttack, false),
    clip_def("EnemyHarpyMove", SoundClips::EnemyHarpyMove, false),
    clip_def("EnemyHarpyBark", SoundClips::EnemyHarpyBark, false),
    clip_def("EnemyHarpyAttack", SoundClips::EnemyHarpyAttack, false),
    clip_def("EnemyWereboarMove", SoundClips::EnemyWereboarMove, false),
    clip_def("EnemyWereboarBark", SoundClips::EnemyWereboarBark, false),
    clip_def("EnemyWereboarAttack", SoundClips::EnemyWereboarAttack, false),
    clip_def("EnemySkeletonMove", SoundClips::EnemySkeletonMove, false),
    clip_def("EnemySkeletonBark", SoundClips::EnemySkeletonBark, false),
    clip_def("EnemySkeletonAttack", SoundClips::EnemySkeletonAttack, false),
    clip_def("EnemyGiantMove", SoundClips::EnemyGiantMove, false),
    clip_def("EnemyGiantBark", SoundClips::EnemyGiantBark, false),
    clip_def("EnemyGiantAttack", SoundClips::EnemyGiantAttack, false),
    clip_def("EnemyZombieMove", SoundClips::EnemyZombieMove, false),
    clip_def("EnemyZombieBark", SoundClips::EnemyZombieBark, false),
    clip_def("EnemyZombieAttack", SoundClips::EnemyZombieAttack, false),
    clip_def("EnemyGhostMove", SoundClips::EnemyGhostMove, false),
    clip_def("EnemyGhostBark", SoundClips::EnemyGhostBark, false),
    clip_def("EnemyGhostAttack", SoundClips::EnemyGhostAttack, false),
    clip_def("EnemyMummyMove", SoundClips::EnemyMummyMove, false),
    clip_def("EnemyMummyBark", SoundClips::EnemyMummyBark, false),
    clip_def("EnemyMummyAttack", SoundClips::EnemyMummyAttack, false),
    clip_def("EnemyScorpionMove", SoundClips::EnemyScorpionMove, false),
    clip_def("EnemyScorpionBark", SoundClips::EnemyScorpionBark, false),
    clip_def("EnemyScorpionAttack", SoundClips::EnemyScorpionAttack, false),
    clip_def("EnemyOrcShamanMove", SoundClips::EnemyOrcShamanMove, false),
    clip_def("EnemyOrcShamanBark", SoundClips::EnemyOrcShamanBark, false),
    clip_def("EnemyOrcShamanAttack", SoundClips::EnemyOrcAttack, true),
    clip_def("EnemyGargoyleMove", SoundClips::EnemyGargoyleMove, false),
    clip_def("EnemyGargoyleBark", SoundClips::EnemyGargoyleBark, false),
    clip_def("EnemyGargoyleAttack", SoundClips::EnemyGargoyleAttack, false),
    clip_def("EnemyWraithMove", SoundClips::EnemyWraithMove, false),
    clip_def("EnemyWraithBark", SoundClips::EnemyWraithBark, false),
    clip_def("EnemyWraithAttack", SoundClips::EnemyWraithAttack, false),
    clip_def("EnemyOrcWarlordMove", SoundClips::EnemyOrcWarlordMove, false),
    clip_def("EnemyOrcWarlordBark", SoundClips::EnemyOrcWarlordBark, false),
    clip_def("EnemyOrcWarlordAttack", SoundClips::EnemyOrcWarlordAttack, false),
    clip_def("EnemyFrostDaedraMove", SoundClips::EnemyFrostDaedraMove, false),
    clip_def("EnemyFrostDaedraBark", SoundClips::EnemyFrostDaedraBark, false),
    clip_def("EnemyFrostDaedraAttack", SoundClips::EnemyFrostDaedraAttack, false),
    clip_def("EnemyFireDaedraMove", SoundClips::EnemyFireDaedraMove, false),
    clip_def("EnemyFireDaedraBark", SoundClips::EnemyFireDaedraBark, false),
    clip_def("EnemyFireDaedraAttack", SoundClips::EnemyFireDaedraAttack, false),
    clip_def("EnemyLesserDaedraMove", SoundClips::EnemyLesserDaedraMove, false),
    clip_def("EnemyLesserDaedraBark", SoundClips::EnemyLesserDaedraBark, false),
    clip_def("EnemyLesserDaedraAttack", SoundClips::EnemyLesserDaedraAttack, false),
    clip_def("EnemyFemaleVampireMove", SoundClips::EnemyFemaleVampireMove, false),
    clip_def("EnemyFemaleVampireBark", SoundClips::EnemyFemaleVampireBark, false),
    clip_def("EnemyFemaleVampireAttack", SoundClips::EnemyFemaleVampireAttack, false),
    clip_def("EnemySeducerMove", SoundClips::EnemySeducerMove, false),
    clip_def("EnemySeducerBark", SoundClips::EnemySeducerBark, false),
    clip_def("EnemySeducerAttack", SoundClips::EnemySeducerAttack, false),
    clip_def("EnemyVampireMove", SoundClips::EnemyVampireMove, false),
    clip_def("EnemyVampireBark", SoundClips::EnemyVampireBark, false),
    clip_def("EnemyVampireAttack", SoundClips::EnemyVampireAttack, false),
    clip_def("EnemyDaedraLordMove", SoundClips::EnemyDaedraLordMove, false),
    clip_def("EnemyDaedraLordBark", SoundClips::EnemyDaedraLordBark, false),
    clip_def("EnemyDaedraLordAttack", SoundClips::EnemyDaedraLordAttack, false),
    clip_def("EnemyLichMove", SoundClips::EnemyLichMove, false),
    clip_def("EnemyLichBark", SoundClips::EnemyLichBark, false),
    clip_def("EnemyLichAttack", SoundClips::EnemyLichAttack, false),
    clip_def("EnemyLichKingMove", SoundClips::EnemyLichKingMove, false),
    clip_def("EnemyLichKingBark", SoundClips::EnemyLichKingBark, false),
    clip_def("EnemyLichKingAttack", SoundClips::EnemyLichKingAttack, false),
    clip_def("EnemyFaeryDragonMove", SoundClips::EnemyFaeryDragonMove, false),
    clip_def("EnemyFaeryDragonBark", SoundClips::EnemyFaeryDragonBark, false),
    clip_def("EnemyFaeryDragonAttack", SoundClips::EnemyFaeryDragonAttack, false),
    clip_def("EnemyFireAtronachMove", SoundClips::EnemyFireAtronachMove, false),
    clip_def("EnemyFireAtronachBark", SoundClips::EnemyFireAtronachBark, false),
    clip_def("EnemyFireAtronachAttack", SoundClips::EnemyFireAtronachAttack, false),
    clip_def("EnemyIronAtronachMove", SoundClips::EnemyIronAtronachMove, false),
    clip_def("EnemyIronAtronachBark", SoundClips::EnemyIronAtronachBark, false),
    clip_def("EnemyIronAtronachAttack", SoundClips::EnemyIronAtronachAttack, false),
    clip_def("EnemyFleshAtronachMove", SoundClips::EnemyFleshAtronachMove, false),
    clip_def("EnemyFleshAtronachBark", SoundClips::EnemyFleshAtronachBark, false),
    clip_def("EnemyFleshAtronachAttack", SoundClips::EnemyFleshAtronachAttack, false),
    clip_def("EnemyIceAtronachMove", SoundClips::EnemyIceAtronachMove, false),
    clip_def("EnemyIceAtronachBark", SoundClips::EnemyIceAtronachBark, false),
    clip_def("EnemyIceAtronachAttack", SoundClips::EnemyIceAtronachAttack, false),
    clip_def("EnemyCentaur2Move", SoundClips::EnemyCentaur2Move, false),
    clip_def("EnemyCentaur2Bark", SoundClips::EnemyCentaur2Bark, false),
    clip_def("EnemyCentaur2Attack", SoundClips::EnemyCentaur2Attack, false),
    clip_def("EnemyImp2Move", SoundClips::EnemyImp2Move, false),
    clip_def("EnemyImp2Bark", SoundClips::EnemyImp2Bark, false),
    clip_def("EnemyImp2Attack", SoundClips::EnemyImp2Attack, false),
    clip_def("EnemyDreughMove", SoundClips::EnemyDreughMove, false),
    clip_def("EnemyDreughBark", SoundClips::EnemyDreughBark, false),
    clip_def("EnemyDreughAttack", SoundClips::EnemyDreughAttack, false),
    clip_def("EnemyLamiaMove", SoundClips::EnemyLamiaMove, false),
    clip_def("EnemyLamiaBark", SoundClips::EnemyLamiaBark, false),
    clip_def("EnemyLamiaAttack", SoundClips::EnemyLamiaAttack, false),
    clip_def("EnemyHumanMove", SoundClips::EnemyHumanMove, false),
    clip_def("EnemyHumanBark", SoundClips::EnemyHumanBark, false),
    clip_def("EnemyHumanAttack", SoundClips::EnemyHumanAttack, false),
    clip_def("HorseClop2", SoundClips::HorseClop2, false),
    clip_def("PainSound", SoundClips::PainSound, false),
    clip_def("DiceRoll", SoundClips::DiceRoll, false),
    clip_def("ArenaMinotaur", SoundClips::ArenaMinotaur, false),
    clip_def("HaltWeak", SoundClips::HaltWeak, false),
    clip_def("ArenaHomonculus", SoundClips::ArenaHomonculus, false),
    clip_def("ArenaIceGolem", SoundClips::ArenaIceGolem, false),
    clip_def("FootstepMetal", SoundClips::FootstepMetal, false),
    clip_def("PlayerFootstepStone1", SoundClips::PlayerFootstepStone1, false),
    clip_def("PlayerFootstepOutside1", SoundClips::PlayerFootstepOutside1, false),
    clip_def("PlayerFootstepSnow1", SoundClips::PlayerFootstepSnow1, false),
    clip_def("PlayerSwimming", SoundClips::PlayerSwimming, false),
    clip_def("PlayerFootstepWood1", SoundClips::PlayerFootstepWood1, false),
    clip_def("ArenaGoblin", SoundClips::ArenaGoblin, false),
    clip_def("ActivateLockUnlock", SoundClips::ActivateLockUnlock, false),
    clip_def("ArenaSkeleton", SoundClips::ArenaSkeleton, false),
    clip_def("ArenaLich", SoundClips::ArenaLich, false),
    clip_def("ArenaLizardMan", SoundClips::ArenaLizardMan, false),
    clip_def("ArenaGhoul2", SoundClips::ArenaGhoul2, false),
    clip_def("ActivateOpenGate", SoundClips::ActivateOpenGate, false),
    clip_def("ArenaOpenDoor", SoundClips::ArenaOpenDoor, false),
    clip_def("ArenaOrc", SoundClips::ArenaOrc, false),
    clip_def("ActivateRatchet", SoundClips::ActivateRatchet, false),
    clip_def("ArenaRat", SoundClips::ArenaRat, false),
    clip_def("PlayerFootstepStone2", SoundClips::PlayerFootstepStone2, false),
    clip_def("PlayerFootstepOutside2", SoundClips::PlayerFootstepOutside2, false),
    clip_def("PlayerFootstepSnow2", SoundClips::PlayerFootstepSnow2, false),
    clip_def("PlayerFootstepWood2", SoundClips::PlayerFootstepWood2, false),
    clip_def("SplashSmallLow", SoundClips::SplashSmallLow, false),
    clip_def("HighPitchWail", SoundClips::HighPitchWail, false),
    clip_def("HighPitchScream", SoundClips::HighPitchScream, false),
    clip_def("ArenaSpider", SoundClips::ArenaSpider, false),
    clip_def("ArenaFootstep3", SoundClips::ArenaFootstep3, false),
    clip_def("SplashLarge", SoundClips::SplashLarge, false),
    clip_def("ArenaStoneGolem", SoundClips::ArenaStoneGolem, false),
    clip_def("ArenaStopThief", SoundClips::ArenaStopThief, false),
    clip_def("SplashSmall", SoundClips::SplashSmall, false),
    clip_def("SwingMediumPitch", SoundClips::SwingMediumPitch, false),
    clip_def("StormLightningShort", SoundClips::StormLightningShort, false),
    clip_def("StormLightningThunder", SoundClips::StormLightningThunder, false),
    clip_def("StormThunderRoll", SoundClips::StormThunderRoll, false),
    clip_def("ArenaTroll", SoundClips::ArenaTroll, false),
    clip_def("SwingMediumPitch2", SoundClips::SwingMediumPitch2, false),
    clip_def("ArenaVampire", SoundClips::ArenaVampire, false),
    clip_def("ArenaWolf", SoundClips::ArenaWolf, false),
    clip_def("ArenaWraith", SoundClips::ArenaWraith, false),
    clip_def("ArenaZombie", SoundClips::ArenaZombie, false),
    clip_def("ButtonClick", SoundClips::ButtonClick, false),
    clip_def("GoldPieces", SoundClips::GoldPieces, false),
    clip_def("PageTurn", SoundClips::PageTurn, false),
    clip_def("ParchmentScratching", SoundClips::ParchmentScratching, false),
    clip_def("MakeItem", SoundClips::MakeItem, false),
    clip_def("MakePotion", SoundClips::MakePotion, false),
    clip_def("RaceBreton", SoundClips::RaceBreton, false),
    clip_def("RaceRedguard", SoundClips::RaceRedguard, false),
    clip_def("RaceNord", SoundClips::RaceNord, false),
    clip_def("RaceDarkElf", SoundClips::RaceDarkElf, false),
    clip_def("RaceHighElf", SoundClips::RaceHighElf, false),
    clip_def("RaceWoodElf", SoundClips::RaceWoodElf, false),
    clip_def("RaceKhajiit", SoundClips::RaceKhajiit, false),
    clip_def("RaceArgonian", SoundClips::RaceArgonian, false),
    clip_def("SelectClassDrums", SoundClips::SelectClassDrums, false),
    clip_def("DiceRoll2", SoundClips::DiceRoll2, false),
    clip_def("EquipShortBlade", SoundClips::EquipShortBlade, false),
    clip_def("EquipLongBlade", SoundClips::EquipLongBlade, false),
    clip_def("EquipTwoHandedBlade", SoundClips::EquipTwoHandedBlade, false),
    clip_def("EquipStaff", SoundClips::EquipStaff, false),
    clip_def("EquipClothing", SoundClips::EquipClothing, false),
    clip_def("EquipJewellery", SoundClips::EquipJewellery, false),
    clip_def("OpenBook", SoundClips::OpenBook, false),
    clip_def("SnoringByFire", SoundClips::SnoringByFire, false),
    clip_def("MaleGasp", SoundClips::MaleGasp, false),
    clip_def("FemaleGasp", SoundClips::FemaleGasp, false),
    clip_def("BlowingWindIntro", SoundClips::BlowingWindIntro, false),
    clip_def("AmbientRaining", SoundClips::AmbientRaining, false),
    clip_def("BretonMalePain1", SoundClips::BretonMalePain1, false),
    clip_def("BretonMalePain2", SoundClips::BretonMalePain2, false),
    clip_def("BretonMalePain3", SoundClips::BretonMalePain3, false),
    clip_def("RedguardMalePain1", SoundClips::RedguardMalePain1, false),
    clip_def("RedguardMalePain2", SoundClips::RedguardMalePain2, false),
    clip_def("RedguardMalePain3", SoundClips::RedguardMalePain3, false),
    clip_def("NordMalePain1", SoundClips::NordMalePain1, false),
    clip_def("NordMalePain2", SoundClips::NordMalePain2, false),
    clip_def("NordMalePain3", SoundClips::NordMalePain3, false),
    clip_def("DarkElfMalePain1", SoundClips::DarkElfMalePain1, false),
    clip_def("DarkElfMalePain2", SoundClips::DarkElfMalePain2, false),
    clip_def("DarkElfMalePain3", SoundClips::DarkElfMalePain3, false),
    clip_def("HighElfMalePain1", SoundClips::HighElfMalePain1, false),
    clip_def("HighElfMalePain2", SoundClips::HighElfMalePain2, false),
    clip_def("HighElfMalePain3", SoundClips::HighElfMalePain3, false),
    clip_def("WoodElfMalePain1", SoundClips::WoodElfMalePain1, false),
    clip_def("WoodElfMalePain2", SoundClips::WoodElfMalePain2, false),
    clip_def("WoodElfMalePain3", SoundClips::WoodElfMalePain3, false),
    clip_def("KhajiitMalePain1", SoundClips::KhajiitMalePain1, false),
    clip_def("KhajiitMalePain2", SoundClips::KhajiitMalePain2, false),
    clip_def("KhajiitMalePain3", SoundClips::KhajiitMalePain3, false),
    clip_def("ArgonianMalePain1", SoundClips::ArgonianMalePain1, false),
    clip_def("ArgonianMalePain2", SoundClips::ArgonianMalePain2, false),
    clip_def("EquipMaceOrHammer", SoundClips::EquipMaceOrHammer, false),
    clip_def("EquipFlail", SoundClips::EquipFlail, false),
    clip_def("EquipAxe", SoundClips::EquipAxe, false),
    clip_def("EquipBow", SoundClips::EquipBow, false),
    clip_def("EquipLeather", SoundClips::EquipLeather, false),
    clip_def("EquipChain", SoundClips::EquipChain, false),
    clip_def("EquipPlate", SoundClips::EquipPlate, false),
    clip_def("Burning", SoundClips::Burning, false),
    clip_def("LongBeep", SoundClips::LongBeep, false),
    clip_def("ArenaIronGolem", SoundClips::ArenaIronGolem, false),
    clip_def("SpookyHigh2", SoundClips::SpookyHigh2, false),
    clip_def("KhajiitFemalePain3", SoundClips::KhajiitFemalePain3, false),
    clip_def("ArgonianFemalePain1", SoundClips::ArgonianFemalePain1, false),
    clip_def("ArgonianFemalePain2", SoundClips::ArgonianFemalePain2, false),
    clip_def("ArgonianFemalePain3", SoundClips::ArgonianFemalePain3, false),
    clip_def("Parry1", SoundClips::Parry1, false),
    clip_def("Parry2", SoundClips::Parry2, false),
    clip_def("Parry3", SoundClips::Parry3, false),
    clip_def("Parry4", SoundClips::Parry4, false),
    clip_def("Parry5", SoundClips::Parry5, false),
    clip_def("Parry6", SoundClips::Parry6, false),
    clip_def("Parry7", SoundClips::Parry7, false),
    clip_def("Parry8", SoundClips::Parry8, false),
    clip_def("Parry9", SoundClips::Parry9, false),
    clip_def("BirdCall1", SoundClips::BirdCall1, false),
    clip_def("BirdCall2", SoundClips::BirdCall2, false),
    clip_def("WaterGentle", SoundClips::WaterGentle, false),
    clip_def("WaterRough", SoundClips::WaterRough, false),
    clip_def("AmbientPeople1", SoundClips::AmbientPeople1, false),
    clip_def("AmbientPeople2", SoundClips::AmbientPeople2, false),
    clip_def("AmbientPeople3", SoundClips::AmbientPeople3, false),
    clip_def("AmbientPeople4", SoundClips::AmbientPeople4, false),
    clip_def("AmbientPeople5", SoundClips::AmbientPeople5, false),
    clip_def("AmbientPeople6", SoundClips::AmbientPeople6, false),
    clip_def("AmbientPeople7", SoundClips::AmbientPeople7, false),
    clip_def("AmbientPeople8", SoundClips::AmbientPeople8, false),
    clip_def("AmbientPeople9", SoundClips::AmbientPeople9, false),
    clip_def("AmbientPeople10", SoundClips::AmbientPeople10, false),
    clip_def("Vengeance", SoundClips::Vengeance, false),
    clip_def("Halt", SoundClips::Halt, false),
    clip_def("Halt2", SoundClips::Halt2, false),
    clip_def("Groan", SoundClips::Groan, false),
];

/// Number of DFU catalogue members (duplicates included).
pub const SOUND_CLIP_COUNT: usize = SOUND_CLIP_CATALOG.len();

/// Resolves a DFU member name (verbatim, case-sensitive) to its catalogue entry.
pub fn sound_clip_by_name(name: &str) -> Option<&'static SoundClipDef> {
    SOUND_CLIP_CATALOG.iter().find(|def| def.name == name)
}

/// The measured retail goldens (`goldens/sound-clips-375.tsv`), embedded at
/// compile time: the binary carries the measurement, never the retail bytes
/// (INV-L99-6). Regenerated and verified by
/// `scripts/audio/derive-snd-goldens.py`.
pub const SOUND_CLIP_GOLDENS_TSV: &str = include_str!("../goldens/sound-clips-375.tsv");

/// Number of golden rows (one per DFU catalogue member).
pub const SOUND_CLIP_GOLDEN_ROWS: usize = 375;

/// `expect` column of a golden row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldenExpect {
    /// A PCM payload was measured for this directory index.
    Pcm,
    /// The directory entry exists but carries 0 bytes (`Invalid = 5`).
    Empty,
    /// No directory entry (`None = -1`, DFU's "no clip" sentinel).
    None,
}

/// One golden row: the DFU member, its discriminant and the measured payload
/// of the retail entry at that directory index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenClip {
    /// DFU member name, verbatim.
    pub name: String,
    /// DFU discriminant (= directory index, or -1 for the sentinel).
    pub discriminant: i32,
    /// Which measurement the row records.
    pub expect: GoldenExpect,
    /// Directory `RecordID` of the entry (`None` for the `None` sentinel).
    pub record_id: Option<u32>,
    /// Payload length in bytes (= 8-bit mono samples).
    pub samples: usize,
    /// FNV-1a 64 over the payload bytes (`None` for `empty`/`none` rows).
    pub fnv1a64: Option<u64>,
    /// First payload byte (`None` for `empty`/`none` rows).
    pub first: Option<u8>,
    /// Last payload byte (`None` for `empty`/`none` rows).
    pub last: Option<u8>,
}

/// Parses the embedded golden file (`goldens/sound-clips-375.tsv`).
///
/// The file is a measured artifact, so this reader is strict: 8 tab-separated
/// columns, a valid `expect`, a well-formed row shape, unique names and exactly
/// [`SOUND_CLIP_GOLDEN_ROWS`] rows. Any violation is an error, never a guess.
pub fn golden_clips() -> Result<Vec<GoldenClip>, String> {
    parse_golden_set(SOUND_CLIP_GOLDENS_TSV)
}

/// Parses golden rows and enforces the catalogue row count.
fn parse_golden_set(text: &str) -> Result<Vec<GoldenClip>, String> {
    let rows = parse_goldens(text)?;
    if rows.len() != SOUND_CLIP_GOLDEN_ROWS {
        return Err(format!(
            "golden rows: expected {SOUND_CLIP_GOLDEN_ROWS}, found {}",
            rows.len()
        ));
    }
    Ok(rows)
}

/// Parses the rows of a golden file (`#` header lines are ignored).
fn parse_goldens(text: &str) -> Result<Vec<GoldenClip>, String> {
    let mut rows = Vec::new();
    let mut names = std::collections::HashSet::new();
    for (offset, raw) in text.lines().enumerate() {
        let line_no = offset + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with('#') {
            continue;
        }
        if line.is_empty() {
            return Err(format!("line {line_no}: empty row"));
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() != 8 {
            return Err(format!(
                "line {line_no}: expected 8 columns, found {}",
                columns.len()
            ));
        }
        let name = columns[0];
        if name.is_empty() {
            return Err(format!("line {line_no}: empty name"));
        }
        if !names.insert(name) {
            return Err(format!("line {line_no}: duplicate name {name}"));
        }
        let discriminant: i32 = columns[1]
            .parse()
            .map_err(|_| format!("line {line_no}: bad discriminant {:?}", columns[1]))?;
        let expect = match columns[2] {
            "pcm" => GoldenExpect::Pcm,
            "empty" => GoldenExpect::Empty,
            "none" => GoldenExpect::None,
            other => return Err(format!("line {line_no}: bad expect {other:?}")),
        };
        let record_id = optional_column(columns[3], line_no, "record_id", |cell| {
            cell.parse::<u32>().ok()
        })?;
        let samples: usize = columns[4]
            .parse()
            .map_err(|_| format!("line {line_no}: bad samples {:?}", columns[4]))?;
        let fnv1a64 = optional_column(columns[5], line_no, "fnv1a64", parse_fnv1a64_hex)?;
        let first = optional_column(columns[6], line_no, "first", |cell| cell.parse::<u8>().ok())?;
        let last = optional_column(columns[7], line_no, "last", |cell| cell.parse::<u8>().ok())?;
        let shape_ok = match expect {
            GoldenExpect::Pcm => {
                discriminant >= 0
                    && record_id.is_some()
                    && samples > 0
                    && fnv1a64.is_some()
                    && first.is_some()
                    && last.is_some()
            }
            GoldenExpect::Empty => {
                discriminant >= 0
                    && record_id.is_some()
                    && samples == 0
                    && fnv1a64.is_none()
                    && first.is_none()
                    && last.is_none()
            }
            GoldenExpect::None => {
                discriminant < 0
                    && record_id.is_none()
                    && samples == 0
                    && fnv1a64.is_none()
                    && first.is_none()
                    && last.is_none()
            }
        };
        if !shape_ok {
            return Err(format!(
                "line {line_no}: row shape does not match expect {expect:?}"
            ));
        }
        rows.push(GoldenClip {
            name: name.to_string(),
            discriminant,
            expect,
            record_id,
            samples,
            fnv1a64,
            first,
            last,
        });
    }
    Ok(rows)
}

/// Reads a column that may be `-` (not applicable).
fn optional_column<T>(
    cell: &str,
    line_no: usize,
    field: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>, String> {
    if cell == "-" {
        return Ok(None);
    }
    parse(cell)
        .map(Some)
        .ok_or_else(|| format!("line {line_no}: bad {field} {cell:?}"))
}

/// Parses the `fnv1a64` column: exactly 16 lowercase hex digits.
fn parse_fnv1a64_hex(cell: &str) -> Option<u64> {
    if cell.len() != 16
        || !cell
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    u64::from_str_radix(cell, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal synthetic `DAGGER.SND` (BSA 0x0200) with two test
    /// PCM clips — 0 retail bytes (INV-L99-6).
    fn build_snd(records: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(records.len() as u16).to_le_bytes());
        out.extend_from_slice(&0x0200u16.to_le_bytes());
        for (_, pcm) in records {
            out.extend_from_slice(pcm);
        }
        for (id, pcm) in records {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        }
        out
    }

    #[test]
    fn parses_numbered_directory() {
        let snd = build_snd(&[
            (3, vec![0x80, 0x81, 0x82]),
            (91, vec![0x7f, 0x7e]),
        ]);
        let bank = SoundBank::parse(&snd).expect("parse");
        assert_eq!(bank.len(), 2);
        assert_eq!(bank.record_id_at(0), Some(3));
        assert_eq!(bank.record_id_at(1), Some(91));
    }

    /// Two-record bank in order [(3, …), (91, …)]: position 0 carries record 3
    /// and position 1 record 91. Decoding is by position and preserves each
    /// entry's own `record_id` on the clip.
    #[test]
    fn decodes_by_directory_index() {
        let snd = build_snd(&[(3, vec![0x80, 0x81, 0x82]), (91, vec![0x7f, 0x7e])]);
        let bank = SoundBank::parse(&snd).expect("parse");
        let arrow = bank.decode(&snd, 0).expect("decode index 0");
        assert_eq!(arrow.index, 0);
        assert_eq!(arrow.record_id, 3);
        assert_eq!(arrow.samples, 3);
        assert_eq!(arrow.pcm, vec![0x80, 0x81, 0x82]);
        let fall = bank.decode(&snd, 1).expect("decode index 1");
        assert_eq!(fall.index, 1);
        assert_eq!(fall.record_id, 91);
        assert_eq!(fall.samples, 2);
    }

    /// **Index dispatch, not id dispatch.** The directory is deliberately
    /// shuffled so the entry at position 0 carries `RecordID` 91: `decode(0)`
    /// must return that entry, proving the position is the key (DFU
    /// `SoundReader.cs:59-101` → `SndFile.cs:158`). Under the former
    /// `RecordID`-keyed `decode`, this call looked up record 0 and failed.
    #[test]
    fn index_dispatch_ignores_record_id_order() {
        let snd = build_snd(&[(91, vec![0x7f, 0x7e]), (3, vec![0x80, 0x81, 0x82])]);
        let bank = SoundBank::parse(&snd).expect("parse");
        assert_eq!(bank.record_id_at(0), Some(91));
        let first = bank.decode(&snd, 0).expect("decode index 0");
        assert_eq!(
            (first.index, first.record_id, first.pcm.clone()),
            (0, 91, vec![0x7f, 0x7e]),
            "index 0 must return the entry at position 0 (record 91), not record 0"
        );
        let second = bank.decode(&snd, 1).expect("decode index 1");
        assert_eq!((second.index, second.record_id), (1, 3));
        assert_eq!(second.pcm, vec![0x80, 0x81, 0x82]);
        // The enum discriminants are positions by definition.
        assert_eq!(SoundClips::ArrowShoot.index(), Some(3));
        assert_eq!(SoundClips::FallDamage.index(), Some(91));
    }

    /// The explicit id lookup keeps working — and returns the entry at its own
    /// position, which in the shuffled bank differs from the position-keyed one.
    #[test]
    fn decode_by_record_id_is_explicit_and_position_aware() {
        let snd = build_snd(&[(91, vec![0x7f, 0x7e]), (3, vec![0x80, 0x81, 0x82])]);
        let bank = SoundBank::parse(&snd).expect("parse");
        let by_id = bank.decode_by_record_id(&snd, 3).expect("record 3");
        assert_eq!((by_id.index, by_id.record_id), (1, 3));
        assert_eq!(bank.index_of_record_id(3), Some(1));
        assert_eq!(bank.index_of_record_id(91), Some(0));
        assert_eq!(bank.index_of_record_id(4), None);
        assert_ne!(
            (bank.decode(&snd, 0).expect("index 0").record_id),
            by_id.record_id,
            "index dispatch and id dispatch select different entries in this bank"
        );
    }

    /// `index >= len` is a distinct, typed failure — never a fallback to a
    /// `RecordID` lookup (91 is a valid `RecordID` in this bank, yet index 91
    /// must still be out of range).
    #[test]
    fn out_of_range_index_errors_without_id_fallback() {
        let snd = build_snd(&[(3, vec![0x80]), (91, vec![0x7f])]);
        let bank = SoundBank::parse(&snd).expect("parse");
        let err = bank.decode(&snd, 2).expect_err("index 2 of 2");
        assert_eq!(err, SndError::IndexOutOfRange { index: 2, count: 2 });
        let err = bank.decode(&snd, 91).expect_err("index 91 of 2");
        assert_eq!(err, SndError::IndexOutOfRange { index: 91, count: 2 });
        let err = bank.decode_by_record_id(&snd, 7).expect_err("record 7 absent");
        assert_eq!(err, SndError::MissingRecordId { record_id: 7, count: 2 });
    }

    /// An existing but empty record is `EmptyClip`, kept distinct from the
    /// range error (DFU `Invalid = 5` is a present index with no data).
    #[test]
    fn empty_record_is_a_distinct_error() {
        let snd = build_snd(&[(5, Vec::new()), (3, vec![0x80])]);
        let bank = SoundBank::parse(&snd).expect("parse");
        let err = bank.decode(&snd, 0).expect_err("empty record");
        assert_eq!(err, SndError::EmptyClip { index: 0 });
        assert_eq!(bank.decode(&snd, 1).expect("index 1").pcm, vec![0x80]);
    }

    #[test]
    fn rejects_wrong_directory_type() {
        // 0x0100 (named) is not an effect bank.
        let mut snd = Vec::new();
        snd.extend_from_slice(&1u16.to_le_bytes());
        snd.extend_from_slice(&0x0100u16.to_le_bytes());
        snd.extend_from_slice(&[1, 2, 3]); // data (3 bytes) before the directory
        snd.extend_from_slice(b"FOO.WAV\0\0\0\0\0\0\0"); // 14-byte name
        snd.extend_from_slice(&3u32.to_le_bytes()); // size
        let err = SoundBank::parse(&snd).expect_err("wrong type");
        assert!(matches!(err, SndError::NotNumbered));
    }

    #[test]
    fn wave_header_is_44_bytes() {
        let clip = SoundClip { index: 3, record_id: 6, samples: 2, pcm: vec![0x80, 0x7f] };
        let w = clip.wave_header();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(&w[12..16], b"fmt ");
        assert_eq!(&w[36..40], b"data");
        assert_eq!(w.len(), 44 + 2);
        // data length field == 2.
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 2);
    }


    // ---------- DFU catalogue: 375 members / 374 variants ----------

    #[test]
    fn catalog_has_375_members_in_dfu_order() {
        assert_eq!(SOUND_CLIP_CATALOG.len(), 375);
        assert_eq!(SOUND_CLIP_COUNT, 375);
        assert_eq!(SOUND_CLIP_CATALOG[0].name, "None");
        assert_eq!(SOUND_CLIP_CATALOG[0].clip, SoundClips::None);
        let last = SOUND_CLIP_CATALOG[SOUND_CLIP_CATALOG.len() - 1];
        assert_eq!(last.name, "Groan");
        assert_eq!(last.clip, SoundClips::Groan);
        let position = |name: &str| {
            SOUND_CLIP_CATALOG
                .iter()
                .position(|def| def.name == name)
                .unwrap_or_else(|| panic!("catalogue without {name}"))
        };
        // DFU source order, not numeric order: the Gargoyle attack (180) is
        // declared after move/bark (181/182), and the Orc Shaman attack (138)
        // is declared at its own position among the 178-179 entries.
        assert!(position("EnemyOrcShamanBark") < position("EnemyOrcShamanAttack"));
        assert!(position("EnemyOrcShamanAttack") < position("EnemyGargoyleMove"));
        assert!(position("EnemyGargoyleMove") < position("EnemyGargoyleAttack"));
        assert!(position("EnemyOrcAttack") < position("EnemyOrcShamanAttack"));
    }

    #[test]
    fn catalog_variants_are_unique_and_named() {
        let mut names = std::collections::HashSet::new();
        let mut values = std::collections::HashSet::new();
        let mut aliases = 0;
        for def in SOUND_CLIP_CATALOG {
            assert!(names.insert(def.name), "duplicate catalogue name {}", def.name);
            let value = def.clip as i32;
            if def.alias {
                aliases += 1;
                assert_eq!(value, 138, "unexpected alias {}", def.name);
                assert_eq!(def.name, "EnemyOrcShamanAttack");
            } else {
                assert!(values.insert(value), "duplicate variant value {value}");
                assert_eq!(def.clip.name(), def.name);
            }
        }
        assert_eq!(values.len(), 374, "374 distinct variants");
        assert_eq!(aliases, 1, "exactly one flagged alias (value 138)");
        assert_eq!(names.len(), 375, "375 DFU member names");
        // Both names of the duplicated discriminant resolve to the same variant.
        let attack = sound_clip_by_name("EnemyOrcAttack").map(|def| def.clip);
        let shaman = sound_clip_by_name("EnemyOrcShamanAttack").map(|def| def.clip);
        assert_eq!(attack, Some(SoundClips::EnemyOrcAttack));
        assert_eq!(shaman, attack);
        assert_eq!(sound_clip_by_name("NoSuchClip"), None);
        assert_eq!(SoundClips::EnemyOrcAttack.definitions().count(), 2);
        assert_eq!(SoundClips::ArrowShoot.definitions().count(), 1);
    }

    #[test]
    fn catalog_spot_anchors_match_dfu() {
        assert_eq!(SoundClips::None as i32, -1);
        assert_eq!(SoundClips::SpookyHigh as i32, 0);
        assert_eq!(SoundClips::ArrowShoot as i32, 3);
        assert_eq!(SoundClips::Invalid as i32, 5);
        assert_eq!(SoundClips::FallDamage as i32, 91);
        assert_eq!(SoundClips::SwingLowPitch as i32, 105);
        assert_eq!(SoundClips::Hit1 as i32, 108);
        assert_eq!(SoundClips::EnemyOrcAttack as i32, 138);
        assert_eq!(SoundClips::Groan as i32, 458);
        assert_eq!(SoundClips::None.name(), "None");
        assert_eq!(SoundClips::Invalid.name(), "Invalid");
        assert_eq!(SoundClips::EnemyOrcAttack.name(), "EnemyOrcAttack");
        assert_eq!(SoundClips::Groan.name(), "Groan");
    }

    #[test]
    fn index_is_the_discriminant_or_none() {
        assert_eq!(SoundClips::None.index(), None);
        assert_eq!(SoundClips::ArrowShoot.index(), Some(3));
        assert_eq!(SoundClips::Invalid.index(), Some(5));
        assert_eq!(SoundClips::FallDamage.index(), Some(91));
        assert_eq!(SoundClips::Groan.index(), Some(458));
        for def in SOUND_CLIP_CATALOG {
            let discriminant = def.clip as i32;
            let expected = if discriminant < 0 {
                None
            } else {
                Some(discriminant as usize)
            };
            assert_eq!(def.clip.index(), expected, "{} = {discriminant}", def.name);
        }
    }

    // ---------- measured retail goldens (hermetic: TSV embedded) ----------

    /// A well-formed row for the rejection tests. The parser validates column
    /// form only: it never recomputes the digest (that is the bank's job, done
    /// by `scripts/audio/derive-snd-goldens.py`).
    const SAMPLE_ROW: &str = "ArrowShoot\t3\tpcm\t6\t2\taa272f372d2aa6fc\t129\t127";

    #[test]
    fn goldens_have_375_rows() {
        let rows = golden_clips().expect("embedded goldens parse");
        assert_eq!(rows.len(), 375);
        assert_eq!(rows.len(), SOUND_CLIP_CATALOG.len());
    }

    #[test]
    fn golden_header_records_the_measurement() {
        let mut lines = SOUND_CLIP_GOLDENS_TSV.lines();
        let first = lines.next().expect("header line");
        assert!(first.contains("measured from retail"), "header: {first}");
        let record = lines.next().expect("measurement record");
        assert!(record.contains("snd_sha256="), "record: {record}");
        assert!(record.contains("records=459"), "record: {record}");
        assert!(record.contains("bsa=0x0200"), "record: {record}");
        assert!(
            record.contains("fnv1a64(basis 0xcbf29ce484222325, prime 0x100000001b3)"),
            "record: {record}"
        );
        let sha = record
            .split("snd_sha256=")
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .expect("sha256 field");
        assert_eq!(sha.len(), 64);
        assert!(sha.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let columns = lines.next().expect("column line");
        assert_eq!(
            columns,
            "# name\tdiscriminant\texpect\trecord_id\tsamples\tfnv1a64\tfirst\tlast"
        );
    }

    #[test]
    fn golden_rows_match_catalog_names_and_discriminants() {
        let rows = golden_clips().expect("embedded goldens parse");
        assert_eq!(rows.len(), SOUND_CLIP_CATALOG.len());
        for (row, def) in rows.iter().zip(SOUND_CLIP_CATALOG) {
            assert_eq!(row.name, def.name);
            assert_eq!(row.discriminant, def.clip as i32, "{}", row.name);
            assert_eq!(
                sound_clip_by_name(&row.name).map(|found| found.clip),
                Some(def.clip),
                "{} does not resolve back to the catalogue",
                row.name
            );
        }
        let aliased: Vec<_> = rows
            .iter()
            .filter(|row| row.discriminant == 138)
            .collect();
        assert_eq!(aliased.len(), 2);
        assert_eq!(aliased[0].name, "EnemyOrcAttack");
        assert_eq!(aliased[1].name, "EnemyOrcShamanAttack");
        // Both names measure the same directory entry.
        assert_eq!(aliased[0].record_id, aliased[1].record_id);
        assert_eq!(aliased[0].fnv1a64, aliased[1].fnv1a64);
    }

    #[test]
    fn golden_counts_are_pcm_373_empty_1_none_1() {
        let rows = golden_clips().expect("embedded goldens parse");
        let count = |expect| rows.iter().filter(|row| row.expect == expect).count();
        assert_eq!(count(GoldenExpect::Pcm), 373);
        assert_eq!(count(GoldenExpect::Empty), 1);
        assert_eq!(count(GoldenExpect::None), 1);
        let empty = rows
            .iter()
            .find(|row| row.expect == GoldenExpect::Empty)
            .expect("one empty row");
        assert_eq!(
            (empty.name.as_str(), empty.discriminant, empty.record_id, empty.samples, empty.fnv1a64),
            ("Invalid", 5, Some(8), 0, None)
        );
        let none = rows
            .iter()
            .find(|row| row.expect == GoldenExpect::None)
            .expect("one none row");
        assert_eq!(
            (none.name.as_str(), none.discriminant, none.record_id, none.samples),
            ("None", -1, None, 0)
        );
    }

    #[test]
    fn golden_anchors_match_the_measured_clips() {
        let rows = golden_clips().expect("embedded goldens parse");
        let find = |name: &str| {
            rows.iter()
                .find(|row| row.name == name)
                .unwrap_or_else(|| panic!("golden without {name}"))
        };
        // Same measurement the AUDIO-001 probe carries for its two mapped clips.
        let arrow = find("ArrowShoot");
        assert_eq!(
            (arrow.discriminant, arrow.record_id, arrow.samples),
            (3, Some(6), 2259)
        );
        assert_eq!(arrow.fnv1a64, Some(0xaa27_2f37_2d2a_a6fc));
        assert_eq!((arrow.first, arrow.last), (Some(129), Some(127)));
        let fall = find("FallDamage");
        assert_eq!(
            (fall.discriminant, fall.record_id, fall.samples),
            (91, Some(359), 2226)
        );
        assert_eq!(fall.fnv1a64, Some(0xbb9c_ce9c_0f2e_d458));
        assert_eq!((fall.first, fall.last), (Some(126), Some(129)));
        for row in rows.iter().filter(|row| row.expect == GoldenExpect::Pcm) {
            assert!(row.samples > 0, "{} has no samples", row.name);
            assert!(row.fnv1a64.is_some() && row.first.is_some() && row.last.is_some());
            assert_ne!(
                row.fnv1a64,
                Some(0xcbf2_9ce4_8422_2325),
                "{} hashes an empty payload",
                row.name
            );
        }
    }

    #[test]
    fn golden_parser_round_trips_and_is_deterministic() {
        let first = golden_clips().expect("parse 1");
        let second = golden_clips().expect("parse 2");
        assert_eq!(first, second, "parsing is deterministic");
        for row in &first {
            let def = sound_clip_by_name(&row.name)
                .unwrap_or_else(|| panic!("golden row {} is not in the catalogue", row.name));
            assert_eq!(def.clip as i32, row.discriminant);
            match row.expect {
                GoldenExpect::None => {
                    assert_eq!(row.discriminant, -1);
                    assert_eq!(def.clip.index(), None);
                }
                GoldenExpect::Pcm | GoldenExpect::Empty => {
                    assert_eq!(def.clip.index(), Some(row.discriminant as usize));
                }
            }
        }
    }

    #[test]
    fn golden_parser_rejects_malformed_rows() {
        assert!(parse_goldens(SAMPLE_ROW).is_ok(), "the sample row must parse");
        assert!(
            parse_goldens("ArrowShoot\t3\tpcm\t6\t2\taa272f372d2aa6fc\t129").is_err(),
            "7 columns"
        );
        assert!(
            parse_goldens("ArrowShoot\t3\tpcm\t6\t2\taa272f372d2aa6fc\t129\t127\textra").is_err(),
            "9 columns"
        );
        assert!(
            parse_goldens("\t3\tpcm\t6\t2\taa272f372d2aa6fc\t129\t127").is_err(),
            "empty name"
        );
        assert!(
            parse_goldens("ArrowShoot\tnine\tpcm\t6\t2\taa272f372d2aa6fc\t129\t127").is_err(),
            "bad discriminant"
        );
        assert!(
            parse_goldens("ArrowShoot\t3\twav\t6\t2\taa272f372d2aa6fc\t129\t127").is_err(),
            "bad expect"
        );
        assert!(
            parse_goldens("ArrowShoot\t3\tpcm\t6\t2\taa272f372d2aa6fz\t129\t127").is_err(),
            "bad hex"
        );
        assert!(
            parse_goldens("ArrowShoot\t3\tpcm\t6\t2\tAA272F372D2AA6FC\t129\t127").is_err(),
            "uppercase hex"
        );
        assert!(
            parse_goldens("ArrowShoot\t3\tpcm\t6\t2\taa272f372d2aa6f\t129\t127").is_err(),
            "15 hex digits"
        );
        assert!(
            parse_goldens("ArrowShoot\t3\tpcm\t6\t0\taa272f372d2aa6fc\t129\t127").is_err(),
            "pcm row without samples"
        );
        assert!(
            parse_goldens("ArrowShoot\t3\tpcm\t6\t2\t-\t129\t127").is_err(),
            "pcm row without digest"
        );
        assert!(
            parse_goldens("None\t-1\tpcm\t6\t2\taa272f372d2aa6fc\t129\t127").is_err(),
            "pcm row with the sentinel discriminant"
        );
        assert!(
            parse_goldens("None\t-1\tnone\t6\t0\t-\t-\t-").is_err(),
            "none row with a record id"
        );
        assert!(
            parse_goldens("Invalid\t5\tempty\t8\t3\t-\t-\t-").is_err(),
            "empty row with samples"
        );
        assert!(
            parse_goldens(
                "ArrowShoot\t3\tpcm\t6\t2\taa272f372d2aa6fc\t129\t127\n\
                 ArrowShoot\t4\tpcm\t7\t2\taa272f372d2aa6fc\t129\t127"
            )
            .is_err(),
            "duplicate name"
        );
        assert!(
            parse_goldens(&format!("{SAMPLE_ROW}\n\n")).is_err(),
            "blank row"
        );
    }

    #[test]
    fn golden_parser_rejects_non_375_sets() {
        let one = format!(
            "# name\tdiscriminant\texpect\trecord_id\tsamples\tfnv1a64\tfirst\tlast\n{SAMPLE_ROW}\n"
        );
        let error = parse_golden_set(&one).expect_err("1 row is not 375");
        assert!(error.contains("375"), "error: {error}");
        assert_eq!(
            parse_goldens(&one).expect("row parser is length-agnostic").len(),
            1
        );
    }
}
