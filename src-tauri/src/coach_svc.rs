//! «Тренер», part 1. Reports are written by Kouros itself from numbers the code already computed
//! (no network): «Как мой день?» at the moment you ask, and the week analysis over a rolling period.
//! For a live talk, «Спросить Claude» packs the same facts as text to paste into Claude's chat;
//! Claude's answer can be pasted back and is kept in the history.

use crate::engine::{add_days, day_num, mean};
use crate::{db, engine_svc, health_svc, meds_svc, nutrition_svc, psych_svc, store};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn f1(x: f64) -> String {
    format!("{:.1}", x).replace('.', ",")
}

fn f0(x: f64) -> String {
    format!("{:.0}", x)
}

fn hm(min: f64) -> String {
    let m = min.round() as i64;
    if m >= 60 { format!("{} ч {:02} мин", m / 60, m % 60) } else { format!("{m} мин") }
}

fn get_json(conn: &Connection, key: &str) -> Value {
    db::get_setting(conn, key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

// ---------- profile ----------

pub fn profile(conn: &Connection) -> Value {
    get_json(conn, "coach_profile")
}

pub fn save_profile(conn: &Connection, v: &Value) -> Result<(), String> {
    if !v.is_object() {
        return Err("Ожидались ответы знакомства".into());
    }
    db::set_setting(conn, "coach_profile", &v.to_string())
}

pub fn notes(conn: &Connection) -> String {
    db::get_setting(conn, "coach_notes").unwrap_or_default()
}

pub fn save_notes(conn: &Connection, text: &str) -> Result<(), String> {
    db::set_setting(conn, "coach_notes", text.trim())
}

const GOALS: [(&str, &str); 7] = [
    ("hm", "полумарафон быстрее"), ("mar", "первый марафон"), ("k10", "улучшить 5 и 10 км"), ("body", "снизить вес и улучшить состав тела"),
    ("health", "здоровье и общая форма"), ("str", "стать сильнее"), ("inj", "бегать без травм"),
];
const LIKES: [(&str, &str); 5] = [("run", "бег"), ("walk", "ходьба"), ("gym", "силовые в зале"), ("ofp", "укрепляющие упражнения"), ("bike", "велосипед")];
const MODES: [(&str, &str); 4] = [("build", "набор объёма"), ("maintain", "поддержание формы"), ("winter", "зимовка"), ("return", "возвращение после перерыва")];

/// The profile in plain words (for the reports and the Claude package).
pub fn profile_lines(p: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let names = |arr: &Value, table: &[(&str, &str)]| -> Vec<String> {
        arr.as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str()).map(|k| table.iter().find(|t| t.0 == k).map(|t| t.1.to_string()).unwrap_or(k.to_string())).collect())
            .unwrap_or_default()
    };
    let g = names(&p["goals"], &GOALS);
    if !g.is_empty() {
        out.push(format!("Цели: {}", g.join(", ")));
    }
    if let Some(m) = p["mode"].as_str() {
        out.push(format!("Режим: {}", MODES.iter().find(|x| x.0 == m).map(|x| x.1).unwrap_or(m)));
    }
    if let Some(w) = p["week"].as_object() {
        let parts: Vec<String> = w.iter().filter_map(|(k, v)| v.as_f64().filter(|x| *x > 0.0).map(|x| format!("{} {x:.0}", LIKES.iter().find(|t| t.0 == k).map(|t| t.1).unwrap_or(k)))).collect();
        if !parts.is_empty() {
            out.push(format!("Хочет в неделю (дней): {}", parts.join(", ")));
        }
    }
    for (k, t) in [("days", "Дни для тренировок"), ("time", "Время на тренировку"), ("when", "Удобное время"), ("where", "Где"), ("injuries", "Травмы и слабые места"), ("tools", "Средства восстановления"), ("other", "Ещё о себе")] {
        if let Some(a) = p[k].as_array() {
            let v: Vec<&str> = a.iter().filter_map(|x| x.as_str()).collect();
            if !v.is_empty() {
                out.push(format!("{t}: {}", v.join(", ")));
            }
        } else if let Some(s) = p[k].as_str() {
            out.push(format!("{t}: {s}"));
        }
    }
    if let Some(g) = p["goalRace"].as_object() {
        out.push(format!(
            "Цель-старт: {} {} км за {} — {}",
            g.get("title").and_then(|x| x.as_str()).unwrap_or(""),
            g.get("distanceKm").and_then(|x| x.as_f64()).map(|x| format!("{x}")).unwrap_or_default(),
            g.get("target").and_then(|x| x.as_str()).unwrap_or("?"),
            g.get("date").and_then(|x| x.as_str()).unwrap_or("дата не указана")
        ));
    }
    match p["style"].as_str() {
        Some("short") => out.push("Объяснения: коротко".into()),
        Some("long") => out.push("Объяснения: подробно".into()),
        _ => {}
    }
    out
}

// ---------- reports storage ----------

fn store_report(conn: &Connection, kind: &str, from: &str, to: &str, title: &str, body: &Value, data: &Value, parent: Option<i64>) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO coach_reports (kind, ts, period_from, period_to, title, body, data, parent_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![kind, store::now(), from, to, title, body.to_string(), data.to_string(), parent],
    )
    .map_err(err)?;
    Ok(conn.last_insert_rowid())
}

fn report_row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let body: String = r.get(6)?;
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "ts": r.get::<_, i64>(2)?,
        "from": r.get::<_, Option<String>>(3)?, "to": r.get::<_, Option<String>>(4)?, "title": r.get::<_, String>(5)?,
        "body": serde_json::from_str::<Value>(&body).unwrap_or(Value::Null), "parent": r.get::<_, Option<i64>>(7)?,
    }))
}

pub fn report(conn: &Connection, id: i64) -> Result<Value, String> {
    conn.query_row(
        "SELECT id, kind, ts, period_from, period_to, title, body, parent_id FROM coach_reports WHERE id = ?1",
        params![id],
        report_row,
    )
    .map_err(err)
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM coach_reports WHERE id = ?1 OR parent_id = ?1", params![id]).map_err(err)?;
    Ok(())
}

fn last_of(conn: &Connection, kind: &str) -> Option<(i64, i64, String, Value)> {
    conn.query_row(
        "SELECT id, ts, period_to, data FROM coach_reports WHERE kind = ?1 ORDER BY ts DESC LIMIT 1",
        params![kind],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default(), r.get::<_, String>(3)?)),
    )
    .ok()
    .map(|(id, ts, to, d)| (id, ts, to, serde_json::from_str(&d).unwrap_or(Value::Null)))
}

/// When the next week analysis becomes available (7 days after the last one).
fn week_window(conn: &Connection, today: &str) -> (String, String, Option<i64>) {
    match last_of(conn, "week") {
        Some((_, ts, to, _)) => {
            let next = ts + 7 * 86400;
            let mut from = add_days(&to, 1);
            if from > today.to_string() {
                from = today.to_string();
            }
            if day_num(today).unwrap_or(0) - day_num(&from).unwrap_or(0) > 27 {
                from = add_days(today, -27);
            }
            (from, today.to_string(), Some(next))
        }
        None => (add_days(today, -6), today.to_string(), None),
    }
}

