//! «В каких кроссовках сегодня» (0.23): picks a pair for today's planned run (or walk) from your
//! shoes and their characteristics (filled in by hand or pasted from a review table), your body
//! weight, active pains, wear, rotation and today's conditions from a two-tap question
//! (temperature band, surface). Plain scoring, every point has a reason in words.

use crate::db;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn f(v: &Value, k: &str) -> Option<f64> {
    v[k].as_f64()
}

/// Today's conditions: settings `shoe_conditions` {date, temp, surface}.
/// temp: "lt-10" | "-10..0" | "0..10" | "10..20" | "gt20"; surface: "dry" | "wet" | "snow" | "ice" | "trail".
pub fn conditions(conn: &Connection, today: &str) -> Option<Value> {
    db::get_setting(conn, "shoe_conditions").and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(|v| v["date"].as_str() == Some(today))
}

pub fn set_conditions(conn: &Connection, today: &str, temp: &str, surface: &str) -> Result<(), String> {
    db::set_setting(conn, "shoe_conditions", &json!({"date": today, "temp": temp, "surface": surface}).to_string())
}

/// What today's plan needs from a shoe: (need, title, minutes).
fn need(items: &[Value]) -> Option<(&'static str, String, f64)> {
    let has = |k: &str| items.iter().find(|i| i["kind"] == k).map(|i| i["min"].as_f64().unwrap_or(0.0));
    for (k, n, t) in [("test", "race", "контрольный тест"), ("intervals", "tempo", "интервалы"), ("tempo", "tempo", "темповый бег"), ("long", "long", "длительный бег"), ("easy", "easy", "лёгкий бег"), ("walkrun", "easy", "бег с ходьбой"), ("walk", "walk", "прогулка")] {
        if let Some(m) = has(k) {
            return Some((n, t.to_string(), m));
        }
    }
    None
}

fn last_used(conn: &Connection, id: &str) -> Option<i64> {
    conn.query_row("SELECT max(start_utc) FROM va WHERE shoe_id = ?1", params![id], |r| r.get::<_, Option<i64>>(0)).ok().flatten()
}

