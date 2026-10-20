"""Builds the Kouros catalogs (run in GitHub Actions, results committed to the branch):

- src-tauri/resources/catalog.json.gz — supplements by brand: NOW and Solgar from the NIH Dietary
  Supplement Label Database (DSLD, CC0), Maxler from maxler.com (Europe region).
- src-tauri/resources/drugs.json.gz — medicines registered in Russia: trade name → МНН, form,
  dose (the interchangeability list published on grls.rosminzdrav.ru).

Amounts are converted to Kouros units (the Apple Health metric units)."""
import gzip, json, os, re, subprocess, sys, time, urllib.error, urllib.parse, urllib.request
from datetime import date

subprocess.run([sys.executable, "-m", "pip", "install", "-q", "xlrd==2.0.1"], check=False)
UA = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36", "Accept-Language": "en"}
RES = os.path.join("..", "..", "src-tauri", "resources")
OUT = "out"
os.makedirs(RES, exist_ok=True)
os.makedirs(OUT, exist_ok=True)
LOG = []


def note(*a):
    s = " ".join(str(x) for x in a)
    print(s, flush=True)
    LOG.append(s)


def get(url, tries=4):
    for k in range(tries):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=120) as r:
                return r.read()
        except urllib.error.HTTPError as e:
            if e.code in (403, 404, 410):
                note("skip", e.code, url[:120])
                return None
            note("retry", k, url[:120], e)
            time.sleep(3 * (k + 1))
        except Exception as e:
            note("retry", k, url[:120], e)
            time.sleep(3 * (k + 1))
    return None


# ---------- nutrient mapping ----------
# group/name pattern → (key, unit, factor from the source unit)
NUT = [
    (r"^vitamin a\b|retinol|beta.?carotene", "dietary_vitamin_a_total", "мкг"),
    (r"^vitamin c\b|ascorbic", "dietary_vitamin_c_total", "мг"),
    (r"^vitamin d\b|^vit\.? d|cholecalciferol|vitamin d3|vitamin d-3|ergocalciferol", "dietary_vitamin_d_total", "мкг"),
    (r"^vitamin e\b|tocopher", "dietary_vitamin_e_total", "мг"),
    (r"^vitamin k\b|^vit\.? k|phylloquinone|menaquinone|vitamin k2|vitamin k1", "dietary_vitamin_k_total", "мкг"),
    (r"thiamin|vitamin b1\b|vitamin b-1\b", "dietary_thiamin_total", "мг"),
    (r"riboflavin|vitamin b2\b|vitamin b-2\b", "dietary_riboflavin_total", "мг"),
    (r"niacin|vitamin b3\b|nicotinamide|niacinamide", "dietary_niacin_total", "мг"),
    (r"vitamin b6|vitamin b-6|pyridox", "dietary_vitamin_b6_total", "мг"),
    (r"folate|folic|methylfolate|vitamin b9", "dietary_folate_total", "мкг"),
    (r"vitamin b12|vitamin b-12|cobalamin", "dietary_vitamin_b12_total", "мкг"),
    (r"biotin|vitamin b7\b|vitamin h\b", "dietary_biotin_total", "мкг"),
    (r"pantothen|vitamin b5\b|vitamin b-5\b", "dietary_pantothenic_acid_total", "мг"),
    (r"^calcium", "dietary_calcium_total", "мг"),
    (r"^iron\b", "dietary_iron_total", "мг"),
    (r"^magnesium", "dietary_magnesium_total", "мг"),
    (r"^zinc", "dietary_zinc_total", "мг"),
    (r"^potassium", "dietary_potassium_total", "мг"),
    (r"^selenium", "dietary_selenium_total", "мкг"),
    (r"^phosphorus", "dietary_phosphorus_total", "мг"),
    (r"^copper", "dietary_copper_total", "мг"),
    (r"^manganese", "dietary_manganese_total", "мг"),
    (r"^sodium|^salt$", "dietary_sodium_total", "мг"),
    (r"^iodine", "dietary_iodine_total", "мкг"),
    (r"^chromium", "dietary_chromium_total", "мкг"),
    (r"^choline", "choline", "мг"),
    (r"caffeine", "dietary_caffeine_total", "мг"),
    (r"^protein", "dietary_protein_total", "г"),
    (r"carbohydrate", "dietary_carbohydrates_total", "г"),
    (r"^total fat|^fats?\b", "dietary_fat_total_total", "г"),
    (r"^calories$|^energy", "dietary_energy_total", "ккал"),
    (r"^epa\b|eicosapentaenoic", "epa", "мг"),
    (r"^dha\b|docosahexaenoic", "dha", "мг"),
    (r"^creatine", "creatine", "мг"),
    (r"melatonin", "melatonin", "мг"),
]
UNIT_RU = {"mg": "мг", "mcg": "мкг", "µg": "мкг", "ug": "мкг", "g": "г", "kcal": "ккал", "calories": "ккал", "iu": "МЕ", "mg dfe": "мкг", "mcg dfe": "мкг", "mcg rae": "мкг"}
IU = {"dietary_vitamin_d_total": 1 / 40, "dietary_vitamin_a_total": 0.3, "dietary_vitamin_e_total": 0.67}


