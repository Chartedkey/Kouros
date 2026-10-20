//! Estimated fiber and micronutrients for YAZIO entries that came with calories and macros only
//! (photo recognition, product cards without micros). Real data is never touched.
//!
//! Order: 1) the same product from your own history with full data (nutrients per kcal);
//! 2) Russian dishes (`food_dishes.rs`, 0.19: recipes computed from the table below), then
//! a built-in reference table (per 100 g, approximate values after USDA FoodData Central,
//! public domain), matched by words in the name; dishes with several matched parts are split by
//! fitting their shares to the entry's own protein / carbs / fat. Supplements are skipped.

use crate::engine::day_of;
use crate::health_svc::iso;
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

/// Nutrients we estimate: (food_entry_* suffix, Apple daily-total metric).
pub const NUTS: [(&str, &str); 22] = [
    ("fiber", "dietary_fiber_total"),
    ("potassium", "dietary_potassium_total"),
    ("magnesium", "dietary_magnesium_total"),
    ("calcium", "dietary_calcium_total"),
    ("iron", "dietary_iron_total"),
    ("zinc", "dietary_zinc_total"),
    ("vitamin_c", "dietary_vitamin_c_total"),
    ("vitamin_a", "dietary_vitamin_a_total"),
    ("folate", "dietary_folate_total"),
    ("vitamin_b12", "dietary_vitamin_b12_total"),
    ("vitamin_d", "dietary_vitamin_d_total"),
    ("vitamin_e", "dietary_vitamin_e_total"),
    ("vitamin_k", "dietary_vitamin_k_total"),
    ("thiamin", "dietary_thiamin_total"),
    ("riboflavin", "dietary_riboflavin_total"),
    ("niacin", "dietary_niacin_total"),
    ("vitamin_b6", "dietary_vitamin_b6_total"),
    ("selenium", "dietary_selenium_total"),
    ("phosphorus", "dietary_phosphorus_total"),
    ("copper", "dietary_copper_total"),
    ("manganese", "dietary_manganese_total"),
    ("sodium", "dietary_sodium_total"),
];
const N: usize = 22;

/// Group densities per 100 kcal for the nutrients the per-food table does not carry:
/// vit D µg, E mg, K µg, B1 mg, B2 mg, B3 mg, B6 mg, Se µg, P mg, Cu mg, Mn mg, Na mg.
/// Sodium is without salt added while cooking.
fn group(c: char) -> [f64; 12] {
    match c {
        'm' => [0.04, 0.08, 0.6, 0.1, 0.08, 2.0, 0.16, 8., 80., 0.04, 0.005, 25.],
        'p' => [0.06, 0.18, 0.2, 0.04, 0.07, 8.0, 0.36, 16., 138., 0.03, 0.01, 45.],
        'f' => [3.0, 1.0, 0.3, 0.1, 0.15, 4.5, 0.35, 25., 150., 0.08, 0.02, 50.],
        'e' => [1.4, 0.73, 0.2, 0.03, 0.32, 0.05, 0.12, 22., 138., 0.05, 0.02, 99.],
        'd' => [0.1, 0.1, 0.4, 0.05, 0.25, 0.15, 0.06, 6., 150., 0.02, 0.005, 90.],
        'c' => [0.16, 0.18, 0.6, 0.01, 0.1, 0.03, 0.02, 3.7, 130., 0.01, 0.01, 170.],
        'g' => [0.0, 0.1, 0.8, 0.06, 0.03, 0.8, 0.08, 5., 75., 0.1, 0.6, 3.],
        'b' => [0.0, 0.08, 0.4, 0.12, 0.06, 1.1, 0.04, 7., 45., 0.08, 0.35, 190.],
        'l' => [0.0, 0.3, 3.0, 0.15, 0.05, 0.4, 0.1, 2., 100., 0.25, 0.5, 150.],
        'v' => [0.1, 2.5, 80., 0.2, 0.25, 2.0, 0.5, 2., 150., 0.2, 0.6, 80.],
        'k' => [0.0, 8.0, 900., 0.3, 0.8, 3.0, 0.8, 4., 200., 0.5, 3.5, 300.],
        'r' => [0.0, 0.2, 1.0, 0.05, 0.05, 0.5, 0.3, 0.5, 20., 0.08, 0.2, 1.],
        'n' => [0.0, 1.5, 3.0, 0.12, 0.05, 0.5, 0.06, 3., 90., 0.25, 0.35, 3.],
        's' => [0.05, 0.5, 1.0, 0.03, 0.04, 0.3, 0.02, 2., 25., 0.05, 0.1, 70.],
        'o' => [0.02, 2.5, 6.0, 0., 0., 0., 0., 0., 0., 0., 0., 0.],
        'x' => [0.1, 0.4, 3.0, 0.1, 0.1, 1.5, 0.1, 7., 70., 0.05, 0.15, 230.],
        'q' => [0.1, 0.07, 0.5, 0.07, 0.05, 1.0, 0.05, 4., 50., 0.02, 0.01, 330.],
        _ => [0.0, 0.1, 0.2, 0.2, 0.06, 0.9, 0.1, 0.2, 38., 0.1, 0.03, 2.],
    }
}

