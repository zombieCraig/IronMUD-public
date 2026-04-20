//! Time, weather, and environmental types for IronMUD

use serde::{Deserialize, Serialize};

// Game time constants
pub const REAL_MINUTES_PER_GAME_HOUR: u64 = 2; // 2 real minutes = 1 game hour
pub const GAME_HOURS_PER_DAY: u64 = 24;
pub const GAME_DAYS_PER_MONTH: u64 = 30;
pub const GAME_MONTHS_PER_YEAR: u64 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    Spring, // Months 3, 4, 5
    Summer, // Months 6, 7, 8
    Autumn, // Months 9, 10, 11
    Winter, // Months 12, 1, 2
}

impl Default for Season {
    fn default() -> Self {
        Season::Spring
    }
}

impl std::fmt::Display for Season {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Season::Spring => write!(f, "Spring"),
            Season::Summer => write!(f, "Summer"),
            Season::Autumn => write!(f, "Autumn"),
            Season::Winter => write!(f, "Winter"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Dawn,      // 5-6
    Morning,   // 7-11
    Noon,      // 12-13
    Afternoon, // 14-16
    Dusk,      // 17-18
    Evening,   // 19-21
    Night,     // 22-4
}

impl Default for TimeOfDay {
    fn default() -> Self {
        TimeOfDay::Morning
    }
}

impl std::fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeOfDay::Dawn => write!(f, "dawn"),
            TimeOfDay::Morning => write!(f, "morning"),
            TimeOfDay::Noon => write!(f, "high noon"),
            TimeOfDay::Afternoon => write!(f, "afternoon"),
            TimeOfDay::Dusk => write!(f, "dusk"),
            TimeOfDay::Evening => write!(f, "evening"),
            TimeOfDay::Night => write!(f, "night"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherCondition {
    Clear,
    PartlyCloudy,
    Cloudy,
    Overcast,
    LightRain,
    Rain,
    HeavyRain,
    Thunderstorm,
    LightSnow,
    Snow,
    Blizzard,
    Fog,
}

impl Default for WeatherCondition {
    fn default() -> Self {
        WeatherCondition::Clear
    }
}

impl std::fmt::Display for WeatherCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WeatherCondition::Clear => write!(f, "clear and sunny"),
            WeatherCondition::PartlyCloudy => write!(f, "partly cloudy"),
            WeatherCondition::Cloudy => write!(f, "cloudy"),
            WeatherCondition::Overcast => write!(f, "overcast and grey"),
            WeatherCondition::LightRain => write!(f, "lightly raining"),
            WeatherCondition::Rain => write!(f, "raining"),
            WeatherCondition::HeavyRain => write!(f, "raining heavily"),
            WeatherCondition::Thunderstorm => write!(f, "stormy with thunder"),
            WeatherCondition::LightSnow => write!(f, "lightly snowing"),
            WeatherCondition::Snow => write!(f, "snowing"),
            WeatherCondition::Blizzard => write!(f, "a blizzard"),
            WeatherCondition::Fog => write!(f, "foggy"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureCategory {
    Freezing,   // < 0C
    Cold,       // 0-9C
    Cool,       // 10-14C
    Mild,       // 15-19C
    Warm,       // 20-24C
    Hot,        // 25-34C
    Sweltering, // >= 35C
}

impl Default for TemperatureCategory {
    fn default() -> Self {
        TemperatureCategory::Mild
    }
}

impl std::fmt::Display for TemperatureCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemperatureCategory::Freezing => write!(f, "freezing cold"),
            TemperatureCategory::Cold => write!(f, "cold"),
            TemperatureCategory::Cool => write!(f, "cool"),
            TemperatureCategory::Mild => write!(f, "mild"),
            TemperatureCategory::Warm => write!(f, "warm"),
            TemperatureCategory::Hot => write!(f, "hot"),
            TemperatureCategory::Sweltering => write!(f, "sweltering"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameTime {
    /// Unix timestamp when game time started (server first run)
    pub epoch_start: i64,
    /// Current game hour (0-23)
    pub hour: u8,
    /// Current game day (1-30)
    pub day: u8,
    /// Current game month (1-12)
    pub month: u8,
    /// Current game year (starting at 1)
    pub year: u32,
    /// Current weather condition
    pub weather: WeatherCondition,
    /// Base temperature in Celsius (modified by season/time/weather)
    pub base_temperature: i32,
    /// Timestamp of last weather change
    pub last_weather_change: i64,
    /// Timestamp of last time tick
    pub last_time_tick: i64,
}

impl Default for GameTime {
    fn default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        GameTime {
            epoch_start: now,
            hour: 8, // Start at 8 AM
            day: 1,
            month: 3, // Start in spring (Month 3)
            year: 1,
            weather: WeatherCondition::Clear,
            base_temperature: 18, // Mild spring temperature
            last_weather_change: now,
            last_time_tick: now,
        }
    }
}

impl GameTime {
    pub fn get_season(&self) -> Season {
        match self.month {
            3 | 4 | 5 => Season::Spring,
            6 | 7 | 8 => Season::Summer,
            9 | 10 | 11 => Season::Autumn,
            12 | 1 | 2 => Season::Winter,
            _ => Season::Spring,
        }
    }

    pub fn get_time_of_day(&self) -> TimeOfDay {
        match self.hour {
            5..=6 => TimeOfDay::Dawn,
            7..=11 => TimeOfDay::Morning,
            12..=13 => TimeOfDay::Noon,
            14..=16 => TimeOfDay::Afternoon,
            17..=18 => TimeOfDay::Dusk,
            19..=21 => TimeOfDay::Evening,
            _ => TimeOfDay::Night, // 22-4
        }
    }

    pub fn get_temperature_category(&self) -> TemperatureCategory {
        let effective_temp = self.calculate_effective_temperature();
        match effective_temp {
            t if t < 0 => TemperatureCategory::Freezing,
            t if t < 10 => TemperatureCategory::Cold,
            t if t < 15 => TemperatureCategory::Cool,
            t if t < 20 => TemperatureCategory::Mild,
            t if t < 25 => TemperatureCategory::Warm,
            t if t < 35 => TemperatureCategory::Hot,
            _ => TemperatureCategory::Sweltering,
        }
    }

    pub fn calculate_effective_temperature(&self) -> i32 {
        let mut temp = self.base_temperature;

        // Season modifier
        temp += match self.get_season() {
            Season::Winter => -15,
            Season::Autumn => -5,
            Season::Spring => 0,
            Season::Summer => 10,
        };

        // Time of day modifier
        temp += match self.get_time_of_day() {
            TimeOfDay::Night => -8,
            TimeOfDay::Dawn => -5,
            TimeOfDay::Morning => -2,
            TimeOfDay::Noon => 3,
            TimeOfDay::Afternoon => 2,
            TimeOfDay::Dusk => -1,
            TimeOfDay::Evening => -4,
        };

        // Weather modifier
        temp += match self.weather {
            WeatherCondition::Clear => 2,
            WeatherCondition::PartlyCloudy => 0,
            WeatherCondition::Cloudy => -2,
            WeatherCondition::Overcast => -4,
            WeatherCondition::LightRain => -3,
            WeatherCondition::Rain => -5,
            WeatherCondition::HeavyRain => -7,
            WeatherCondition::Thunderstorm => -8,
            WeatherCondition::LightSnow => -5,
            WeatherCondition::Snow => -8,
            WeatherCondition::Blizzard => -15,
            WeatherCondition::Fog => -2,
        };

        temp
    }

    pub fn is_daytime(&self) -> bool {
        self.hour >= 6 && self.hour < 20
    }

    /// Advance time by one hour, handling day/month/year rollovers
    pub fn advance_hour(&mut self) {
        self.hour += 1;
        if self.hour >= 24 {
            self.hour = 0;
            self.day += 1;
            if self.day > GAME_DAYS_PER_MONTH as u8 {
                self.day = 1;
                self.month += 1;
                if self.month > GAME_MONTHS_PER_YEAR as u8 {
                    self.month = 1;
                    self.year += 1;
                }
            }
        }
    }
}