def to_key(name, group=""):
    for txt in (group or "", name or ""):
        t = txt.lower().strip()
        for pat, key, unit in NUT:
            if re.search(pat, t):
                return key, unit
    return None, None


def convert(key, unit_target, amount, unit):
    u = (unit or "").lower().strip()
    u = u.replace("µ", "mc").replace("μ", "mc")
    if "iu" in u.split() or u == "iu":
        f = IU.get(key)
        return (amount * f, unit_target) if f else (None, None)
    base = {"mcg": 0.001, "mg": 1.0, "g": 1000.0}
    if unit_target in ("мг", "мкг", "г"):
        tgt = {"мг": 1.0, "мкг": 0.001, "г": 1000.0}[unit_target]
        for k, v in base.items():
            if u.startswith(k):
                return amount * v / tgt, unit_target
        return None, None
    if unit_target == "ккал":
        if "kj" in u:
            return amount / 4.184, "ккал"
        return amount, "ккал"
    return amount, unit_target


def comp(name, amount, unit, group=""):
    key, ut = to_key(name, group)
    if key and amount is not None:
        a, u = convert(key, ut, float(amount), unit)
        if a is not None:
            return [key, name, round(a, 4), u]
    return ["", name, float(amount) if amount is not None else 0, UNIT_RU.get((unit or "").lower(), unit or "")]


def dedupe(comps):
    seen, out = set(), []
    for c in comps:
        if c[0] and c[0] in seen:
            continue
        seen.add(c[0])
        out.append(c)
    return out


# Per serving more than this is not a real label (a per-100 g column picked by mistake).
PLAUSIBLE = {"dietary_vitamin_c_total": 3000, "dietary_magnesium_total": 800, "dietary_iron_total": 100, "dietary_zinc_total": 100, "dietary_calcium_total": 2000,
             "dietary_vitamin_b6_total": 200, "dietary_vitamin_e_total": 1000, "dietary_sodium_total": 3000}


def plausible(comps):
    return all(c[2] <= PLAUSIBLE.get(c[0], 1e12) for c in comps)


def kind_of(name, ptype, comps):
    t = (name + " " + (ptype or "")).lower()
    keys = {c[0] for c in comps}
    if "protein" in t or "whey" in t or "casein" in t or ("dietary_protein_total" in keys and any(c[0] == "dietary_protein_total" and c[2] >= 10 for c in comps)):
        return "protein"
    n = (name or "").lower()
    if any(w in n for w in ("isotonic", "electrolyte", "hydration", "max motion")):
        return "isotonic"
    if any(w in t for w in ("vitamin", "mineral", "multi")):
        return "vitamin"
    if any(w in t for w in ("amino", "bcaa", "glutamine", "creatine", "carnitine", "arginine", "eaa")):
        return "amino"
    return "other"