/// Nutrients per kcal of a reference row, in NUTS order.
fn per_kcal(cat: char, r: &[f64; 14]) -> [f64; N] {
    let kc = r[0].max(1.0);
    let g = group(cat);
    let mut o = [0.0; N];
    for i in 0..10 {
        o[i] = r[4 + i] / kc;
    }
    for i in 0..12 {
        o[10 + i] = g[i] / 100.0;
    }
    o
}

/// keys, then per 100 g: kcal, protein, carbs, fat, fiber g, K mg, Mg mg, Ca mg, Fe mg, Zn mg,
/// vit C mg, vit A µg RAE, folate µg, B12 µg. More specific entries first.
type Row = (&'static [&'static str], char, [f64; 14]);
const REF: &[Row] = &[
    (&["куриная грудк", "куриное филе", "филе кур", "грудка кур", "грудк"], 'p', [165., 31., 0., 3.6, 0., 256., 29., 15., 1.0, 1.0, 0., 6., 4., 0.3]),
    (&["печенье"], 's', [480., 6., 65., 22., 2., 140., 25., 30., 2., 0.6, 0., 0., 20., 0.]),
    (&["печень"], 'm', [119., 17., 0.7, 4.8, 0., 230., 19., 8., 9., 2.7, 18., 3296., 588., 16.6]),
    (&["грудинк", "бекон"], 'q', [400., 12., 0., 40., 0., 250., 12., 8., 0.7, 1.5, 0., 10., 3., 0.6]),
    (&["индейк"], 'p', [150., 29., 0., 3., 0., 300., 28., 10., 1.1, 2., 0., 5., 8., 0.4]),
    (&["куриц", "курин", "цыпл", "окороч", "бедр"], 'p', [190., 27., 0., 9., 0., 230., 23., 15., 1.2, 1.9, 0., 15., 6., 0.3]),
    (&["говяд", "телят", "стейк"], 'm', [250., 26., 0., 15., 0., 320., 21., 18., 2.6, 6., 0., 0., 8., 2.6]),
    (&["фарш", "котлет", "тефтел"], 'm', [230., 18., 5., 15., 0.3, 280., 20., 20., 2., 3.5, 0., 10., 8., 1.5]),
    (&["свин"], 'm', [242., 27., 0., 14., 0., 423., 28., 19., 0.9, 2.4, 0.6, 2., 5., 0.7]),
    (&["колбас", "сервелат", "салями"], 'q', [300., 12., 2., 27., 0., 250., 15., 10., 1., 1.5, 0., 0., 3., 0.8]),
    (&["сосиск", "сардельк"], 'q', [260., 11., 2., 23., 0., 200., 13., 15., 1., 1.5, 0., 0., 3., 0.8]),
    (&["ветчин"], 'q', [145., 21., 1.5, 6., 0., 290., 20., 8., 1., 2., 0., 0., 3., 0.6]),
    (&["лосос", "сёмг", "семг", "форел", "горбуш", "кижуч", "нерк"], 'f', [208., 20., 0., 13., 0., 363., 27., 9., 0.3, 0.4, 0., 58., 26., 3.2]),
    (&["тунец", "тунц"], 'f', [130., 28., 0., 1., 0., 320., 35., 10., 1., 0.6, 0., 20., 4., 2.5]),
    (&["треск", "минтай", "хек", "пикш", "судак", "тилапи"], 'f', [82., 18., 0., 0.7, 0., 413., 32., 16., 0.4, 0.5, 1., 12., 7., 0.9]),
    (&["кревет", "кальмар", "мидии"], 'f', [99., 24., 0.2, 0.3, 0., 170., 39., 70., 0.5, 1.6, 0., 0., 19., 1.1]),
    (&["рыб", "скумбри", "сельд"], 'f', [150., 20., 0., 8., 0., 350., 30., 15., 0.8, 0.6, 0., 30., 10., 5.]),
    (&["яиц", "яйц", "яичн", "омлет", "белок яичн"], 'e', [143., 12.6, 0.7, 9.5, 0., 138., 12., 56., 1.8, 1.3, 0., 160., 47., 0.9]),
    (&["сырник"], 'd', [220., 15., 18., 10., 0.5, 120., 15., 120., 0.6, 0.6, 0., 70., 20., 0.5]),
    (&["творож", "творог"], 'd', [120., 17., 3., 5., 0., 110., 10., 120., 0.2, 0.4, 0., 40., 15., 0.6]),
    (&["греческ"], 'd', [97., 9., 4., 5., 0., 141., 11., 100., 0.1, 0.5, 0., 30., 7., 0.8]),
    (&["йогурт"], 'd', [70., 5., 8., 2., 0., 200., 17., 150., 0.1, 0.6, 0.5, 20., 10., 0.5]),
    (&["кефир", "ряженк", "айран"], 'd', [50., 3., 4., 2., 0., 150., 12., 120., 0.1, 0.4, 1., 25., 5., 0.4]),
    (&["моцарел", "рикотт"], 'c', [280., 22., 2., 20., 0., 76., 20., 500., 0.4, 2.9, 0., 180., 7., 2.3]),
    (&["сыр"], 'c', [380., 25., 1., 30., 0., 90., 27., 800., 0.5, 3.5, 0., 250., 20., 1.5]),
    (&["капучин", "латте", "раф"], 'd', [50., 3., 4.5, 2.5, 0., 150., 12., 110., 0.1, 0.4, 0., 30., 5., 0.3]),
    (&["молок", "молоч"], 'd', [60., 3.2, 4.8, 3.2, 0., 150., 10., 120., 0.03, 0.4, 0., 46., 5., 0.45]),
    (&["протеин", "protein", "whey"], 'd', [380., 75., 8., 5., 0., 500., 100., 400., 2., 3., 0., 0., 0., 1.]),
    (&["тофу"], 'l', [76., 8., 1.9, 4.8, 0.3, 121., 30., 350., 5.4, 0.8, 0.1, 0., 15., 0.]),
    (&["фасол"], 'l', [120., 7., 21., 0.4, 5.5, 420., 45., 60., 2.5, 1.2, 0.5, 3., 60., 0.]),
    (&["нут", "хумус"], 'l', [164., 8.9, 27., 2.6, 7.6, 291., 48., 49., 2.9, 1.5, 1.3, 1., 172., 0.]),
    (&["чечевиц"], 'l', [116., 9., 20., 0.4, 7.9, 369., 36., 19., 3.3, 1.3, 1.5, 0., 181., 0.]),
    (&["горох", "горош"], 'l', [118., 8.3, 21., 0.4, 8.3, 362., 36., 14., 1.3, 1., 0.4, 0., 65., 0.]),
    (&["бурый рис", "дикий рис"], 'g', [112., 2.3, 23.5, 0.8, 1.8, 79., 39., 10., 0.4, 0.6, 0., 0., 4., 0.]),
    (&["рис"], 'g', [130., 2.7, 28., 0.3, 0.4, 35., 12., 10., 0.2, 0.5, 0., 0., 3., 0.]),
    (&["гречк", "гречнев"], 'g', [92., 3.4, 20., 0.6, 2.7, 88., 51., 7., 0.8, 0.6, 0., 0., 14., 0.]),
    (&["гранол"], 'g', [470., 10., 64., 20., 7., 400., 110., 60., 3., 2.5, 0., 0., 40., 0.]),
    (&["мюсли"], 'g', [370., 10., 66., 6., 8., 370., 100., 50., 3., 2.4, 1., 0., 40., 0.]),
    (&["овсян", "овёс", "геркулес"], 'g', [71., 2.5, 12., 1.5, 1.7, 70., 27., 9., 0.9, 1., 0., 0., 6., 0.]),
    (&["булгур"], 'g', [83., 3.1, 18.6, 0.2, 4.5, 68., 32., 10., 1., 0.6, 0., 0., 18., 0.]),
    (&["перлов"], 'g', [123., 2.3, 28., 0.4, 3.8, 94., 22., 11., 1.3, 0.8, 0., 0., 16., 0.]),
    (&["пшён", "пшен"], 'g', [119., 3.5, 23.7, 1., 1.3, 62., 44., 3., 0.6, 0.9, 0., 0., 9., 0.]),
    (&["киноа"], 'g', [120., 4.4, 21., 1.9, 2.8, 172., 64., 17., 1.5, 1.1, 0., 1., 42., 0.]),
    (&["макарон", "паста", "спагет", "лапш", "пенне", "фузилл"], 'g', [158., 5.8, 31., 0.9, 1.8, 44., 18., 7., 1.3, 0.5, 0., 0., 7., 0.]),
    (&["кукуруз"], 'v', [96., 3.4, 21., 1.5, 2.4, 218., 26., 3., 0.5, 0.6, 5.5, 9., 23., 0.]),
    (&["фри"], 'x', [312., 3.4, 41., 15., 3.8, 579., 35., 18., 0.8, 0.5, 4.7, 0., 30., 0.]),
    (&["пюре"], 'x', [100., 1.9, 16., 3.5, 1.5, 280., 19., 24., 0.3, 0.3, 5., 30., 8., 0.1]),
    (&["картоф", "картош"], 'v', [87., 1.9, 20., 0.1, 1.8, 379., 22., 5., 0.3, 0.3, 7.4, 0., 10., 0.]),
    (&["батончик"], 's', [350., 25., 35., 12., 10., 250., 60., 150., 2., 2., 0., 0., 20., 0.5]),
    (&["ржан", "бородин"], 'b', [259., 8.5, 48., 3.3, 5.8, 166., 40., 73., 2.8, 1.1, 0., 1., 51., 0.]),
    (&["цельнозерн"], 'b', [250., 12., 43., 3.5, 7., 250., 75., 100., 2.5, 1.8, 0., 0., 40., 0.]),
    (&["лаваш", "тортиль"], 'b', [275., 9., 56., 1.2, 2.2, 120., 25., 20., 1.5, 0.7, 0., 0., 30., 0.]),
    (&["хлеб", "батон", "булк", "багет", "тост"], 'b', [265., 9., 49., 3.2, 2.7, 115., 25., 50., 1.5, 0.7, 0., 0., 40., 0.]),
    (&["блин", "оладь"], 'b', [227., 6., 28., 10., 0.9, 140., 15., 110., 1.2, 0.5, 0., 60., 20., 0.3]),
    (&["пицц"], 'x', [266., 11., 33., 10., 2.3, 172., 24., 188., 2.5, 1.3, 1., 70., 40., 0.5]),
    (&["бургер", "шаурм", "сэндвич", "бутерброд"], 'x', [250., 13., 24., 11., 1.5, 230., 20., 80., 2.5, 2.2, 0.5, 10., 30., 1.]),
    (&["борщ"], 'x', [50., 2., 6., 2., 1.5, 200., 12., 20., 0.6, 0.3, 6., 50., 15., 0.1]),
    (&["суп", "щи", "солянк", "бульон"], 'x', [40., 2., 5., 1.5, 1., 120., 8., 15., 0.4, 0.3, 3., 60., 8., 0.1]),
    (&["плов"], 'x', [180., 6., 24., 7., 1., 120., 15., 15., 1., 1., 1., 40., 8., 0.3]),
    (&["брокк"], 'v', [35., 2.4, 7., 0.4, 3.3, 293., 21., 40., 0.7, 0.5, 65., 77., 108., 0.]),
    (&["цветн"], 'v', [23., 1.8, 4.1, 0.5, 2.3, 142., 9., 16., 0.3, 0.2, 44., 1., 44., 0.]),
    (&["капуст"], 'v', [25., 1.3, 5.8, 0.1, 2.5, 170., 12., 40., 0.5, 0.2, 37., 5., 43., 0.]),
    (&["морков"], 'v', [41., 0.9, 9.6, 0.2, 2.8, 320., 12., 33., 0.3, 0.2, 6., 835., 19., 0.]),
    (&["огур"], 'v', [15., 0.7, 3.6, 0.1, 0.5, 147., 13., 16., 0.3, 0.2, 2.8, 5., 7., 0.]),
    (&["помидор", "томат", "черри"], 'v', [18., 0.9, 3.9, 0.2, 1.2, 237., 11., 10., 0.3, 0.2, 14., 42., 15., 0.]),
    (&["перец", "перц"], 'v', [27., 1., 6., 0.3, 2., 211., 12., 7., 0.4, 0.25, 128., 157., 46., 0.]),
    (&["кабач", "цукин"], 'v', [17., 1.2, 3.1, 0.3, 1., 261., 18., 16., 0.4, 0.3, 18., 10., 24., 0.]),
    (&["баклаж"], 'v', [25., 1., 6., 0.2, 3., 229., 14., 9., 0.2, 0.2, 2., 1., 22., 0.]),
    (&["свекл", "свёкл"], 'v', [44., 1.7, 10., 0.2, 2., 305., 23., 16., 0.8, 0.35, 3.6, 2., 80., 0.]),
    (&["шпинат"], 'k', [23., 2.9, 3.6, 0.4, 2.2, 558., 79., 99., 2.7, 0.5, 28., 469., 194., 0.]),
    (&["руккол", "айсберг", "листь", "салат"], 'k', [15., 1.4, 2.9, 0.2, 1.3, 194., 13., 36., 0.9, 0.2, 9., 370., 38., 0.]),
    (&["зелен", "укроп", "петруш", "кинз"], 'k', [40., 3., 7., 0.8, 3., 550., 60., 150., 4., 1., 100., 300., 120., 0.]),
    (&["лук"], 'v', [40., 1.1, 9.3, 0.1, 1.7, 146., 10., 23., 0.2, 0.2, 7.4, 0., 19., 0.]),
    (&["гриб", "шампин", "вешенк"], 'v', [22., 3.1, 3.3, 0.3, 1., 318., 9., 3., 0.5, 0.5, 2., 0., 17., 0.04]),
    (&["авокад"], 'v', [160., 2., 8.5, 14.7, 6.7, 485., 29., 12., 0.55, 0.64, 10., 7., 81., 0.]),
    (&["овощ", "рагу", "лечо"], 'v', [50., 2.5, 10., 0.3, 3.5, 200., 18., 30., 0.7, 0.4, 15., 120., 30., 0.]),
    (&["лён", "льнян", "семена льна"], 'n', [534., 18., 29., 42., 27., 813., 392., 255., 5.7, 4.3, 0.6, 0., 87., 0.]),
    (&["чиа"], 'n', [486., 17., 42., 31., 34., 407., 335., 631., 7.7, 4.6, 1.6, 0., 49., 0.]),
    (&["яблок"], 'r', [52., 0.3, 14., 0.2, 2.4, 107., 5., 6., 0.1, 0., 4.6, 3., 3., 0.]),
    (&["банан"], 'r', [89., 1.1, 23., 0.3, 2.6, 358., 27., 5., 0.3, 0.15, 8.7, 3., 20., 0.]),
    (&["апельсин", "мандарин", "клементин"], 'r', [47., 0.9, 12., 0.1, 2.4, 181., 10., 40., 0.1, 0.07, 53., 11., 30., 0.]),
    (&["грейпфрут", "помело"], 'r', [42., 0.8, 11., 0.1, 1.6, 135., 9., 22., 0.1, 0.07, 31., 58., 13., 0.]),
    (&["груш"], 'r', [57., 0.4, 15., 0.1, 3.1, 116., 7., 9., 0.2, 0.1, 4.3, 1., 7., 0.]),
    (&["виноград"], 'r', [69., 0.7, 18., 0.2, 0.9, 191., 7., 10., 0.4, 0.1, 3.2, 3., 2., 0.]),
    (&["ягод", "клубник", "черник", "малин", "голубик", "смородин", "клюкв", "земляник"], 'r', [45., 0.8, 10., 0.4, 4., 150., 15., 20., 0.5, 0.3, 40., 3., 25., 0.]),
    (&["киви"], 'r', [61., 1.1, 15., 0.5, 3., 312., 17., 34., 0.3, 0.1, 93., 4., 25., 0.]),
    (&["хурм"], 'r', [70., 0.6, 18.6, 0.2, 3.6, 161., 9., 8., 0.15, 0.1, 7.5, 81., 8., 0.]),
    (&["персик", "нектарин", "абрикос"], 'r', [39., 0.9, 9.5, 0.25, 1.5, 190., 9., 6., 0.25, 0.2, 6.6, 16., 4., 0.]),
    (&["ананас", "манго"], 'r', [55., 0.6, 14., 0.2, 1.5, 140., 11., 12., 0.2, 0.1, 40., 30., 25., 0.]),
    (&["финик"], 'r', [282., 2.5, 75., 0.4, 8., 656., 43., 39., 1., 0.3, 0.4, 0., 19., 0.]),
    (&["изюм"], 'r', [299., 3.1, 79., 0.5, 3.7, 749., 32., 50., 1.9, 0.2, 2.3, 0., 5., 0.]),
    (&["кураг"], 'r', [241., 3.4, 63., 0.5, 7.3, 1162., 32., 55., 2.7, 0.4, 1., 180., 10., 0.]),
    (&["кешью"], 'n', [574., 15., 32., 46., 3., 565., 260., 45., 6., 5.6, 0., 0., 25., 0.]),
    (&["миндал"], 'n', [579., 21., 22., 50., 12.5, 733., 270., 269., 3.7, 3.1, 0., 0., 44., 0.]),
    (&["грецк"], 'n', [654., 15., 14., 65., 6.7, 441., 158., 98., 2.9, 3.1, 1.3, 1., 98., 0.]),
    (&["арахис"], 'n', [567., 26., 16., 49., 8.5, 705., 168., 92., 4.6, 3.3, 0., 0., 240., 0.]),
    (&["фундук"], 'n', [628., 15., 17., 61., 9.7, 680., 163., 114., 4.7, 2.5, 6.3, 1., 113., 0.]),
    (&["семечк", "подсолнеч", "тыквен"], 'n', [584., 21., 20., 51., 8.6, 645., 325., 78., 5.3, 5., 1.4, 3., 227., 0.]),
    (&["орех", "фисташ"], 'n', [600., 17., 20., 53., 7., 600., 200., 100., 3.5, 3., 1., 0., 60., 0.]),
    (&["шоколад"], 's', [546., 6., 55., 33., 8., 600., 180., 60., 9., 2.5, 0., 0., 10., 0.]),
    (&["мороже", "пломбир"], 's', [207., 3.5, 24., 11., 0.7, 199., 14., 128., 0.1, 0.7, 0.6, 118., 5., 0.4]),
    (&["торт", "пирожн", "кекс", "маффин", "круассан", "выпечк", "пирог"], 's', [370., 5., 50., 17., 1., 120., 15., 60., 1.5, 0.5, 0., 80., 20., 0.2]),
    (&["мёд"], 's', [304., 0.3, 82., 0., 0.2, 52., 2., 6., 0.4, 0.2, 0.5, 0., 2., 0.]),
    (&["сливочн"], 'o', [717., 0.9, 0.1, 81., 0., 24., 2., 24., 0., 0.1, 0., 684., 3., 0.2]),
    (&["масло"], 'o', [884., 0., 0., 100., 0., 1., 0., 1., 0.6, 0., 0., 0., 0., 0.]),
    (&["сок"], 'j', [45., 0.7, 10., 0.2, 0.2, 200., 11., 11., 0.2, 0.05, 50., 10., 30., 0.]),
];