/// Ranked shoes for today with reasons.
pub fn advice(conn: &Connection) -> Result<Value, String> {
    let p = crate::engine_svc::profile(conn);
    let today = crate::engine_svc::today(&p);
    let tz = p.tz_offset_s;
    let plan_items: Vec<Value> = conn
        .query_row("SELECT items FROM plan_days WHERE date = ?1", params![today], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let shoes: Vec<Value> = crate::activity_svc::shoes(conn, tz)?.as_array().cloned().unwrap_or_default();
    let active: Vec<&Value> = shoes.iter().filter(|s| !s["retired"].as_bool().unwrap_or(false)).collect();
    let cond = conditions(conn, &today);
    let Some((nd, title, min)) = need(&plan_items) else {
        return Ok(json!({"need": null, "shoes": active.len(), "conditions": cond, "picks": []}));
    };
    let body = crate::exercise_svc::body_kg(conn, &today);
    let pains: Vec<(String, i64)> = crate::body_svc::active(conn).into_iter().map(|(a, _, l, _, _)| (a, l)).collect();
    let pain = |a: &str| pains.iter().filter(|x| x.0 == a).map(|x| x.1).max().unwrap_or(0);
    let now = crate::store::now();
    let temp = cond.as_ref().and_then(|c| c["temp"].as_str().map(String::from)).unwrap_or_default();
    let surface = cond.as_ref().and_then(|c| c["surface"].as_str().map(String::from)).unwrap_or_default();
    let cold = matches!(temp.as_str(), "lt-10" | "-10..0");
    let hot = temp == "gt20";
    let heavy = body.map(|b| b >= 85.0).unwrap_or(false);
    let long_run = nd == "long" || min >= 75.0;

    let mut any_membrane = false;
    let mut picks: Vec<(f64, Value)> = Vec::new();
    for s in &active {
        let kind = s["kind"].as_str().unwrap_or("train");
        if nd != "walk" && kind == "walk" {
            continue;
        }
        let sp = &s["spec"];
        let mut score = 0.0;
        let mut plus: Vec<String> = Vec::new();
        let mut minus: Vec<String> = Vec::new();
        // Purpose.
        let mut pace: Vec<String> = sp["pace"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if pace.is_empty() {
            pace.push(match kind { "race" => "race", "trail" => "trail", "walk" => "walk", _ => "daily" }.to_string());
        }
        let has = |k: &str| pace.iter().any(|x| x == k);
        let fit = match nd {
            "easy" => if has("daily") || has("recovery") { 30.0 } else if has("tempo") { 12.0 } else if has("race") { -5.0 } else { 8.0 },
            "long" => if has("daily") || has("recovery") { 30.0 } else if has("tempo") { 14.0 } else { 0.0 },
            "tempo" => if has("tempo") { 30.0 } else if has("race") { 22.0 } else { 10.0 },
            "race" => if has("race") { 30.0 } else if has("tempo") { 22.0 } else { 5.0 },
            _ => if has("walk") || has("daily") || has("recovery") { 25.0 } else { 5.0 },
        };
        score += fit;
        if fit >= 30.0 {
            plus.push(format!("подходит для «{title}»"));
        } else if has("race") && matches!(nd, "easy" | "long") {
            minus.push("стартовую пару лучше беречь для ключевых дней".into());
        }
        // Cushioning and softness: easy and long runs (more with a heavier runner).
        let wc = match nd { "easy" | "walk" => 12.0, "long" => 14.0, "tempo" => 3.0, _ => 0.0 } * if heavy { 1.5 } else { 1.0 };
        if let Some(sa) = f(sp, "sa") {
            let k = ((sa - 120.0) / 30.0).clamp(-1.0, 1.0);
            score += wc * k;
            if k > 0.4 && wc >= 12.0 {
                plus.push(format!("хорошая амортизация ({} SA){}", sa.round(), if heavy { " — важна при твоём весе" } else { "" }));
            } else if k < -0.3 && wc >= 12.0 {
                minus.push("амортизации мало для такой тренировки".into());
            }
        }
        if let Some(ac) = f(sp, "softness") {
            let k = ((24.0 - ac) / 8.0).clamp(-1.0, 1.0);
            score += k * match nd { "easy" | "walk" => 6.0, "long" => 4.0, _ => 0.0 };
        }
        // Speed: energy return and weight.
        let ws = match nd { "tempo" => 10.0, "race" => 12.0, _ => 0.0 };
        if let Some(er) = f(sp, "er") {
            let k = ((er - 60.0) / 12.0).clamp(-1.0, 1.0);
            score += ws * k;
            if k > 0.5 && ws > 0.0 {
                plus.push(format!("хороший возврат энергии ({}%)", er.round()));
            }
        }
        if let Some(g) = f(sp, "weightG") {
            let k = ((270.0 - g) / 50.0).clamp(-1.0, 1.0);
            score += ws * 0.8 * k;
            if k > 0.5 && ws > 0.0 {
                plus.push(format!("лёгкая ({} г)", g.round()));
            } else if k < -0.4 && ws > 0.0 {
                minus.push(format!("тяжеловата для быстрой работы ({} г)", g.round()));
            }
        }
        // Stability on long runs and with knee / shin pain.
        if sp["arch"] == "stability" {
            if long_run {
                score += 4.0;
                plus.push("стабилизация помогает к концу длинной".into());
            }
            if pain("knees") >= 3 || pain("shin") >= 3 {
                score += 6.0;
                plus.push("стабилизация разгружает колено и голень".into());
            }
        }
        if (pain("knees") >= 3 || pain("shin") >= 3) && f(sp, "sa").map(|x| x >= 130.0).unwrap_or(false) {
            score += 4.0;
        }
        // Achilles and calves: higher drop or a rocker.
        if pain("ankles") >= 3 || pain("calves") >= 3 {
            match f(sp, "dropMm") {
                Some(d) if d < 6.0 => { score -= 12.0; minus.push(format!("низкий перепад ({} мм) — лишняя нагрузка на ахилл и икры", d.round())) }
                Some(d) if d >= 8.0 => { score += 5.0; plus.push(format!("перепад {} мм разгружает ахилл", d.round())) }
                _ => {}
            }
            if sp["rocker"].as_bool() == Some(true) {
                score += 4.0;
                plus.push("рокер разгружает ахилл и икры".into());
            }
        }
        if pain("foot") >= 3 {
            if let Some(w) = f(sp, "toeboxMm") {
                score += ((w - 72.0) / 5.0).clamp(-1.0, 1.0) * 5.0;
                if w >= 75.0 {
                    plus.push("широкий носок — меньше давит на стопу".into());
                }
            }
        }
        // Surface and weather.
        match surface.as_str() {
            "trail" => {
                if has("trail") || kind == "trail" { score += 20.0; plus.push("трейловая — для грязи и пересечённой местности".into()) } else { score -= 10.0 }
            }
            "wet" | "snow" | "ice" => {
                if let Some(t) = f(sp, "traction") {
                    let k = ((t - 0.5) / 0.15).clamp(-1.0, 1.0);
                    score += k * if surface == "wet" { 8.0 } else { 12.0 };
                    if k > 0.3 {
                        plus.push(format!("хорошее сцепление ({})", format!("{t:.2}").replace('.', ",")));
                    } else if k < -0.3 {
                        minus.push(format!("слабое сцепление ({}) — на {} скользко", format!("{t:.2}").replace('.', ","), if surface == "wet" { "мокром" } else { "снегу" }));
                    }
                }
                if has("trail") && surface != "wet" {
                    score += 6.0;
                    plus.push("протектор трейловой пары держит снег лучше".into());
                }
            }
            _ => {}
        }
        // 0.23.1: membrane (Gore-Tex and similar). Unset → guessed from the model name (GTX).
        let nm = s["name"].as_str().unwrap_or("").to_lowercase();
        let membrane = sp["membrane"].as_bool().unwrap_or(nm.contains("gtx") || nm.contains("gore-tex") || nm.contains("goretex"));
        if membrane {
            any_membrane = true;
            match surface.as_str() {
                "wet" => { score += 8.0; plus.push("мембрана (Gore-Tex) — ноги останутся сухими".into()) }
                "snow" => { score += 12.0; plus.push("мембрана (Gore-Tex) — снег не промочит ноги".into()) }
                "ice" => { score += 5.0; plus.push("мембрана — ноги в тепле и сухие".into()) }
                _ => {}
            }
            if hot {
                score -= 10.0;
                minus.push("мембрана — в жару ноге будет жарко и влажно".into());
            } else if temp == "10..20" && long_run {
                score -= 4.0;
                minus.push("мембрана — на долгой пробежке в тепло ноге жарко".into());
            } else if cold && surface != "wet" && surface != "snow" && surface != "ice" {
                score += 3.0;
                plus.push("мембрана не продувается — теплее в мороз".into());
            }
        }
        if cold {
            if let Some(c) = f(sp, "coldPct") {
                if c >= 20.0 {
                    score -= if temp == "lt-10" { 12.0 } else { 7.0 };
                    minus.push(format!("пена на холоде твердеет на {}%", c.round()));
                } else if c <= 12.0 {
                    score += 3.0;
                    plus.push("пена почти не дубеет на холоде".into());
                }
            }
            if sp["season"] == "summer" {
                score -= 6.0;
                minus.push("летняя модель".into());
            }
        }
        if hot {
            if let Some(b) = f(sp, "breath") {
                score += (b - 3.0) * 3.0;
                if b >= 4.0 {
                    plus.push("хорошо проветривается".into());
                }
            }
            if sp["season"] == "winter" {
                score -= 5.0;
            }
        }
        // Wear.
        let km = s["km"].as_f64().unwrap_or(0.0);
        let limit = s["limitKm"].as_f64().unwrap_or(700.0).max(1.0);
        let frac = km / limit;
        if frac >= 1.0 {
            score -= 20.0;
            minus.push(format!("ресурс выработан ({} из ≈{} км) — пора менять", km.round(), limit.round()));
        } else if frac >= 0.9 {
            score -= if nd == "easy" { 4.0 } else { 12.0 };
            minus.push(format!("пара почти выработана ({} из ≈{} км) — только лёгкие пробежки", km.round(), limit.round()));
        }
        // Rotation: give the foam a rest after yesterday.
        if let Some(t) = last_used(conn, s["id"].as_str().unwrap_or("")) {
            let h = (now - t) as f64 / 3600.0;
            if h < 30.0 && active.len() > 1 {
                score -= 8.0;
                minus.push("бегал в ней меньше суток назад — пусть пена отдохнёт".into());
            }
        }
        let filled = sp.as_object().map(|o| o.len()).unwrap_or(0);
        picks.push((score, json!({
            "id": s["id"], "name": s["name"], "score": score.round(), "plus": plus, "minus": minus, "km": km, "limitKm": limit, "spec": filled > 0,
        })));
    }
    picks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut warn: Vec<String> = Vec::new();
    if surface == "ice" {
        warn.push("На льду в обычных кроссовках легко упасть: лучше с шипами или накладками — или перенести бег на дорожку.".into());
    }
    if any_membrane && matches!(surface.as_str(), "wet" | "snow") {
        warn.push("Мембрана не спасает, если вода заливается сверху — в глубокие лужи или снег выше щиколотки она удержит воду внутри. Помогут высокие носки или гетры.".into());
    }
    if temp == "lt-10" && matches!(nd, "tempo" | "race") {
        warn.push("В сильный мороз интенсивную работу лучше делать в зале или сократить — дыхательным путям тяжело.".into());
    }
    Ok(json!({
        "need": {"kind": nd, "title": title, "min": min}, "shoes": active.len(), "conditions": cond, "warn": warn,
        "picks": picks.into_iter().map(|x| x.1).collect::<Vec<_>>(),
    }))
}