# ---------- DSLD (NOW, Solgar) ----------
def dsld_brand(brand, exact):
    out = {}
    start = 0
    while True:
        raw = get(f"https://api.ods.od.nih.gov/dsld/v9/brand-products?q={urllib.parse.quote(brand)}&size=1000&from={start}")
        if not raw:
            break
        d = json.loads(raw)
        hits = d.get("hits", [])
        if not hits:
            break
        for h in hits:
            s = h["_source"]
            if s.get("brandName", "").strip().lower() not in exact:
                continue
            name = re.sub(r"\s+", " ", s.get("fullName", "")).strip()
            if not name:
                continue
            ss = (s.get("servingSizes") or [{}])[0]
            serving = f"{ss.get('minQuantity') or 1:g} {ss.get('unit') or ''}".strip()
            comps = []
            for r in s.get("ingredientRows") or []:
                q = (r.get("quantity") or [{}])[0]
                if q.get("quantity") in (None, 0):
                    continue
                comps.append(comp(r.get("name", ""), q.get("quantity"), q.get("unit"), r.get("ingredientGroup", "")))
            rec = {
                "id": f"dsld:{s['id']}", "b": brand.lower(), "n": name, "s": serving,
                "f": (s.get("physicalState") or {}).get("langualCodeDescription", ""),
                "t": kind_of(name, (s.get("productType") or {}).get("langualCodeDescription", ""), comps),
                "c": comps, "off": bool(s.get("offMarket")), "d": s.get("entryDate", ""),
            }
            key = name.lower()
            old = out.get(key)
            # Prefer products on the market, then the newest label.
            if old is None or (old["off"] and not rec["off"]) or (old["off"] == rec["off"] and rec["d"] > old["d"]):
                out[key] = rec
        start += len(hits)
        if start >= d.get("total", {}).get("value", 0):
            break
    recs = [r for r in out.values() if r["c"]]
    on = [r for r in recs if not r["off"]]
    note("DSLD", brand, "labels kept", len(recs), "on market", len(on))
    return recs


# ---------- Maxler (maxler.com, Europe) ----------
def maxler():
    raw = get("https://www.maxler.com/products.json?limit=250")
    recs = []
    if not raw:
        return recs
    products = json.loads(raw).get("products", [])
    for p in products:
        if p.get("product_type") in ("Bundle", "Gift Card", "") and "gift" in p.get("handle", ""):
            continue
        html = get(f"https://www.maxler.com/products/{p['handle']}") or b""
        rec = parse_maxler(p, html.decode("utf-8", "ignore"))
        if rec:
            for k in ("why",):
                pass
            recs.append(rec)
    for r in recs:
        # Effervescent tablets: the table is per 100 g with an unlabeled per-tablet column — not reliable yet.
        if r["c"] and "effervescent" in (r.get("s") or "").lower():
            note("  maxler effervescent (dropped):", r["n"])
            r["c"] = []
            r["why"] = "effervescent"
            continue
        if r["c"] and not plausible(r["c"]):
            note("  maxler implausible (dropped):", r["n"], [c for c in r["c"] if c[2] > PLAUSIBLE.get(c[0], 1e12)])
            r["c"] = []
            r["why"] = "implausible"
    good = [r for r in recs if r["c"]]
    note("Maxler products", len(products), "with composition", len(good))
    for r in recs:
        if not r["c"]:
            note("  maxler without composition:", r["n"], r.get("why", ""))
    with open(os.path.join(OUT, "maxler_sample.json"), "w") as f:
        json.dump(good[:40], f, ensure_ascii=False, indent=1)
    return good


VAL = re.compile(r"^(<\s*)?([\d]+(?:[.,]\d+)?)\s*(mg|µg|μg|mcg|g|kj|kcal|iu|ie|ml)(?![a-z])", re.I)