pub struct DayEst {
    /// Estimated amounts added, in NUTS order.
    pub add: [f64; N],
    /// Micros that came with supplement entries in YAZIO — taken out of the food totals.
    pub supp: [f64; N],
    pub entries: usize,
    pub estimated: usize,
    pub unknown: Vec<String>,
    /// Nutrients whose estimate that day was implausibly high and was dropped.
    pub dropped: Vec<usize>,
}

/// One food entry and how its nutrients were obtained.
pub struct EntryEst {
    pub ts: i64,
    pub name: String,
    pub kcal: f64,
    /// "yazio" | "history" | "reference" | "unknown" | "supplement"
    pub kind: &'static str,
    pub parts: Vec<String>,
    pub add: [f64; N],
}

const SUPPLEMENTS: [&str; 16] = [
    "omega", "омега", "multi", "мульти", "витамин", "vitamin", "капсул", "таблет", "bcaa", "креатин", "creatine", "коллаген", "magnesium", "цинк", "d3", "б12",
];

pub fn is_supplement(name: &str) -> bool {
    let n = name.to_lowercase();
    SUPPLEMENTS.iter().any(|s| n.contains(s))
}

#[derive(Default)]
struct Entry {
    names: Vec<String>,
    kcal: f64,
    p: Option<f64>,
    c: Option<f64>,
    f: Option<f64>,
    micro: [Option<f64>; N],
}