pub fn state(conn: &Connection) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let today = engine_svc::today(&p);
    let (wf, wt, next) = week_window(conn, &today);
    let now = store::now();
    let list: Vec<Value> = {
        let mut st = conn
            .prepare("SELECT id, kind, ts, period_from, period_to, title, body, parent_id FROM coach_reports ORDER BY ts DESC LIMIT 60")
            .map_err(err)?;
        let v = st.query_map([], report_row).map_err(err)?.flatten().collect();
        v
    };
    Ok(json!({
        "profile": profile(conn), "notes": notes(conn),
        "week": {"from": wf, "to": wt, "ready": next.map(|n| now >= n).unwrap_or(true), "nextTs": next},
        "reports": list,
        "rules": claude_rules(conn),
    }))
}

// ---------- day report ----------

struct Sec {
    key: &'static str,
    title: &'static str,
    level: &'static str,
    text: String,
    why: Option<String>,
    todo: Option<String>,
}

fn sec(key: &'static str, title: &'static str, level: &'static str, text: String) -> Sec {
    Sec { key, title, level, text, why: None, todo: None }
}

impl Sec {
    fn why(mut self, s: impl Into<String>) -> Self {
        self.why = Some(s.into());
        self
    }
    fn todo(mut self, s: impl Into<String>) -> Self {
        self.todo = Some(s.into());
        self
    }
    fn json(&self) -> Value {
        json!({"key": self.key, "title": self.title, "level": self.level, "text": self.text, "why": self.why, "todo": self.todo})
    }
}