def tokens(html):
    html = re.sub(r"(?is)<(script|style|svg)[^>]*>.*?</\1>", " ", html)
    parts = [re.sub(r"\s+", " ", t).strip() for t in re.split(r"<[^>]+>", html)]
    import html as h
    return [h.unescape(t) for t in parts if t and t not in ("|",)]


def parse_maxler(p, html):
    t = tokens(html)
    try:
        i = next(k for k, x in enumerate(t) if x.upper() == "SUPPLEMENT FACTS" or x.upper() == "NUTRITION FACTS")
    except StopIteration:
        return {"id": f"maxler:{p['handle']}", "b": "maxler", "n": p["title"], "s": "", "f": "", "t": "other", "c": [], "why": "no facts"}
    t = t[i + 1:]
    # First variant block: up to the next «Serving size» after the first one, or the ingredients.
    serving = ""
    start = 0
    for k, x in enumerate(t):
        if x.lower().startswith("serving size") or x.lower().startswith("daily dose") or x.lower().startswith("portion"):
            serving = x.split(":", 1)[-1].strip()
            start = k + 1
            if not serving and k + 1 < len(t):
                serving = t[k + 1]
                start = k + 2
            break
    end = len(t)
    for k in range(start, len(t)):
        x = t[k].lower()
        if x.startswith("serving size") or x.startswith("ingredients") or x.startswith("amino acid profile") or x.startswith("*calorific"):
            end = k
            break
    block = t[start:end]
    # Column headers: where «per serving» is among the value columns.
    heads = [x.lower() for x in block[:10] if re.search(r"^per |%|nrv|daily dose|^dose", x.lower()) and not x.lower().startswith("servings")]
    vheads = heads
    per_serv_idx = next((k for k, h in enumerate(vheads) if "serving" in h or "daily dose" in h or "per dose" in h or "portion" in h or "tablet" in h or "capsule" in h or "scoop" in h), None)
    if per_serv_idx is None:
        # «Per 100 g» with an unlabeled serving column next to it (effervescent tablets, liquids).
        h100 = next((k for k, h in enumerate(vheads) if re.search(r"100\s*(g|ml)", h)), None)
        if h100 is None:
            h100 = next((k for k, x in enumerate(block[:12]) if re.search(r"100\s*(g|ml)", x.lower())), None)
            h100 = 0 if h100 is not None else None
        per_serv_idx = h100 + 1 if h100 is not None else (len(vheads) - 1 if vheads else 0)
    comps = []
    k = 0
    while k < len(block):
        name = block[k]
        if not re.search(r"[A-Za-zА-Яа-я]", name) or VAL.match(name) or re.search(r"^per |%|nrv|^amount$|^servings|^nutritional values|^\d{1,3}$", name.lower()):
            k += 1
            continue
        vals = []
        j = k + 1
        while j < len(block) and re.match(r"^\d{1,2}$|^\*+$", block[j]):
            j += 1
        while j < len(block) and (VAL.match(block[j]) or re.match(r"^[\d.,]+\s*%$|^\*+$|^-$|^–$", block[j])):
            if VAL.match(block[j]):
                vals.append(block[j])
            j += 1
        if vals:
            v = vals[min(per_serv_idx, len(vals) - 1)]
            nm = re.sub(r"[*†]+$", "", name).strip(" –-")
            m = re.search(r"([\d.,]+)\s*kcal", v, re.I) if "kj" in v.lower() else None
            if m:
                comps.append(["dietary_energy_total", "Энергия", float(m.group(1).replace(",", ".")), "ккал"])
            else:
                g = VAL.match(v)
                amount = float(g.group(2).replace(",", "."))
                unit = g.group(3).lower().replace("µg", "mcg").replace("ie", "iu")
                low = nm.lower()
                if low.startswith("salt"):
                    comps.append(["dietary_sodium_total", "Натрий (из соли)", round(amount * 400 if unit == "g" else amount * 0.4, 1), "мг"])
                elif "of which" in low or "saturated" in low or "sugar" in low:
                    pass
                elif low.startswith("fib"):
                    comps.append(["dietary_fiber_total", nm, amount, "г"])
                else:
                    comps.append(comp(nm, amount, unit))
            k = j
        else:
            k += 1
    form = "порошок" if re.search(r"powder|scoop", " ".join(block[:3]) + serving, re.I) else ""
    return {"id": f"maxler:{p['handle']}", "b": "maxler", "n": p["title"], "s": serving, "f": form,
            "t": kind_of(p["title"], p.get("product_type", ""), comps), "c": dedupe([c for c in comps if c[2] > 0])}