fn load(conn: &Connection, from: &str) -> BTreeMap<i64, Entry> {
    let mut out: BTreeMap<i64, Entry> = BTreeMap::new();
    if let Ok(mut st) = conn.prepare("SELECT ts, txt FROM hm WHERE metric = 'food_entry_name' AND date >= ?1 AND txt IS NOT NULL") {
        let rows: Vec<(String, String)> = st.query_map(params![from], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
        for (ts, name) in rows {
            if let Some(t) = iso(&ts) {
                out.entry(t).or_default().names.push(name.trim().to_lowercase());
            }
        }
    }
    let mut metrics = vec!["'food_entry_energy'", "'food_entry_protein'", "'food_entry_carbohydrates'", "'food_entry_fat_total'"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    for (n, _) in NUTS {
        metrics.push(format!("'food_entry_{n}'"));
    }
    let sql = format!("SELECT ts, metric, num FROM hm WHERE metric IN ({}) AND date >= ?1 AND num IS NOT NULL", metrics.join(","));
    if let Ok(mut st) = conn.prepare(&sql) {
        let rows: Vec<(String, String, f64)> = st.query_map(params![from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()).unwrap_or_default();
        for (ts, m, v) in rows {
            let Some(t) = iso(&ts) else { continue };
            let e = out.entry(t).or_default();
            let add = |x: &mut Option<f64>| *x = Some(x.unwrap_or(0.0) + v);
            match m.trim_start_matches("food_entry_") {
                "energy" => e.kcal += v,
                "protein" => add(&mut e.p),
                "carbohydrates" => add(&mut e.c),
                "fat_total" => add(&mut e.f),
                k => {
                    if let Some(i) = NUTS.iter().position(|(n, _)| *n == k) {
                        add(&mut e.micro[i]);
                    }
                }
            }
        }
    }
    out
}

/// Shares (by calories) of the matched parts that best reproduce the entry's macro split.
fn fit(parts: &[&[f64; 14]], e: &Entry) -> Vec<f64> {
    let k = parts.len();
    if k == 1 {
        return vec![1.0];
    }
    let frac = |r: &[f64; 14]| {
        let kc = r[0].max(1.0);
        (r[1] * 4.0 / kc, r[2] * 4.0 / kc, r[3] * 9.0 / kc)
    };
    let kc = e.kcal.max(1.0);
    let tp = e.p.map(|x| x * 4.0 / kc);
    let tc = e.c.map(|x| x * 4.0 / kc);
    let tf = e.f.map(|x| x * 9.0 / kc);
    if tp.is_none() && tc.is_none() && tf.is_none() {
        return vec![1.0 / k as f64; k];
    }
    let fr: Vec<(f64, f64, f64)> = parts.iter().map(|r| frac(r)).collect();
    let cost = |w: &[f64]| {
        let (mut p, mut c, mut f) = (0.0, 0.0, 0.0);
        for (i, x) in w.iter().enumerate() {
            p += x * fr[i].0;
            c += x * fr[i].1;
            f += x * fr[i].2;
        }
        let sq = |a: f64, t: Option<f64>| t.map(|t| (a - t).powi(2)).unwrap_or(0.0);
        // A small pull towards equal shares keeps the split sane when macros fit several ways.
        let even: f64 = w.iter().map(|x| (x - 1.0 / k as f64).powi(2)).sum::<f64>() * 0.02;
        sq(p, tp) + sq(c, tc) + sq(f, tf) + even
    };
    let step = if k <= 3 { 20 } else { 10 };
    let mut best = (f64::MAX, vec![1.0 / k as f64; k]);
    let mut w = vec![0usize; k];
    // Enumerate compositions of `step` into k parts.
    fn rec(i: usize, left: usize, w: &mut Vec<usize>, step: usize, f: &mut dyn FnMut(&[usize])) {
        if i + 1 == w.len() {
            w[i] = left;
            f(w);
            return;
        }
        for x in 0..=left {
            w[i] = x;
            rec(i + 1, left - x, w, step, f);
        }
    }
    rec(0, step, &mut w, step, &mut |c: &[usize]| {
        let ws: Vec<f64> = c.iter().map(|x| *x as f64 / step as f64).collect();
        let v = cost(&ws);
        if v < best.0 {
            best = (v, ws);
        }
    });
    best.1
}

/// Short words that must stand alone ("сок", not "высокобелковый").
const WHOLE: [&str; 8] = ["фри", "сок", "нут", "лук", "щи", "раф", "чиа", "батон"];

/// Position of `k` at the start of a word in `text` (and at its end too for WHOLE keys).
fn find_word(text: &str, k: &str) -> Option<usize> {
    let whole = WHOLE.contains(&k);
    text.match_indices(k).map(|(i, _)| i).find(|&i| {
        let before = text[..i].chars().next_back().map(|c| c.is_alphabetic()).unwrap_or(false);
        let after = text[i + k.len()..].chars().next().map(|c| c.is_alphabetic()).unwrap_or(false);
        !before && (!whole || !after)
    })
}

/// Parts of an entry: for every product name, a Russian dish first (one dish = the whole name),
/// else the reference ingredients found in it. At most 4 parts in all.
fn match_ref(names: &[String]) -> Vec<(&'static str, char, &'static [f64; 14])> {
    let mut out = Vec::new();
    // «ё» and «е» are written both ways in product names.
    let norm = |t: &str| t.replace('ё', "е");
    'names: for name in names {
        let name = norm(name);
        for (keys, cat, v) in crate::food_dishes::DISHES {
            if let Some(k) = keys.iter().find(|k| find_word(&name, &norm(k)).is_some()) {
                out.push((*k, *cat, v));
                if out.len() >= 4 {
                    break 'names;
                }
                continue 'names;
            }
        }
        let mut text = name.clone();
        for (keys, cat, v) in REF {
            for k in keys.iter() {
                let kn = norm(k);
                if let Some(pos) = find_word(&text, &kn) {
                    out.push((*k, *cat, v));
                    text.replace_range(pos..pos + kn.len(), &"_".repeat(kn.len()));
                    break;
                }
            }
            if out.len() >= 4 {
                break 'names;
            }
        }
    }
    out
}

fn history(all: &BTreeMap<i64, Entry>) -> BTreeMap<String, (f64, [f64; N], [bool; N])> {
    let mut hist: BTreeMap<String, (f64, [f64; N], [bool; N])> = BTreeMap::new();
    for e in all.values() {
        if e.kcal < 5.0 || e.micro.iter().all(|m| m.is_none()) || e.names.is_empty() {
            continue;
        }
        let mut names = e.names.clone();
        names.dedup();
        if names.len() != 1 || is_supplement(&names[0]) {
            continue;
        }
        let h = hist.entry(names[0].clone()).or_insert((0.0, [0.0; N], [false; N]));
        h.0 += e.kcal;
        for i in 0..N {
            if let Some(v) = e.micro[i] {
                h.1[i] += v;
                h.2[i] = true;
            }
        }
    }
    hist
}

/// Every entry from `from` (local calendar days) with how its nutrients were obtained.
pub fn entries(conn: &Connection, from: &str, tz: i64) -> Vec<(String, EntryEst)> {
    let all = load(conn, "2000-01-01");
    let hist = history(&all);
    let start = iso(&format!("{from}T00:00:00Z")).unwrap_or(0) - 86400;
    let mut out = Vec::new();
    for (ts, e) in all.range(start..) {
        let d = day_of(*ts, tz);
        if d.as_str() < from {
            continue;
        }
        let mut names = e.names.clone();
        names.dedup();
        let name = names.join(" + ");
        let mut est = EntryEst { ts: *ts, name: name.clone(), kcal: e.kcal, kind: "unknown", parts: Vec::new(), add: [0.0; N] };
        if is_supplement(&name) {
            est.kind = "supplement";
            for i in 0..N {
                est.add[i] = e.micro[i].unwrap_or(0.0);
            }
            out.push((d, est));
            continue;
        }
        if e.kcal <= 0.0 {
            continue;
        }
        if e.micro.iter().any(|m| m.is_some()) {
            est.kind = "yazio";
            out.push((d, est));
            continue;
        }
        if let Some(h) = names.first().and_then(|n| if names.len() == 1 { hist.get(n) } else { None }) {
            est.kind = "history";
            for i in 0..N {
                if h.2[i] {
                    est.add[i] = e.kcal * h.1[i] / h.0.max(1.0);
                }
            }
            out.push((d, est));
            continue;
        }
        let parts = match_ref(&names);
        if !parts.is_empty() {
            let rows: Vec<&[f64; 14]> = parts.iter().map(|p| p.2).collect();
            let w = fit(&rows, e);
            for (j, (key, cat, r)) in parts.iter().enumerate() {
                let pk = per_kcal(*cat, r);
                for i in 0..N {
                    est.add[i] += e.kcal * w[j] * pk[i];
                }
                est.parts.push(format!("{key} {:.0}%", w[j] * 100.0));
            }
            est.kind = "reference";
        }
        out.push((d, est));
    }
    out
}

/// Estimates per local calendar day (as Apple's daily totals), from `from` on. `caps` are the
/// norms in NUTS order: a day's estimate above 4× the norm is treated as implausible and dropped.
pub fn estimate(conn: &Connection, from: &str, tz: i64, caps: &[f64; N]) -> BTreeMap<String, DayEst> {
    let mut out: BTreeMap<String, DayEst> = BTreeMap::new();
    for (d, e) in entries(conn, from, tz) {
        let day = out.entry(d).or_insert(DayEst { add: [0.0; N], supp: [0.0; N], entries: 0, estimated: 0, unknown: Vec::new(), dropped: Vec::new() });
        match e.kind {
            "supplement" => {
                for i in 0..N {
                    day.supp[i] += e.add[i];
                }
            }
            "history" | "reference" => {
                day.entries += 1;
                day.estimated += 1;
                for i in 0..N {
                    day.add[i] += e.add[i];
                }
            }
            "unknown" => {
                day.entries += 1;
                if !day.unknown.contains(&e.name) {
                    day.unknown.push(e.name);
                }
            }
            _ => day.entries += 1,
        }
    }
    for day in out.values_mut() {
        for i in 0..N {
            if caps[i] > 0.0 && day.add[i] > 4.0 * caps[i] {
                day.add[i] = 0.0;
                day.dropped.push(i);
            }
        }
    }
    out
}