/// «Как мой день?» — written now from the current numbers.
pub fn day(conn: &mut Connection) -> Result<Value, String> {
    let s = engine_svc::summary(conn)?;
    let h = health_svc::summary(conn)?;
    let n = nutrition_svc::summary(conn)?;
    let ps = psych_svc::summary(conn)?;
    let food = nutrition_svc::food7(conn);
    let today = s["today"].as_str().unwrap_or("").to_string();
    let md = meds_svc::summary(conn, &today, &food)?;
    let now = store::now();
    let tz = engine_svc::profile(conn).tz_offset_s;

    // Where in your day we are.
    let wake = h["hypnogram"]["end"].as_i64().filter(|_| h["hypnogram"]["date"].as_str() == Some(today.as_str()));
    let awake_h = wake.map(|w| (now - w) as f64 / 3600.0);
    let night = h["nightState"]["useYesterday"].as_bool().unwrap_or(false);
    let phase = if night {
        "night"
    } else if awake_h.map(|x| x < 3.0).unwrap_or(false) {
        "morning"
    } else if s["readiness"]["eveningOn"].as_bool().unwrap_or(false) {
        "evening"
    } else {
        "day"
    };
    let mut secs: Vec<Sec> = Vec::new();
    let mut facts: Vec<String> = Vec::new();
    let clock = |t: i64| {
        let m = (t + tz).rem_euclid(86400) / 60;
        format!("{:02}:{:02}", m / 60, m % 60)
    };
    facts.push(format!("Сейчас {} ({}), {}", clock(now), today, match phase {
        "morning" => "вскоре после пробуждения",
        "day" => "день",
        "evening" => "вечер",
        _ => "ночь, сон ещё не записан",
    }));
    if let (Some(w), Some(a)) = (wake, awake_h) {
        facts.push(format!("Проснулся в {}, на ногах {:.1} ч", clock(w), a).replace('.', ","));
    }

    // Sleep.
    let last_night = s["sleep"].as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
    let need = num(&h["need"]["totalMin"]).unwrap_or(480.0);
    let debt = num(&h["need"]["debtMin"]).unwrap_or(0.0);
    if let Some(t) = num(&last_night["totalS"]) {
        let min = t / 60.0;
        facts.push(format!("Сон ({}): {} при потребности {}, долг сна за неделю {}", last_night["date"].as_str().unwrap_or(""), hm(min), hm(need), hm(debt)));
        if phase != "night" {
            if min < need - 45.0 {
                secs.push(
                    sec("sleep", "Сон", "warn", format!("Спал {} — на {} меньше нужного.", hm(min), hm(need - min)))
                        .why("Недосып снижает HRV и восстановление мышц, а ощущение усилия на тренировке растёт — тот же темп даётся тяжелее.")
                        .todo(format!("Сегодня лучше лечь пораньше: план — {}. Если есть возможность, 20 минут дневного сна до 16:00.", h["plan"]["lights"].as_str().unwrap_or("по плану"))),
                );
            } else {
                secs.push(sec("sleep", "Сон", "ok", format!("Сон {} — достаточно.", hm(min))));
            }
        }
    }

    // Readiness and recovery.
    let ev_on = s["readiness"]["eveningOn"].as_bool().unwrap_or(false);
    let score = if ev_on { num(&s["readiness"]["evening"]["score"]) } else { num(&s["readiness"]["score"]) };
    let comps = s["readiness"]["components"].as_array().cloned().unwrap_or_default();
    let wsum: f64 = comps.iter().filter(|c| c["score"].is_number()).map(|c| c["weight"].as_f64().unwrap_or(0.0)).sum();
    let mut pts: Vec<(f64, String)> = comps
        .iter()
        .filter_map(|c| {
            let sc = c["score"].as_f64()?;
            let p = c["weight"].as_f64().unwrap_or(0.0) * (sc - 50.0) / wsum.max(0.01);
            Some((p, format!("{} ({})", c["label"].as_str().unwrap_or(""), c["note"].as_str().unwrap_or(""))))
        })
        .collect();
    pts.sort_by(|a, b| b.0.abs().partial_cmp(&a.0.abs()).unwrap_or(std::cmp::Ordering::Equal));
    let left = num(&s["recovery"]["leftH"]).unwrap_or(0.0);
    if !night {
        if let Some(sc) = score {
            facts.push(format!("Готовность {}: {}", if ev_on { "вечером" } else { "утром" }, f0(sc)));
            for (p, t) in pts.iter().take(4) {
                facts.push(format!("  {}: {}{} балла", t, if *p >= 0.0 { "+" } else { "−" }, f1(p.abs())));
            }
            let neg: Vec<&String> = pts.iter().filter(|x| x.0 <= -2.0).map(|x| &x.1).take(2).collect();
            let level = if sc >= 55.0 { "ok" } else if sc >= 35.0 { "warn" } else { "bad" };
            let mut t = format!("Готовность {} — {}.", f0(sc), if sc >= 67.0 { "выше обычного" } else if sc >= 45.0 { "обычный день" } else if sc >= 35.0 { "заметно ниже нормы" } else { "организму тяжело" });
            if !neg.is_empty() {
                t.push_str(&format!(" Тянет вниз: {}.", neg.iter().map(|x| x.as_str()).collect::<Vec<_>>().join("; ")));
            }
            let mut sct = sec("ready", "Готовность", level, t);
            if level != "ok" {
                sct = sct.why("Оценка ниже нормы значит, что нервная система и сердце ещё не восстановились — нагрузка сегодня даст меньше пользы и больше усталости.");
            }
            secs.push(sct);
        }
        facts.push(format!("Восстановление после тренировок: осталось {} ч", f0(left)));
        if left >= 6.0 {
            secs.push(sec("recovery", "Восстановление", "warn", format!("После последних тренировок восстановление ещё ≈{} ч.", f0(left)))
                .todo("Сегодня без тяжёлой работы на те же мышцы; лёгкое движение ускорит восстановление."));
        }
    }

    // Training plan and what's done.
    let adv = &s["day"]["advice"];
    let done = num(&s["day"]["todayLoad"]).unwrap_or(0.0);
    let lo = num(&s["day"]["target"]["lo"]).unwrap_or(0.0);
    let hi = num(&s["day"]["target"]["hi"]).unwrap_or(0.0);
    facts.push(format!("Совет дня: {} — {}", adv["title"].as_str().unwrap_or(""), adv["text"].as_str().unwrap_or("")));
    facts.push(format!("Нагрузка сегодня {} при цели {}–{}", f0(done), f0(lo), f0(hi)));
    if let Some(r) = adv["reasons"].as_array().filter(|r| !r.is_empty()) {
        facts.push(format!("Совет снижен: {}", r.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; ")));
    }
    if !night {
        let mut t = format!("{}. {}", adv["title"].as_str().unwrap_or(""), adv["text"].as_str().unwrap_or(""));
        if done >= lo && lo > 0.0 {
            t = format!("Норма дня набрана ({} из {}–{}). Дальше — только восстановление: прогулка, растяжка, сон.", f0(done), f0(lo), f0(hi));
        } else if done > 0.0 {
            t.push_str(&format!(" Уже сделано {} из {}–{}.", f0(done), f0(lo), f0(hi)));
        }
        let mut sct = sec("plan", "Тренировка", "info", t);
        if let Some(r) = adv["reasons"].as_array().filter(|r| !r.is_empty()) {
            sct = sct.why(format!("Совет мягче, чем позволяет тело: {}.", r.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; ")));
        }
        if let Some(hint) = adv["hint"].as_str() {
            sct = sct.todo(hint.to_string());
        }
        secs.push(sct);
    }

    // Injury risk and muscles.
    let ha = num(&s["load"]["acwr"]);
    let ma = num(&s["load"]["muscle"]["acwr"]);
    facts.push(format!("Неделя к обычной: сердце {}, мышцы {}", ha.map(f1).unwrap_or("—".into()), ma.map(f1).unwrap_or("—".into())));
    let over: Vec<String> = s["muscleMap"]
        .as_array()
        .map(|a| a.iter().filter(|g| g["state"] == "over").filter_map(|g| g["name"].as_str().map(|x| x.to_lowercase())).collect())
        .unwrap_or_default();
    if !over.is_empty() {
        facts.push(format!("Перегружены: {}", over.join(", ")));
    }
    let worst = ha.unwrap_or(0.0).max(ma.unwrap_or(0.0));
    if worst > 1.3 || !over.is_empty() {
        let mut t = String::new();
        if worst > 1.3 {
            t.push_str(&format!("Неделя тяжелее обычной в {} раза{}. ", f1(worst), if ma.unwrap_or(0.0) > ha.unwrap_or(0.0) { " по мышцам и связкам" } else { "" }));
        }
        if !over.is_empty() {
            t.push_str(&format!("Перегружены: {}.", over.join(", ")));
        }
        secs.push(sec("risk", "Риск травмы", if worst > 1.5 { "bad" } else { "warn" }, t.trim().to_string())
            .why("Травмы чаще всего случаются не от одной тяжёлой тренировки, а от резкого скачка нагрузки на неделе.")
            .todo("Ближайшие дни — по ровному, без спусков и интервалов; эти мышцы — растяжка и роллер."));
    }

    // Stress.
    if phase != "morning" {
        if let (Some(st), Some(nm)) = (num(&s["stress"]["today"]["score"]), num(&s["stress"]["today"]["norm"])) {
            facts.push(format!("Стресс днём {} (обычно {})", f0(st), f0(nm)));
            if st > nm + 8.0 {
                secs.push(sec("stress", "Стресс", "warn", format!("Стресс днём {} — выше твоего обычного ({}).", f0(st), f0(nm)))
                    .why("Высокий стресс держит пульс выше, а HRV ниже — ночью восстановление будет хуже, даже если тренировка лёгкая.")
                    .todo("10–15 минут спокойной прогулки без телефона или 5 минут медленного дыхания (вдох 4 с, выдох 6 с). Вечером — без экранов за час до сна."));
            } else {
                secs.push(sec("stress", "Стресс", "ok", format!("Стресс {} — как обычно.", f0(st))));
            }
        }
    }

    // Food and water — not in the morning.
    if phase == "day" || phase == "evening" || phase == "night" {
        let e = &n["eaten"];
        let t = &n["targets"];
        let hours = awake_h.unwrap_or(10.0);
        let expect = if phase == "day" { ((hours - 1.0) / 14.0).clamp(0.1, 1.0) * 0.9 } else { 0.85 };
        let kcal = num(&e["kcal"]);
        let kt = num(&t["kcal"]).unwrap_or(2500.0);
        let prot = num(&e["protein"]).unwrap_or(0.0);
        let pt = num(&t["protein"]).unwrap_or(150.0);
        let water = num(&e["waterMl"]).unwrap_or(0.0);
        let wt = num(&t["waterMl"]).unwrap_or(3000.0);
        facts.push(format!("Съедено {} ккал из {}, белок {} г из {}, вода {} л из {}", kcal.map(f0).unwrap_or("—".into()), f0(kt), f0(prot), f0(pt), f1(water / 1000.0), f1(wt / 1000.0)));
        match kcal {
            None => secs.push(sec("food", "Еда", "info", "Записей в YAZIO сегодня пока нет — если ел, внеси, иначе отчёт не увидит еду.".into())),
            Some(k) if k < kt * expect * 0.6 => secs.push(
                sec("food", "Еда", "warn", format!("Съедено {} ккал из {} — для этого часа маловато.", f0(k), f0(kt)))
                    .why("Недоедание в день нагрузки замедляет восстановление и мешает сну; к вечеру растёт тяга к перекусам.")
                    .todo(format!("Полноценный приём пищи: белок ({} г осталось) и углеводы.", f0((pt - prot).max(0.0)))),
            ),
            Some(k) if phase != "day" && k > kt * 1.15 => secs.push(
                sec("food", "Еда", "warn", format!("Съедено {} ккал — на {} больше цели.", f0(k), f0(k - kt)))
                    .todo("Поздно вечером лучше без тяжёлой еды — она мешает глубокому сну."),
            ),
            Some(k) => {
                let mut sc = sec("food", "Еда", "ok", format!("Питание по плану: {} ккал из {}.", f0(k), f0(kt)));
                if prot < pt * expect * 0.7 {
                    sc = sec("food", "Еда", "warn", format!("Калорий хватает, но белка {} г из {}.", f0(prot), f0(pt)))
                        .why("Белок нужен мышцам для восстановления после нагрузки; лучше распределять его на 3–4 приёма.")
                        .todo("Добавь творог, яйца, мясо или рыбу в следующий приём.");
                }
                secs.push(sc);
            }
        }
        if water < wt * expect * 0.6 {
            secs.push(sec("water", "Вода", "warn", format!("Выпито {} л из {} — мало для этого часа.", f1(water / 1000.0), f1(wt / 1000.0)))
                .why("Даже небольшое обезвоживание поднимает пульс и ощущение усилия, хуже работает голова и терморегуляция.")
                .todo("Выпей 0,5 л в ближайший час и держи бутылку на виду."));
        } else {
            secs.push(sec("water", "Вода", "ok", format!("Воды {} л — нормально.", f1(water / 1000.0))));
        }
        if phase != "day" {
            let low: Vec<String> = n["micro"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|m| !m["noData"].as_bool().unwrap_or(false))
                        .filter(|m| m["pctAll"].as_f64().or(m["pctEst"].as_f64()).or(m["pct"].as_f64()).map(|p| p < 50.0).unwrap_or(false))
                        .filter(|m| m["key"] != "dietary_sodium_total")
                        .filter_map(|m| m["name"].as_str().map(String::from))
                        .take(4)
                        .collect()
                })
                .unwrap_or_default();
            if !low.is_empty() {
                facts.push(format!("За неделю мало: {}", low.join(", ")));
                secs.push(sec("micro", "Витамины и минералы", "warn", format!("За неделю стабильно мало: {}.", low.join(", ")))
                    .why("Это средние за 7 дней, а не за один день — значит, это привычка рациона.")
                    .todo("Загляни в «Питание» → микронутриенты: там видно, каких продуктов не хватает."));
            }
        }
    }

    // Supplements.
    let items = md["items"].as_array().cloned().unwrap_or_default();
    let missed: Vec<String> = items
        .iter()
        .filter(|m| m["active"].as_bool().unwrap_or(false) && m["schedule"] == "daily" && m["today"].as_f64().unwrap_or(0.0) <= 0.0)
        .filter_map(|m| m["name"].as_str().map(String::from))
        .collect();
    let taken: Vec<String> = items.iter().filter(|m| m["today"].as_f64().unwrap_or(0.0) > 0.0).filter_map(|m| m["name"].as_str().map(String::from)).collect();
    if !taken.is_empty() {
        facts.push(format!("Принято сегодня: {}", taken.join(", ")));
    }
    for w in md["warnings"].as_array().cloned().unwrap_or_default() {
        let key = w["key"].as_str().unwrap_or("");
        let micro = n["micro"].as_array().and_then(|a| a.iter().find(|m| m["key"].as_str() == Some(key)).cloned()).unwrap_or(Value::Null);
        let name = micro["name"].as_str().map(String::from).unwrap_or_else(|| key.replace("dietary_", "").replace("_total", ""));
        let unit = micro["unit"].as_str().unwrap_or("");
        facts.push(format!("Выше верхнего предела за 7 дней: {} — {} {} в день (из добавок {}) при пределе {}", name, w["avg"], unit, w["supp"], w["limit"]));
        secs.push(sec("supp", "Добавки", "bad", format!("{}: в среднем {} {} в день при безопасном пределе {}.", name, w["avg"], unit, w["limit"]))
            .why("Жирорастворимые витамины и минералы накапливаются; долго выше предела — риск для печени, почек или баланса других минералов.")
            .todo("Проверь дозировку на банке и состав всех добавок вместе; обсуди с врачом, нужна ли такая доза."));
    }
    // Medicines that matter for sport, timing of intake (0.18).
    for n in md["notes"].as_array().cloned().unwrap_or_default() {
        let text = n["text"].as_str().unwrap_or("").to_string();
        facts.push(format!("Лекарство: {text}"));
        let lv = if n["level"] == "warn" { "warn" } else { "info" };
        secs.push(sec("medgroup", "Лекарства и тренировки", lv, text).todo("Это не замена совету врача: дозу и схему не меняй сам, вопросы — врачу."));
    }
    for t in md["timing"].as_array().cloned().unwrap_or_default().iter().filter(|t| t["today"].as_bool().unwrap_or(false)) {
        secs.push(sec("medtime", "Время приёма", "info", t["text"].as_str().unwrap_or("").to_string()));
    }
    if !missed.is_empty() && phase != "morning" {
        secs.push(sec("meds", "Витамины и лекарства", "info", format!("Ещё не отмечено сегодня: {}.", missed.join(", ")))
            .todo("Если уже принял — отметь в «Здоровье» → «Витамины и лекарства» или на «Сегодня»."));
    }

    // Feelings.
    let tqr_today = ps["tqr"].as_array().and_then(|a| a.iter().rev().find(|x| x["date"].as_str() == Some(today.as_str()))).and_then(|x| x["score"].as_f64());
    let hooper_today = ps["hooper"].as_array().and_then(|a| a.iter().rev().find(|x| x["date"].as_str() == Some(today.as_str()))).and_then(|x| x["extra"]["feel"].as_f64());
    if let Some(t) = tqr_today {
        facts.push(format!("TQR сегодня {} из 20", f0(t)));
    }
    if let Some(f) = hooper_today {
        facts.push(format!("Самочувствие (Hooper) {} из 100", f0(f)));
    }
    if tqr_today.map(|t| t <= 11.0).unwrap_or(false) || hooper_today.map(|f| f < 40.0).unwrap_or(false) {
        secs.push(sec("feel", "Как ты себя чувствуешь", "warn", "По твоей самооценке восстановление ниже нормы.".into())
            .why("Ощущения часто опережают пульс и HRV на день-два — это ранний сигнал усталости.")
            .todo("Сегодня без интенсива; если так и завтра — день отдыха."));
    }

    // Soreness / pain from the morning Hooper (4th answer, 1–7): don't push through it, offer what still works.
    let sore = ps["hooper"].as_array().and_then(|a| a.iter().rev().find(|x| x["date"].as_str() == Some(today.as_str()))).and_then(|x| x["answers"][3].as_f64());
    if let Some(sv) = sore {
        facts.push(format!("Мышечная боль (Hooper) {} из 7", f0(sv)));
        if sv >= 5.0 && !night {
            let legs = over.iter().any(|g| ["квадрицепсы", "колени", "икры", "ахилл", "задняя поверхность бедра", "ягодицы"].iter().any(|k| g.contains(k)));
            let alt = if legs || over.is_empty() {
                "Вместо бега — велосипед или ходьба по ровному 30–45 минут, силовая на верх тела и кор, мобилити. Так форма сохраняется, а ноги восстанавливаются."
            } else {
                "Нагружай то, что не болит: если болят ноги — верх тела и кор, если верх — лёгкая аэробная работа ногами. Плюс мобилити 15 минут."
            };
            secs.push(sec("pain", "Боль в мышцах", if sv >= 6.0 { "bad" } else { "warn" }, format!("Утром отметил боль в мышцах {} из 7.", f0(sv)))
                .why("Через боль тренироваться не стоит: мышца не готова к нагрузке, техника меняется, и растёт риск травмы. Но и полный отдых не обязателен.")
                .todo(alt));
        }
    }

    // Signals from the engine.
    for g in s["signals"].as_array().cloned().unwrap_or_default().iter().filter(|g| g["level"] == "warn") {
        if let Some(t) = g["text"].as_str() {
            facts.push(format!("Сигнал: {t}"));
            if !secs.iter().any(|x| x.text.contains(t)) && g["code"] != "rhr_up" && g["code"] != "hrv_down" {
                secs.push(sec("signal", "Обрати внимание", "warn", t.to_string()));
            }
        }
    }

    // Evening: sleep plan.
    if phase == "evening" || phase == "night" {
        let lights = h["plan"]["lights"].as_str().unwrap_or("");
        let mut t = format!("Свет выключить в {} — так выйдет {} сна.", lights, hm(need));
        let mut sc = sec("tonight", "Сегодня ночью", "info", String::new());
        if let Some(sg) = h["suggestion"].as_object() {
            if let Some(m) = sg.get("min").and_then(|x| x.as_f64()) {
                t = format!("Организму нужно на {} больше сна: лечь в {} или встать в {}.", hm(m), sg.get("earlierBed").and_then(|x| x.as_str()).unwrap_or(lights), sg.get("laterWake").and_then(|x| x.as_str()).unwrap_or(""));
                let why: Vec<String> = sg.get("reasons").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                if !why.is_empty() {
                    sc = sc.why(format!("Почему: {}.", why.join("; ")));
                }
            }
        }
        sc.text = t;
        facts.push(format!("План сна: {}", sc.text));
        secs.push(sc);
    }

    // Fresh lab results (0.20): out-of-range findings of samples from the last 14 days.
    for f in crate::labs_svc::findings(conn, &today, 14).iter().filter(|f| f["level"] != "info").take(2) {
        let t = f["title"].as_str().unwrap_or("").to_string();
        facts.push(format!("Анализы: {t}"));
        secs.push(sec("lab", "Анализы", if f["level"] == "bad" { "bad" } else { "warn" }, t).why(f["text"].as_str().unwrap_or("").to_string()).todo(f["todo"].as_str().unwrap_or("").to_string()));
    }

    // Pain, return to run, recovery tools (0.16).
    for (_, name, level, runs, _) in crate::body_svc::active(conn) {
        facts.push(format!("Боль: {name} {level}/10"));
        if level >= 4 {
            secs.push(sec("pain", "Боль", if level >= 6 { "bad" } else { "warn" }, format!("{name}: {level}/10.")).why("Через боль тренироваться не стоит — меняется техника и растёт риск настоящей травмы.").todo(
                if runs { "Вместо бега — велосипед или верх тела и кор; в «Плане» Kouros уже предложил замену. Если держится больше недели — начни «Возврат к бегу» в разделе «Боль и восстановление» и покажись врачу." }
                else { "Силовые — без нагрузки на это место; бег и ходьба — можно, если не усиливают боль." },
            ));
        }
    }
    let ret = crate::body_svc::return_state(conn);
    if ret["active"].as_bool().unwrap_or(false) {
        let t = format!("Шаг {} из {}: {}.", ret["stage"].as_i64().unwrap_or(0) + 1, crate::body_svc::STEPS.len(), ret["stageText"].as_str().unwrap_or(""));
        facts.push(format!("Возврат к бегу: {t}"));
        secs.push(sec("return", "Возврат к бегу", "info", t).todo("После пробежки отметь боль, а утром — как себя чувствует это место: от этого зависит следующий шаг."));
    }
    if phase != "morning" && phase != "night" {
        let tl = crate::body_svc::tools(conn, &s);
        let best: Vec<String> = tl["best"].as_array().map(|a| a.iter().filter_map(|x| Some(format!("{} — {}", x["name"].as_str()?, x["text"].as_str()?))).collect()).unwrap_or_default();
        if !best.is_empty() {
            facts.push(format!("Восстановление сегодня: {}", best.join("; ")));
            secs.push(sec("tools", "Восстановление сегодня", "info", best.join(" ")));
        }
    }
    let pend = crate::body_svc::pending(conn);
    if !pend.is_empty() {
        secs.push(sec("checkin", "Как прошла тренировка", "info", "Ты ещё не отметил, как прошла последняя тренировка.".into()).todo("На «Сегодня» — карточка «Как прошла тренировка?»: 10 секунд, и нагрузка с восстановлением станут точнее."));
    }

    // Headline.
    let warns = secs.iter().filter(|x| x.level == "warn" || x.level == "bad").count();
    let headline = match phase {
        "night" => format!("Ночь — главное сейчас сон. {}", secs.iter().find(|x| x.key == "tonight").map(|x| x.text.clone()).unwrap_or_default()),
        _ => {
            let a = adv["title"].as_str().unwrap_or("");
            let r = score.map(|x| format!("Готовность {}", f0(x))).unwrap_or_default();
            let w = if warns == 0 { "Всё в порядке.".to_string() } else { format!("Обрати внимание: {}.", secs.iter().filter(|x| x.level != "ok" && x.level != "info").map(|x| x.title.to_lowercase()).collect::<Vec<_>>().join(", ")) };
            format!("{r} · {a}. {w}").trim_start_matches(" · ").to_string()
        }
    };
    let body = json!({"phase": phase, "headline": headline, "sections": secs.iter().map(|x| x.json()).collect::<Vec<_>>()});
    let data = json!({"facts": facts});
    let title = format!("Как мой день · {}", clock(now));
    let id = store_report(conn, "day", &today, &today, &title, &body, &data, None)?;
    report(conn, id)
}

// ---------- week analysis ----------

fn avg_of(rows: &[(String, f64)], from: &str, to: &str) -> Option<f64> {
    mean(&rows.iter().filter(|(d, _)| d.as_str() >= from && d.as_str() <= to).map(|x| x.1).collect::<Vec<_>>())
}

pub fn week(conn: &mut Connection) -> Result<Value, String> {
    let p = engine_svc::profile(conn);
    let today = engine_svc::today(&p);
    let (from, to, next) = week_window(conn, &today);
    if let Some(nx) = next {
        if store::now() < nx {
            return Err("Следующий анализ станет доступен через 7 дней после прошлого".into());
        }
    }
    let s = engine_svc::summary(conn)?;
    let h = health_svc::summary(conn)?;
    let n = nutrition_svc::summary(conn)?;
    let ps = psych_svc::summary(conn)?;
    let days = (day_num(&to).unwrap_or(0) - day_num(&from).unwrap_or(0) + 1).max(1) as f64;
    let wk = 7.0 / days;
    let in_p = |d: &str| d >= from.as_str() && d <= to.as_str();

    // Loads.
    let hist = s["load"]["history"].as_array().cloned().unwrap_or_default();
    let mut heart = 0.0;
    let mut musc = 0.0;
    let mut usual_h = Vec::new();
    let mut usual_m = Vec::new();
    for r in &hist {
        let d = r[0].as_str().unwrap_or("");
        if in_p(d) {
            heart += r[1].as_f64().unwrap_or(0.0);
            musc += r[2].as_f64().unwrap_or(0.0);
            if let Some(x) = r[5].as_f64() {
                usual_h.push(x);
            }
            if let Some(x) = r[6].as_f64() {
                usual_m.push(x);
            }
        }
    }
    let uh = mean(&usual_h).unwrap_or(0.0);
    let um = mean(&usual_m).unwrap_or(0.0);
    let acts: Vec<Value> = s["load"]["acts"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|a| {
            let d = crate::engine::day_of(a["start"].as_i64().unwrap_or(0), p.tz_offset_s);
            in_p(&d)
        })
        .collect();
    let runs = acts.iter().filter(|a| a["sport"] == "run").count();
    let strength = acts.iter().filter(|a| a["sport"] == "strength").count();
    let walks = acts.iter().filter(|a| a["sport"] == "walk").count();
    let bad_alt = acts.iter().filter(|a| a["bad"].as_bool().unwrap_or(false)).count();

    // Health.
    let ready: Vec<(String, f64)> = s["readiness"]["history"].as_array().map(|a| a.iter().filter_map(|x| Some((x[0].as_str()?.to_string(), x[1].as_f64()?))).collect()).unwrap_or_default();
    let stress: Vec<(String, f64)> = h["stress"]["series"].as_array().map(|a| a.iter().filter_map(|x| Some((x[0].as_str()?.to_string(), x[1].as_f64()?))).collect()).unwrap_or_default();
    let sleep: Vec<(String, f64)> = h["nights"].as_array().map(|a| a.iter().filter_map(|x| Some((x["date"].as_str()?.to_string(), x["totalS"].as_f64()? / 3600.0))).collect()).unwrap_or_default();
    let need_h = num(&h["need"]["totalMin"]).unwrap_or(480.0) / 60.0;
    let r_avg = avg_of(&ready, &from, &to);
    let st_avg = avg_of(&stress, &from, &to);
    let st_norm = num(&h["stress"]["norm"]);
    let sl_avg = avg_of(&sleep, &from, &to);

    // Food.
    let ndays: Vec<Value> = n["days"].as_array().cloned().unwrap_or_default().into_iter().filter(|d| d["date"].as_str().map(|x| in_p(x)).unwrap_or(false) && d["status"] == 2).collect();
    let fav = |k: &str| mean(&ndays.iter().filter_map(|d| d[k].as_f64()).collect::<Vec<_>>());
    let ftg = |k: &str| mean(&ndays.iter().filter_map(|d| d["target"][k].as_f64()).collect::<Vec<_>>());
    let kcal = fav("kcal");
    let kcal_t = ftg("kcal");
    let prot = fav("protein");
    let prot_t = ftg("protein");
    let water = fav("waterMl");
    let water_t = ftg("waterMl");
    let fiber = mean(&ndays.iter().filter_map(|d| d["fiber"].as_f64().map(|f| f + d["fiberEst"].as_f64().unwrap_or(0.0))).collect::<Vec<_>>());
    let fiber_t = ftg("fiber");

    // Supplements adherence (daily ones).
    let adherence: Option<f64> = if !meds_svc::enabled(conn) {
        None
    } else {
        let mut st = conn
            .prepare("SELECT m.id, (SELECT count(DISTINCT l.date) FROM med_log l WHERE l.med_id = m.id AND l.date BETWEEN ?1 AND ?2) FROM meds m WHERE m.archived = 0 AND m.schedule = 'daily'")
            .map_err(err)?;
        let v: Vec<f64> = st.query_map(params![from, to], |r| r.get::<_, i64>(1)).map_err(err)?.flatten().map(|c| c as f64 / days).collect();
        mean(&v)
    };
    let tqr = mean(&ps["tqr"].as_array().map(|a| a.iter().filter(|x| x["date"].as_str().map(|d| in_p(d)).unwrap_or(false)).filter_map(|x| x["score"].as_f64()).collect::<Vec<_>>()).unwrap_or_default());

    // Recommendations: (code, text) — conditions checked again next time.
    let mut recs: Vec<(String, String)> = Vec::new();
    let hw = heart * wk;
    let mw = musc * wk;
    if uh > 0.0 && hw < uh * 0.8 {
        recs.push(("load_low".into(), format!("Нагрузка на сердце {}% от обычной недели — если готовность позволяет, добавь одну лёгкую аэробную тренировку 40–50 минут.", f0(hw / uh * 100.0))));
    }
    if uh > 0.0 && hw > uh * 1.3 {
        recs.push(("load_high".into(), format!("Нагрузка {}% от обычной — следующая неделя полегче: минус 20–30% объёма, без новых интервалов.", f0(hw / uh * 100.0))));
    }
    if um > 0.0 && mw > um * 1.3 {
        recs.push(("muscle_high".into(), "Мышцам и связкам досталось больше обычного — меньше спусков и холмов, больше ровного; растяжка и роллер после бега.".into()));
    }
    if let Some(sl) = sl_avg {
        if sl < need_h - 0.5 {
            recs.push(("sleep_short".into(), format!("Сон в среднем {} при потребности {} — это главный резерв восстановления. Сдвинь отбой на 30–45 минут раньше.", hm(sl * 60.0), hm(need_h * 60.0))));
        }
    }
    if let (Some(pv), Some(ptg)) = (prot, prot_t) {
        if pv < ptg * 0.9 {
            recs.push(("protein_low".into(), format!("Белок {} г при цели {} — добавь белковый продукт в 1–2 приёма.", f0(pv), f0(ptg))));
        }
    }
    if let (Some(w), Some(wt)) = (water, water_t) {
        if w < wt * 0.75 {
            recs.push(("water_low".into(), format!("Воды в среднем {} л при цели {} — держи бутылку на столе и пей между делами.", f1(w / 1000.0), f1(wt / 1000.0))));
        }
    }
    if let (Some(fb), Some(ft)) = (fiber, fiber_t) {
        if fb < ft * 0.7 {
            recs.push(("fiber_low".into(), format!("Клетчатки ≈{} г при цели {} — больше овощей, фруктов, круп и бобовых.", f0(fb), f0(ft))));
        }
    }
    if let Some(a) = adherence {
        if a < 0.8 {
            recs.push(("meds_skip".into(), format!("Ежедневные добавки отмечены в {}% дней — либо пропуски, либо забываешь отмечать.", f0(a * 100.0))));
        }
    }
    if let (Some(sa), Some(nm)) = (st_avg, st_norm) {
        if sa > nm + 6.0 {
            recs.push(("stress_high".into(), format!("Стресс днём выше твоего обычного ({} против {}) — запланируй 2–3 спокойных вечера и короткие паузы днём.", f0(sa), f0(nm))));
        }
    }
    if let Some(t) = tqr {
        if t <= 12.0 {
            recs.push(("tqr_low".into(), format!("По TQR восстановление в среднем {} из 20 — ощущения говорят об усталости, даже если цифры тела в норме.", f1(t))));
        }
    }

    // Possible deficits and lab notes (0.20).
    for d in n["deficits"]["items"].as_array().cloned().unwrap_or_default().iter().filter(|d| d["level"] != "info").take(3) {
        let key = d["key"].as_str().unwrap_or("").replace("dietary_", "").replace("_total", "");
        recs.push((format!("deficit_{key}"), format!("{} {} В еду: {}.", d["text"].as_str().unwrap_or(""), d["advice"].as_str().unwrap_or(""), d["foods"].as_str().unwrap_or("")).replace("  ", " ")));
    }
    for f in crate::labs_svc::findings(conn, &today, 30).iter().filter(|f| f["level"] != "info").take(3) {
        recs.push((format!("lab_{}", f["key"].as_str().unwrap_or("")), format!("Анализы: {} {}", f["title"].as_str().unwrap_or(""), f["todo"].as_str().unwrap_or(""))));
    }

    // What changed since the previous analysis.
    let prev = last_of(conn, "week");
    let mut follow: Vec<Value> = Vec::new();
    if let Some((_, _, _, d)) = &prev {
        for r in d["recs"].as_array().cloned().unwrap_or_default() {
            let code = r["code"].as_str().unwrap_or("");
            let still = recs.iter().any(|x| x.0 == code);
            follow.push(json!({"text": r["text"], "done": !still}));
        }
    }

    let mut lines: Vec<Value> = Vec::new();
    let line = |k: &str, v: String, note: String| json!({"k": k, "v": v, "note": note});
    lines.push(line("Нагрузка на сердце", format!("{} в неделю", f0(hw)), if uh > 0.0 { format!("обычно {}", f0(uh)) } else { String::new() }));
    lines.push(line("Мышцы и связки", format!("{} в неделю", f0(mw)), if um > 0.0 { format!("обычно {}", f0(um)) } else { String::new() }));
    lines.push(line("Тренировки", format!("бег {}, силовые {}, прогулки {}", runs, strength, walks), if days > 7.5 { format!("за {} дн.", f0(days)) } else { String::new() }));
    if let Some(r) = r_avg {
        lines.push(line("Готовность", format!("в среднем {}", f0(r)), String::new()));
    }
    if let Some(sl) = sl_avg {
        lines.push(line("Сон", hm(sl * 60.0), format!("нужно {}", hm(need_h * 60.0))));
    }
    if let Some(sa) = st_avg {
        lines.push(line("Стресс днём", f0(sa), st_norm.map(|x| format!("обычно {}", f0(x))).unwrap_or_default()));
    }
    if let (Some(k), Some(kt)) = (kcal, kcal_t) {
        lines.push(line("Калории", format!("{} в день", f0(k)), format!("цель {} · полных дней {}", f0(kt), ndays.len())));
    }
    if let Some(pv) = prot {
        lines.push(line("Белок", format!("{} г", f0(pv)), prot_t.map(|x| format!("цель {}", f0(x))).unwrap_or_default()));
    }
    if let Some(w) = water {
        lines.push(line("Вода", format!("{} л", f1(w / 1000.0)), water_t.map(|x| format!("цель {}", f1(x / 1000.0))).unwrap_or_default()));
    }
    if let Some(a) = adherence {
        lines.push(line("Добавки", format!("{}% дней", f0(a * 100.0)), "ежедневные".into()));
    }
    if let Some(t) = tqr {
        lines.push(line("TQR", f1(t), "из 20".into()));
    }
    let headline = if recs.is_empty() {
        format!("{} — ровная неделя, менять ничего не нужно.", if days > 7.5 { format!("{} дней", f0(days)) } else { "Неделя".into() })
    } else {
        format!("Главное на следующую неделю: {}", recs[0].1.split(" — ").next().unwrap_or(&recs[0].1))
    };
    let mut notes_v: Vec<String> = Vec::new();
    if days > 7.5 {
        notes_v.push(format!("Период длиннее недели ({} дн.) — нагрузки пересчитаны на 7 дней, чтобы сравнивать с обычной неделей.", f0(days)));
    }
    if bad_alt > 0 {
        notes_v.push(format!("В {} тренировках высота записана со сбоем — для мышц взят твой обычный рельеф.", bad_alt));
    }
    let body = json!({
        "headline": headline, "from": from, "to": to, "days": days,
        "lines": lines.clone(),
        "recs": recs.iter().map(|(c, t)| json!({"code": c, "text": t})).collect::<Vec<_>>(),
        "follow": follow, "notes": notes_v,
    });
    let mut facts: Vec<String> = vec![format!("Период: {} — {} ({} дн.)", from, to, f0(days))];
    for l in &lines {
        facts.push(format!("{}: {} {}", l["k"].as_str().unwrap_or(""), l["v"].as_str().unwrap_or(""), l["note"].as_str().unwrap_or("")).trim().to_string());
    }
    for a in &acts {
        let d = crate::engine::day_of(a["start"].as_i64().unwrap_or(0), p.tz_offset_s);
        facts.push(format!("  {} {} {}: сердце {}, мышцы {}{}", d, a["sport"].as_str().unwrap_or(""), a["name"].as_str().unwrap_or(""), a["heart"], a["muscle"],
            if a["up"].as_f64().unwrap_or(0.0) > 0.0 { format!(", ↑{} ↓{} м", a["up"], a["down"]) } else { String::new() }));
    }
    let data = json!({"recs": recs.iter().map(|(c, t)| json!({"code": c, "text": t})).collect::<Vec<_>>(), "facts": facts});
    let id = store_report(conn, "week", &from, &to, &format!("Анализ недели · {} — {}", from, to), &body, &data, None)?;
    report(conn, id)
}

// ---------- Claude package ----------

pub fn claude_rules(conn: &Connection) -> String {
    let who = match engine_svc::profile(conn).name {
        Some(n) => format!("Спортсмен — {n}"),
        None => "Спортсмен — владелец приложения".to_string(),
    };
    format!("Ты — тренер в приложении Kouros. {who}; данные о тренировках из Strava, о здоровье и питании — из Apple Health. ") + "\
Все цифры уже посчитаны приложением и приходят в сводке: опирайся только на них, новых цифр не придумывай; если чего-то нет в сводке — так и скажи.\n\n\
Тон: дружески и с заботой, на «ты», по-русски. Средняя длина. То, что в норме, — одной строкой. Подробно — только о том, что выбивается: \
объясни, почему это важно именно сегодня, и что конкретно сделать.\n\n\
Правила:\n\
- Не ставь диагнозов. Если признаки похожи на болезнь (пульс и температура выше нормы, HRV ниже) — лёгкий день и наблюдение, если держится несколько дней — к врачу.\n\
- Не уговаривай тренироваться через боль, но и не ограничивайся «отдыхай» — предложи альтернативу, которая не нагружает больное место (верх тела, кор, вело, мобилити, ходьба по ровному).\n\
- Добавки: если доза выше безопасного предела или добавка, похоже, не нужна — скажи об этом и посоветуй сверить с анализами или врачом. Дозы лекарств не меняй.\n\
- Учитывай цели, режим, травмы и средства восстановления из профиля.\n\
- Заканчивай одним-двумя главными советами.\n\n\
План: в сводке есть план тренировок на 7 дней. Если советуешь что-то в нём поменять, в самом конце ответа напиши по строке на каждый изменённый день, строго в таком виде:\n\
[ПЛАН] ср 22 окт: интервалы → лёгкий бег 40 мин\n\
[ПЛАН] сб 25 окт: длительный бег 75 мин + растяжка 15 мин\n\
После «→» — что сделать вместо: отдых, прогулка, растяжка, лёгкий бег, длительный бег, темповый бег, интервалы, силовая в зале, укрепляющие упражнения (ОФП), велосипед — с минутами. Несколько занятий в день — через «+». Kouros покажет эти строки спортсмену как предложения «Принять / Оставить как было». Если менять план не нужно — строк [ПЛАН] не пиши.\n\n\
Этикетка добавки: если спортсмен присылает фото банки и просит состав, ответь одной строкой строго в таком виде (количество — на одну порцию, как на этикетке):\n\
[ДОБАВКА] Maxler Vitamen | порция: 2 таблетки | Витамин D3 25 мкг; Витамин C 250 мг; Цинк 15 мг; Магний 100 мг\n\
Компоненты — через «;», у каждого число и единица (мг, мкг, г, МЕ, ккал). Kouros сам разберёт строку и покажет состав для проверки.\n\n\
Анализы: если спортсмен присылает бланк анализов (скан или фото), перепиши результаты по строке на показатель строго в таком виде, первой строкой — дата сдачи и лаборатория:\n\
[АНАЛИЗЫ] 14.10.2026, KDL\n\
[АНАЛИЗ] Ферритин | 45 | мкг/л | 30-400\n\
[АНАЛИЗ] Витамин D (25-OH) | 24,5 | нг/мл | 30-100\n\
Название, результат как в бланке (с «<» или «>», если есть), единицы, норма лаборатории — через «|». Kouros сам разберёт строки и покажет их для проверки. \
Когда разбираешь анализы, связывай их с нагрузкой и питанием из сводки (КФК, АСТ и креатинин растут после тяжёлых тренировок накануне), без диагнозов — «обрати внимание» и «обсуди с врачом».\n\n\
Лекарства: в сводке могут быть группы, важные для спорта (НПВП, фторхинолоны, бета-блокаторы и другие). Подсказывай «обрати внимание» и «обсуди с врачом», без диагнозов и без изменения доз. \
Корреляции «личного эффекта» — не доказательство."
}

/// Text to paste into Claude's chat: task, profile, notes and the facts of the report (or of today).
pub fn package(conn: &mut Connection, report_id: Option<i64>, question: Option<&str>) -> Result<String, String> {
    let (kind, facts, title) = match report_id {
        Some(id) => {
            let (k, t, d): (String, String, String) = conn
                .query_row("SELECT kind, title, data FROM coach_reports WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map_err(err)?;
            let v: Value = serde_json::from_str(&d).unwrap_or(Value::Null);
            (k, v["facts"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default(), t)
        }
        None => {
            // A free question: the latest day report if it's fresh (≤2 h), else a new one.
            let id = match last_of(conn, "day") {
                Some((id, ts, _, _)) if store::now() - ts <= 2 * 3600 => id,
                _ => day(conn)?["id"].as_i64().unwrap_or(0),
            };
            return package(conn, Some(id), question);
        }
    };
    let task = match (question, kind.as_str()) {
        (Some(q), _) if !q.trim().is_empty() => format!("Вопрос спортсмена: {}", q.trim()),
        (_, "week") => "Задача: разбери этот период — что получилось, что мешает восстановлению, что поменять на следующую неделю (2–4 конкретных изменения).".into(),
        _ => "Задача: коротко разбери мой день на этот момент и скажи главное, что сделать дальше.".into(),
    };
    let p = profile(conn);
    let mut out = vec![format!("[Kouros · {}]", title), task, String::new()];
    let pl = profile_lines(&p);
    if !pl.is_empty() {
        out.push("Профиль:".into());
        out.extend(pl.into_iter().map(|l| format!("- {l}")));
    }
    let nt = notes(conn);
    if !nt.is_empty() {
        out.push(format!("Заметки спортсмена: {nt}"));
    }
    out.push(String::new());
    out.push("Сводка Kouros (цифры посчитаны приложением):".into());
    out.extend(facts.into_iter().map(|f| format!("- {f}")));
    // Vitamins and medicines (0.17): what is taken and how regularly.
    let today_s = engine_svc::today(&engine_svc::profile(conn));
    let food = nutrition_svc::food7(conn);
    if let Ok(md) = meds_svc::summary(conn, &today_s, &food) {
        let mut ml: Vec<String> = Vec::new();
        for m in md["items"].as_array().cloned().unwrap_or_default().iter().filter(|m| m["active"].as_bool().unwrap_or(false)) {
            let days = m["days"].as_array().cloned().unwrap_or_default();
            let taken = days.iter().filter(|d| d[1].as_f64().unwrap_or(0.0) > 0.0).count();
            let comps: Vec<String> = m["components"]
                .as_array()
                .map(|a| a.iter().filter_map(|c| Some(format!("{} {} {}", c["name"].as_str()?, c["amount"], c["unit"].as_str().unwrap_or("")))).collect())
                .unwrap_or_default();
            ml.push(format!(
                "{} ({}, {}; {} × {} за приём){}{} — отмечено {} из {} дн.",
                m["name"].as_str().unwrap_or(""),
                if m["kind"] == "medicine" {
                    match (m["extra"]["inn"].as_str(), m["groupInfo"]["name"].as_str()) {
                        (Some(i), Some(g)) => format!("лекарство, МНН {i}, {g}"),
                        (Some(i), None) => format!("лекарство, МНН {i}"),
                        (None, Some(g)) => format!("лекарство, {g}"),
                        _ => "лекарство".to_string(),
                    }
                } else {
                    "добавка".to_string()
                },
                match m["schedule"].as_str() { Some("daily") => "каждый день", Some("course") => "курсом", _ => "по необходимости" },
                m["dose"], m["form"].as_str().unwrap_or("шт"),
                if comps.is_empty() { String::new() } else { format!(": {}", comps.join(", ")) },
                m["notes"].as_str().filter(|x| !x.is_empty()).map(|x| format!(" ({x})")).unwrap_or_default(),
                taken, days.len().max(1)
            ));
        }
        for n in md["notes"].as_array().cloned().unwrap_or_default() {
            ml.push(format!("Группа «{}»: {}", n["groupName"].as_str().unwrap_or(""), n["text"].as_str().unwrap_or("")));
        }
        for t in md["timing"].as_array().cloned().unwrap_or_default() {
            ml.push(format!("Время приёма ({} дн. из 7): {}", t["days"], t["text"].as_str().unwrap_or("")));
        }
        for m in md["items"].as_array().cloned().unwrap_or_default() {
            let e = &m["effect"]["nextMorning"];
            if e.is_object() {
                ml.push(format!("Личный эффект {} (корреляция, не доказательство): следующее утро с приёмом vs без ({} / {} дн.) — HRV {}%, пульс покоя {}, сон {} мин, готовность {}",
                    m["name"].as_str().unwrap_or(""), e["with"], e["without"], e["hrvPct"], e["rhr"], e["sleepMin"], e["ready"]));
            }
        }
        let unknown = md["unknown"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        if !unknown.is_empty() {
            ml.push(format!("Компоненты, которые Kouros не распознал (оцени сам): {unknown}"));
        }
        for w in md["warnings"].as_array().cloned().unwrap_or_default() {
            ml.push(format!("Выше верхнего предела за 7 дней: {} — {} при пределе {}", w["key"].as_str().unwrap_or("").replace("dietary_", "").replace("_total", ""), w["avg"], w["limit"]));
        }
        if !ml.is_empty() {
            out.push(String::new());
            out.push("Витамины и лекарства:".into());
            out.extend(ml.into_iter().map(|f| format!("- {f}")));
        }
    }
    let labs = crate::labs_svc::lines(conn, &today_s);
    if !labs.is_empty() {
        out.push(String::new());
        out.push("Анализы (последний результат каждого показателя; отклонения и всё из последней сдачи):".into());
        out.extend(labs.into_iter().map(|f| format!("- {f}")));
    }
    if let Ok(nv) = nutrition_svc::summary(conn) {
        let df: Vec<String> = nv["deficits"]["items"].as_array().map(|a| a.iter().filter_map(|d| Some(format!("{} {}", d["text"].as_str()?, d["advice"].as_str().unwrap_or("")))).collect()).unwrap_or_default();
        if !df.is_empty() {
            out.push(String::new());
            out.push("Возможная нехватка в питании (28 дней, еда + оценка + добавки):".into());
            out.extend(df.into_iter().map(|f| format!("- {f}")));
        }
    }
    let body = crate::body_svc::lines(conn);
    if !body.is_empty() {
        out.push(String::new());
        out.push("Тело и цель:".into());
        out.extend(body.into_iter().map(|f| format!("- {f}")));
    }
    let plan = crate::plan_svc::lines(conn);
    if !plan.is_empty() {
        out.push(String::new());
        out.push("План тренировок (прогулка каждый день — сверх плана):".into());
        out.extend(plan.into_iter().map(|f| format!("- {f}")));
    }
    Ok(out.join("\n"))
}

/// Saves Claude's answer pasted back from the chat.
pub fn save_answer(conn: &Connection, parent: Option<i64>, text: &str) -> Result<Value, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("Вставь ответ Claude".into());
    }
    let today = engine_svc::today(&engine_svc::profile(conn));
    // «[ПЛАН] …» lines become plan proposals (Принять / Оставить как было).
    let plan = crate::plan_svc::from_claude(conn, t);
    let id = store_report(conn, "claude", &today, &today, "Ответ Claude", &json!({"text": t, "plan": plan}), &Value::Null, parent)?;
    report(conn, id)
}
