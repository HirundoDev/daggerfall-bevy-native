//! Reachability proof for the presentation vocabulary (CR-4f7a21c8): a side
//! that writes only `use daggerfall_game_core::prelude::*;` can name
//! `WeatherType` and `HourBand` — no `path` dependency on `daggerfall-nervous`,
//! no second definition of either type.

use daggerfall_game_core::prelude::*;

/// The seven weather states, in the source enum order, with their labels.
#[test]
fn weather_type_is_reachable_from_the_prelude_alone() {
    const EXPECTED: [&str; 7] = ["Sunny", "Cloudy", "Overcast", "Fog", "Rain", "Thunder", "Snow"];

    assert_eq!(WeatherType::ALL.len(), 7);
    assert_eq!(WeatherType::ALL.len(), EXPECTED.len());

    for (weather, expected) in WeatherType::ALL.iter().zip(EXPECTED) {
        assert_eq!(weather.label(), expected);
        assert!(!weather.label().is_empty());
    }
}

/// The day/night band: both arms named, both labels exact.
#[test]
fn hour_band_is_reachable_from_the_prelude_alone() {
    assert_eq!(HourBand::Night.label(), "Night");
    assert_eq!(HourBand::Day.label(), "Day");
    assert_ne!(HourBand::Night, HourBand::Day);
}

/// The vocabulary travels in the two events the doc table cites.
#[test]
fn presentation_vocabulary_travels_in_game_events() {
    let weather = GameEvent::WeatherChanged(WeatherType::Thunder);
    let band = GameEvent::DayNightChanged(HourBand::Night);

    assert_eq!(weather.label(), "WeatherChanged");
    assert_eq!(band.label(), "DayNightChanged");
}
