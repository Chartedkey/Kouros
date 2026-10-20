//! Daily intake norms for an adult man, by country / authority. Approximate reference values
//! (RDA / PRI / RNI or adequate intake where no RDA exists). Sodium is a guide value, not a goal.
//! Order matches `MICRO` in nutrition_svc.

pub const COUNTRIES: [(&str, &str, &str); 6] = [
    ("ru", "Россия", "МР 2.3.1.0253-21 (Роспотребнадзор)"),
    ("eu", "Европа", "EFSA"),
    ("us", "США", "NIH / NASEM"),
    ("who", "ВОЗ", "WHO/FAO"),
    ("uk", "Великобритания", "UK RNI (COMA/SACN)"),
    ("dach", "Германия, Австрия, Швейцария", "DACH"),
];

/// metric → value per country (ru, eu, us, who, uk, dach).
pub const NORMS: [(&str, [f64; 6]); 21] = [
    ("dietary_vitamin_a_total", [900., 750., 900., 600., 700., 850.]),
    ("dietary_vitamin_c_total", [100., 110., 90., 45., 40., 110.]),
    ("dietary_vitamin_d_total", [15., 15., 15., 5., 10., 20.]),
    ("dietary_vitamin_e_total", [15., 13., 15., 10., 4., 14.]),
    ("dietary_vitamin_k_total", [120., 70., 120., 65., 75., 70.]),
    ("dietary_thiamin_total", [1.5, 1.3, 1.2, 1.2, 1.0, 1.2]),
    ("dietary_riboflavin_total", [1.8, 1.6, 1.3, 1.3, 1.3, 1.4]),
    ("dietary_niacin_total", [20., 17., 16., 16., 17., 15.]),
    ("dietary_vitamin_b6_total", [2.0, 1.7, 1.3, 1.3, 1.4, 1.6]),
    ("dietary_folate_total", [400., 330., 400., 400., 200., 300.]),
    ("dietary_vitamin_b12_total", [3.0, 4.0, 2.4, 2.4, 1.5, 4.0]),
    ("dietary_calcium_total", [1000., 950., 1000., 1000., 700., 1000.]),
    ("dietary_iron_total", [10., 11., 8., 9.1, 8.7, 11.]),
    ("dietary_magnesium_total", [420., 350., 420., 260., 300., 350.]),
    ("dietary_zinc_total", [12., 11.7, 11., 7., 9.5, 14.]),
    ("dietary_potassium_total", [3500., 3500., 3400., 3510., 3500., 4000.]),
    ("dietary_selenium_total", [70., 70., 55., 34., 75., 70.]),
    ("dietary_phosphorus_total", [700., 550., 700., 700., 550., 700.]),
    ("dietary_copper_total", [1.0, 1.6, 0.9, 0.9, 1.2, 1.25]),
    ("dietary_manganese_total", [2.0, 3.0, 2.3, 2.3, 1.4, 3.5]),
    ("dietary_sodium_total", [1300., 2000., 1500., 2000., 1600., 1500.]),
];

pub fn idx(country: &str) -> usize {
    COUNTRIES.iter().position(|c| c.0 == country).unwrap_or(0)
}

pub fn norm(metric: &str, country: &str) -> Option<f64> {
    NORMS.iter().find(|n| n.0 == metric).map(|n| n.1[idx(country)])
}