# ---------- medicines (grls.rosminzdrav.ru) ----------
def drugs():
    page = get("https://grls.rosminzdrav.ru/Default.aspx")
    if not page:
        return None
    links = re.findall(r'href="([^"]+\.xlsx?)"', page.decode("utf-8", "ignore"))
    links = [l for l in links if "еречень" in urllib.parse.unquote(l)]
    if not links:
        note("GRLS: no list link")
        return None
    u = links[0]
    if u.startswith("http://"):
        u = "https://" + u[7:]
    u = urllib.parse.quote(u, safe=":/?=&%()_.")
    x = get(u)
    if not x:
        return None
    import xlrd
    b = xlrd.open_workbook(file_contents=x)
    rows = []
    for sh in b.sheets():
        head = None
        inn = ""
        for r in range(sh.nrows):
            vals = [str(c.value).strip() for c in sh.row(r)]
            if head is None:
                if any("Торговое" in v for v in vals):
                    head = {"inn": next(i for i, v in enumerate(vals) if v.startswith("МНН")), "trade": next(i for i, v in enumerate(vals) if "Торговое" in v),
                            "form": next((i for i, v in enumerate(vals) if "форма" in v.lower()), None), "dose": next((i for i, v in enumerate(vals) if "Дозировка" in v), None)}
                continue
            if vals[head["inn"]]:
                inn = vals[head["inn"]].lstrip("*").strip()
            trade = re.sub(r"[®™]", "", vals[head["trade"]]).strip()
            if not trade or not inn:
                continue
            form = vals[head["form"]] if head["form"] is not None else ""
            dose = vals[head["dose"]] if head["dose"] is not None else ""
            rows.append([trade, inn, form, dose])
    seen = set()
    out = []
    for r in rows:
        k = (r[0].lower(), r[1].lower(), r[2].lower())
        if k not in seen:
            seen.add(k)
            out.append(r)
    note("GRLS rows", len(out), "from", urllib.parse.unquote(u).rsplit("/", 1)[-1])
    return {"source": urllib.parse.unquote(u).rsplit("/", 1)[-1], "rows": out}


def write(name, obj):
    data = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode()
    with open(os.path.join(RES, name), "wb") as f:
        f.write(gzip.compress(data, 9))
    note("wrote", name, len(data), "bytes")


if __name__ == "__main__":
    what = sys.argv[1:] or ["supplements", "drugs"]
    if "supplements" in what:
        prods = maxler() + dsld_brand("NOW", {"now", "now foods", "now sports"}) + dsld_brand("Solgar", {"solgar"})
        for r in prods:
            r.pop("off", None) if not r.get("off") else None
        write("catalog.json.gz", {"version": date.today().isoformat(),
                                  "brands": [{"key": "maxler", "name": "Maxler"}, {"key": "now", "name": "NOW"}, {"key": "solgar", "name": "Solgar"}],
                                  "products": prods})
    if "drugs" in what:
        d = drugs()
        if d:
            d["version"] = date.today().isoformat()
            write("drugs.json.gz", d)
    with open(os.path.join(OUT, "build_log.txt"), "w") as f:
        f.write("\n".join(LOG))
