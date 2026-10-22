//! Workout file parsing. Pure std — no external crates — so it can be tested anywhere.

pub mod csv;
pub mod fit;
pub mod labs;
pub mod pdf;
pub mod time;
pub mod xml;

/// One sample stream per field, aligned on `t` (seconds from start).
#[derive(Default, Debug, Clone)]
pub struct Streams {
    pub t: Vec<i64>,
    pub hr: Vec<Option<f64>>,
    pub dist: Vec<Option<f64>>,
    pub speed: Vec<Option<f64>>,
    pub alt: Vec<Option<f64>>,
    pub cad: Vec<Option<f64>>,
    pub pwr: Vec<Option<f64>>,
    pub lat: Vec<Option<f64>>,
    pub lng: Vec<Option<f64>>,
}

impl Streams {
    pub fn len(&self) -> usize {
        self.t.len()
    }
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }
    pub fn push(&mut self, t: i64, s: Sample) {
        self.t.push(t);
        self.hr.push(s.hr);
        self.dist.push(s.dist);
        self.speed.push(s.speed);
        self.alt.push(s.alt);
        self.cad.push(s.cad);
        self.pwr.push(s.pwr);
        self.lat.push(s.lat);
        self.lng.push(s.lng);
    }
    pub fn has_gps(&self) -> bool {
        self.lat.iter().any(|v| v.is_some())
    }
    pub fn has_hr(&self) -> bool {
        self.hr.iter().any(|v| v.is_some())
    }

    /// Compact JSON: {"t":[..],"hr":[..],...}; fields with no data are left out.
    pub fn to_json(&self) -> String {
        fn num(v: f64, dec: usize) -> String {
            let s = format!("{v:.dec$}");
            if s.contains('.') {
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                s
            }
        }
        fn arr(out: &mut String, name: &str, v: &[Option<f64>], dec: usize) {
            if !v.iter().any(|x| x.is_some()) {
                return;
            }
            out.push_str(&format!(",\"{name}\":["));
            for (i, x) in v.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                match x {
                    Some(f) if f.is_finite() => out.push_str(&num(*f, dec)),
                    _ => out.push_str("null"),
                }
            }
            out.push(']');
        }
        let mut out = String::from("{\"t\":[");
        out.push_str(&self.t.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
        out.push(']');
        arr(&mut out, "hr", &self.hr, 0);
        arr(&mut out, "dist", &self.dist, 1);
        arr(&mut out, "speed", &self.speed, 3);
        arr(&mut out, "alt", &self.alt, 1);
        arr(&mut out, "cad", &self.cad, 0);
        arr(&mut out, "pwr", &self.pwr, 0);
        arr(&mut out, "lat", &self.lat, 6);
        arr(&mut out, "lng", &self.lng, 6);
        out.push('}');
        out
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Sample {
    pub hr: Option<f64>,
    pub dist: Option<f64>,
    pub speed: Option<f64>,
    pub alt: Option<f64>,
    pub cad: Option<f64>,
    pub pwr: Option<f64>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

#[derive(Default, Debug, Clone)]
pub struct SetRow {
    pub start_utc: Option<i64>,
    pub duration_s: Option<f64>,
    pub reps: Option<i64>,
    pub weight_kg: Option<f64>,
    /// "active" or "rest"
    pub kind: String,
    pub category: Option<i64>,
}

/// A workout read from a file or an API, before it is stored.
#[derive(Default, Debug, Clone)]
pub struct Parsed {
    /// run, walk, ride, ride_virtual, hike, swim, strength, workout, yoga, other
    pub sport: String,
    pub sport_label: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub start_utc: i64,
    pub elapsed_s: Option<f64>,
    pub moving_s: Option<f64>,
    pub distance_m: Option<f64>,
    pub elev_gain_m: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub avg_power: Option<f64>,
    pub calories: Option<f64>,
    pub relative_effort: Option<f64>,
    pub steps: Option<i64>,
    pub total_sets: Option<i64>,
    pub total_reps: Option<i64>,
    pub weight_lifted_kg: Option<f64>,
    pub gear: Option<String>,
    pub device: Option<String>,
    pub streams: Streams,
    pub sets: Vec<SetRow>,
}

impl Parsed {
    /// Fills summary fields that the file did not state, from the streams.
    pub fn fill_from_streams(&mut self) {
        let s = &self.streams;
        if s.is_empty() {
            return;
        }
        if self.elapsed_s.is_none() {
            self.elapsed_s = Some((s.t[s.len() - 1] - s.t[0]) as f64);
        }
        let hrs: Vec<f64> = s.hr.iter().flatten().copied().collect();
        if !hrs.is_empty() {
            if self.avg_hr.is_none() {
                self.avg_hr = Some(hrs.iter().sum::<f64>() / hrs.len() as f64);
            }
            if self.max_hr.is_none() {
                self.max_hr = hrs.iter().copied().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
            }
        }
        if self.distance_m.is_none() {
            self.distance_m = s.dist.iter().rev().flatten().next().copied().filter(|d| *d > 0.0);
        }
        if self.elev_gain_m.is_none() {
            let alts: Vec<f64> = s.alt.iter().flatten().copied().collect();
            if alts.len() > 1 {
                let mut gain = 0.0;
                for w in alts.windows(2) {
                    let d = w[1] - w[0];
                    if d > 0.0 {
                        gain += d;
                    }
                }
                self.elev_gain_m = Some(gain);
            }
        }
    }
}

/// Maps a Strava activity type (API `sport_type`, or the Russian/English label
/// from the archive CSV) to Kouros' own sport code.
pub fn sport_from_strava(t: &str) -> &'static str {
    let k = t.trim().to_lowercase();
    match k.as_str() {
        "run" | "бег" | "trailrun" | "trail run" | "трейлраннинг" | "virtualrun" | "виртуальный забег" => "run",
        "walk" | "ходьба" => "walk",
        "hike" | "поход" => "hike",
        "ride" | "велосипед" | "велозаезд" | "заезд" | "gravelride" | "mountainbikeride" | "ebikeride" | "велопрогулка" => "ride",
        "virtualride" | "virtual ride" | "виртуальный заезд" => "ride_virtual",
        "weighttraining" | "weight training" | "силовая тренировка" => "strength",
        "workout" | "тренировка" | "crossfit" | "кроссфит" | "hiit" => "workout",
        "yoga" | "йога" | "pilates" | "пилатес" => "yoga",
        "swim" | "плавание" => "swim",
        _ => "other",
    }
}
pub mod hae;
