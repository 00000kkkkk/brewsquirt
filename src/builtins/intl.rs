use crate::compiler::Mode;
use crate::value::Value;
use crate::vm::Vm;

pub const LAZY_GLOBALS: [&str; 1] = ["Intl"];

const INTL_SRC: &str = r##"
(() => {
  const NBSP = " "
  const NNBSP = " "
  const NUMBER_DATA = {
    en: { decimal: ".", group: ",", minGroup: 1, percent: "{n}%", currency: "{s}{n}", currencySpace: false },
    de: { decimal: ",", group: ".", minGroup: 1, percent: "{n}" + NBSP + "%", currency: "{n}" + NBSP + "{s}" },
    fr: { decimal: ",", group: NNBSP, minGroup: 1, percent: "{n}" + NBSP + "%", currency: "{n}" + NBSP + "{s}" },
    es: { decimal: ",", group: ".", minGroup: 2, percent: "{n}" + NBSP + "%", currency: "{n}" + NBSP + "{s}" },
    it: { decimal: ",", group: ".", minGroup: 1, percent: "{n}%", currency: "{n}" + NBSP + "{s}" },
    pt: { decimal: ",", group: ".", minGroup: 1, percent: "{n}%", currency: "{s}" + NBSP + "{n}" },
    nl: { decimal: ",", group: ".", minGroup: 1, percent: "{n}%", currency: "{s}" + NBSP + "{n}" },
    uk: { decimal: ",", group: NBSP, minGroup: 1, percent: "{n}%", currency: "{n}" + NBSP + "{s}" },
    ru: { decimal: ",", group: NBSP, minGroup: 1, percent: "{n}" + NBSP + "%", currency: "{n}" + NBSP + "{s}" },
    pl: { decimal: ",", group: NBSP, minGroup: 2, percent: "{n}%", currency: "{n}" + NBSP + "{s}" },
    sv: { decimal: ",", group: NBSP, minGroup: 1, percent: "{n}" + NBSP + "%", currency: "{n}" + NBSP + "{s}" },
    tr: { decimal: ",", group: ".", minGroup: 1, percent: "%{n}", currency: "{s}{n}" },
    ja: { decimal: ".", group: ",", minGroup: 1, percent: "{n}%", currency: "{s}{n}" },
    zh: { decimal: ".", group: ",", minGroup: 1, percent: "{n}%", currency: "{s}{n}" },
    ko: { decimal: ".", group: ",", minGroup: 1, percent: "{n}%", currency: "{s}{n}" },
  }
  const CURRENCY = {
    USD: ["$", 2, "US dollars"], EUR: ["€", 2, "euros"], GBP: ["£", 2, "British pounds"],
    JPY: ["¥", 0, "Japanese yen"], CNY: ["CN¥", 2, "Chinese yuan"], INR: ["₹", 2, "Indian rupees"],
    KRW: ["₩", 0, "South Korean won"], UAH: ["UAH", 2, "Ukrainian hryvnias"], RUB: ["RUB", 2, "Russian rubles"],
    CHF: ["CHF", 2, "Swiss francs"], CAD: ["CA$", 2, "Canadian dollars"], AUD: ["A$", 2, "Australian dollars"],
    PLN: ["PLN", 2, "Polish zlotys"], BRL: ["R$", 2, "Brazilian reals"], MXN: ["MX$", 2, "Mexican pesos"],
    SEK: ["SEK", 2, "Swedish kronor"], TRY: ["TRY", 2, "Turkish liras"], ILS: ["₪", 2, "Israeli new shekels"],
  }
  const UNITS = {
    meter: ["m", "meters"], kilometer: ["km", "kilometers"], centimeter: ["cm", "centimeters"],
    millimeter: ["mm", "millimeters"], mile: ["mi", "miles"], foot: ["ft", "feet"], inch: ["in", "inches"],
    kilogram: ["kg", "kilograms"], gram: ["g", "grams"], pound: ["lb", "pounds"], liter: ["L", "liters"],
    milliliter: ["mL", "milliliters"], second: ["sec", "seconds"], minute: ["min", "minutes"],
    hour: ["hr", "hours"], day: ["day", "days"], week: ["wk", "weeks"], month: ["mth", "months"],
    year: ["yr", "years"], millisecond: ["ms", "milliseconds"], byte: ["byte", "bytes"],
    kilobyte: ["kB", "kilobytes"], megabyte: ["MB", "megabytes"], gigabyte: ["GB", "gigabytes"],
    percent: ["%", "percent"], celsius: ["°C", "degrees Celsius"], fahrenheit: ["°F", "degrees Fahrenheit"],
    "kilometer-per-hour": ["km/h", "kilometers per hour"], "mile-per-hour": ["mph", "miles per hour"],
    "meter-per-second": ["m/s", "meters per second"],
  }

  const canonical = (tag) => {
    if (typeof tag !== "string") throw new TypeError("Incorrect locale information provided")
    const parts = tag.replace(/_/g, "-").split("-")
    if (!/^[A-Za-z]{2,8}$/.test(parts[0])) throw new RangeError(`Incorrect locale information provided`)
    let extension = false
    return parts
      .map((p, i) => {
        if (i === 0) return p.toLowerCase()
        if (p.length === 1) extension = true
        if (extension) return p.toLowerCase()
        return p.length === 2 ? p.toUpperCase() : p.length === 4 ? p[0].toUpperCase() + p.slice(1).toLowerCase() : p.toLowerCase()
      })
      .join("-")
  }
  const localeList = (locales) => {
    if (locales === undefined) return []
    if (locales === null) throw new TypeError("Cannot convert undefined or null to object")
    let list
    if (typeof locales === "string" || (typeof locales === "object" && "baseName" in locales && typeof locales.maximize === "function")) list = [locales]
    else {
      const source = Object(locales)
      const len = Math.min(Math.max(Math.trunc(Number(source.length)) || 0, 0), 2 ** 53 - 1)
      list = []
      for (let k = 0; k < len; k++) {
        if (!(k in source)) continue
        const item = source[k]
        if (typeof item !== "string" && (item === null || typeof item !== "object")) throw new TypeError(`Language ID should be string or object.`)
        list.push(item)
      }
    }
    const out = []
    for (const l of list) {
      const c = canonical(typeof l === "object" ? String(l) : l)
      if (!out.includes(c)) out.push(c)
    }
    return out
  }
  const KNOWN_LANGUAGES = new Set("af am ar as az be bg bn bo br bs ca ce chr cs cy da de dsb dz ee el en eo es et eu fa ff fi fil fo fr fy ga gd gl gu gv ha haw he hi hr hsb hu hy ia id ig is it ja jv ka kk kl km kn ko kok ks ku kw ky lb lg ln lo lt lu lv mg mi mk ml mn mr ms mt my nb nd ne nl nn no om or os pa pl ps pt qu rm rn ro ru rw sa sc sd se sg si sk sl sn so sq sr su sv sw ta te tg th ti tk to tr tt ug uk ur uz vi wo xh yi yo yue zh zu".split(" "))
  const supportedList = (locales) => localeList(locales).filter((l) => KNOWN_LANGUAGES.has(l.split("-")[0].toLowerCase()))
  const resolve = (locales) => supportedList(locales)[0] ?? "en-US"
  const lang = (locale) => locale.split("-")[0]
  const numberData = (locale) => {
    const tag = String(locale)
    if (/^(en|hi|bn|ta|te|mr|gu|kn|ml)-IN$/i.test(tag)) return { ...(NUMBER_DATA[lang(locale)] ?? NUMBER_DATA.en), indian: true }
    return NUMBER_DATA[lang(locale)] ?? NUMBER_DATA.en
  }

  const groupDigits = (int, sep, minGroup, indian = false) => {
    if (int.length < 4 + (minGroup - 1)) return int
    let out = ""
    for (let i = 0; i < int.length; i++) {
      const fromEnd = int.length - i
      const boundary = indian ? fromEnd === 3 || (fromEnd > 3 && (fromEnd - 3) % 2 === 0) : fromEnd % 3 === 0
      if (i > 0 && boundary) out += sep
      out += int[i]
    }
    return out
  }

  const getOption = (options, key, values, fallback) => {
    const v = options[key]
    if (v === undefined) return fallback
    const s = String(v)
    if (values !== undefined && !values.includes(s)) {
      throw new RangeError(`Value ${s} out of range for Intl.NumberFormat options property ${key}`)
    }
    return s
  }
  const defaultNumberOption = (v, min, max, fallback, key) => {
    if (v === undefined) return fallback
    const n = Number(v)
    if (Number.isNaN(n) || n < min || n > max) throw new RangeError(`${key} value is out of range.`)
    return Math.floor(n)
  }
  const getNumberOption = (options, key, min, max, fallback) => defaultNumberOption(options[key], min, max, fallback, key)

  const NU_ZERO = {
    adlm: 0x1e950, ahom: 0x11730, arab: 0x660, arabext: 0x6f0, bali: 0x1b50, beng: 0x9e6, bhks: 0x11c50, brah: 0x11066,
    cakm: 0x11136, cham: 0xaa50, deva: 0x966, diak: 0x11950, fullwide: 0xff10, gara: 0x10d40, gong: 0x11da0, gonm: 0x11d50,
    gujr: 0xae6, gukh: 0x16130, guru: 0xa66, hmng: 0x16b50, hmnp: 0x1e140, java: 0xa9d0, kali: 0xa900, kawi: 0x11f50,
    khmr: 0x17e0, knda: 0xce6, krai: 0x16d70, lana: 0x1a80, lanatham: 0x1a90, laoo: 0xed0, latn: 0x30, lepc: 0x1c40,
    limb: 0x1946, mathbold: 0x1d7ce, mathdbl: 0x1d7d8, mathmono: 0x1d7f6, mathsanb: 0x1d7ec, mathsans: 0x1d7e2, mlym: 0xd66,
    modi: 0x11650, mong: 0x1810, mroo: 0x16a60, mtei: 0xabf0, mymr: 0x1040, mymrepka: 0x116da, mymrpao: 0x116d0,
    mymrshan: 0x1090, mymrtlng: 0xa9f0, nagm: 0x1e4f0, newa: 0x11450, nkoo: 0x7c0, olck: 0x1c50, onao: 0x1e5f1, orya: 0xb66,
    osma: 0x104a0, outlined: 0x1ccf0, rohg: 0x10d30, saur: 0xa8d0, segment: 0x1fbf0, shrd: 0x111d0, sind: 0x112f0,
    sinh: 0xde6, sora: 0x110f0, sund: 0x1bb0, sunu: 0x11bf0, takr: 0x116c0, talu: 0x19d0, tamldec: 0xbe6, telu: 0xc66,
    thai: 0xe50, tibt: 0xf20, tirh: 0x114d0, tnsa: 0x16ac0, tols: 0x11de0, vaii: 0xa620, wara: 0x118e0, wcho: 0x1e2f0,
  }
  const nuSupported = (nu) => nu === "hanidec" || Object.hasOwn(NU_ZERO, nu)
  const nuDigits = (nu) => {
    if (nu === "latn") return null
    if (nu === "hanidec") return [..."〇一二三四五六七八九"]
    return Array.from({ length: 10 }, (_, i) => String.fromCodePoint(NU_ZERO[nu] + i))
  }
  const parseExtension = (tag) => {
    const parts = tag.split("-")
    let i = parts.findIndex((p, k) => k > 0 && p.length === 1)
    if (i === -1) return { base: tag, keywords: {} }
    const base = parts.slice(0, i).join("-")
    const keywords = {}
    while (i < parts.length) {
      const singleton = parts[i]
      i++
      if (singleton === "x") break
      if (singleton !== "u") {
        while (i < parts.length && parts[i].length > 1) i++
        continue
      }
      while (i < parts.length && parts[i].length > 2) i++
      while (i < parts.length && parts[i].length === 2) {
        const key = parts[i++]
        const vals = []
        while (i < parts.length && parts[i].length > 2) vals.push(parts[i++])
        if (!Object.hasOwn(keywords, key)) keywords[key] = vals.length ? vals.join("-") : "true"
      }
    }
    return { base, keywords }
  }
  const resolveNumberLocale = (list, optNu) => {
    const requested = list.find((l) => KNOWN_LANGUAGES.has(l.split("-")[0].toLowerCase()))
    if (requested === undefined) return { locale: "en-US", nu: optNu !== undefined && nuSupported(optNu) ? optNu : "latn" }
    const { base, keywords } = parseExtension(requested)
    let nu = "latn"
    let locale = base
    const extNu = keywords.nu
    if (extNu !== undefined && nuSupported(extNu)) {
      nu = extNu
      locale = `${base}-u-nu-${extNu}`
    }
    if (optNu !== undefined && nuSupported(optNu) && optNu !== nu) {
      nu = optNu
      locale = base
    }
    return { locale, nu }
  }

  const decimalOf = (neg, digits, e) => {
    let d = digits.replace(/^0+/, "")
    if (d === "") return { kind: "finite", neg, d: "", e: 0 }
    const stripped = d.replace(/0+$/, "")
    e += d.length - stripped.length
    return { kind: "finite", neg, d: stripped, e }
  }
  const toPrim = (v) => {
    if (v === null || (typeof v !== "object" && typeof v !== "function")) return v
    const exotic = v[Symbol.toPrimitive]
    if (exotic !== undefined && exotic !== null) {
      const r = exotic.call(v, "number")
      if (r !== null && (typeof r === "object" || typeof r === "function")) throw new TypeError("Cannot convert object to primitive value")
      return r
    }
    for (const k of ["valueOf", "toString"]) {
      const f = v[k]
      if (typeof f === "function") {
        const r = f.call(v)
        if (r === null || (typeof r !== "object" && typeof r !== "function")) return r
      }
    }
    throw new TypeError("Cannot convert object to primitive value")
  }
  const NUMERIC_TEXT = /^([+-]?)(?:(\d+)(?:\.(\d*))?|\.(\d+))(?:[eE]([+-]?\d+))?$/
  const toIntlMV = (value) => {
    const p = toPrim(value)
    if (typeof p === "bigint") return decimalOf(p < 0n, (p < 0n ? -p : p).toString(), 0)
    if (typeof p === "string") {
      const s = p.trim()
      if (s === "") return decimalOf(false, "", 0)
      if (/^0[xX][0-9a-fA-F]+$|^0[oO][0-7]+$|^0[bB][01]+$/.test(s)) return decimalOf(false, BigInt(s).toString(), 0)
      const inf = /^([+-]?)Infinity$/.exec(s)
      if (inf) return { kind: "inf", neg: inf[1] === "-" }
      const m = NUMERIC_TEXT.exec(s)
      if (!m) return { kind: "nan", neg: false }
      const frac = m[3] ?? m[4] ?? ""
      return decimalOf(m[1] === "-", (m[2] ?? "") + frac, (m[5] ? Number(m[5]) : 0) - frac.length)
    }
    const x = Number(p)
    if (Number.isNaN(x)) return { kind: "nan", neg: false }
    if (!Number.isFinite(x)) return { kind: "inf", neg: x < 0 }
    const neg = x < 0 || Object.is(x, -0)
    if (x === 0) return decimalOf(neg, "", 0)
    const m = NUMERIC_TEXT.exec(String(Math.abs(x)))
    const frac = m[3] ?? ""
    return decimalOf(neg, m[2] + frac, (m[5] ? Number(m[5]) : 0) - frac.length)
  }
  const decMagnitude = (x) => x.d.length - 1 + x.e
  const pow10 = (n) => 10n ** BigInt(n)
  const roundAt = (x, m, inc, mode) => {
    const digits = x.d === "" ? 0n : BigInt(x.d)
    const num = x.e >= m ? digits * pow10(x.e - m) : digits
    const den = x.e >= m ? 1n : pow10(m - x.e)
    const unit = den * BigInt(inc)
    const q = num / unit
    const r = num % unit
    let up = false
    if (r !== 0n) {
      if (mode === "infinity") up = true
      else if (mode !== "zero") {
        const twice = r * 2n
        if (twice > unit) up = true
        else if (twice === unit) up = mode === "half-infinity" || (mode === "half-even" && q % 2n === 1n)
      }
    }
    return (q + (up ? 1n : 0n)) * BigInt(inc)
  }
  const rawPrecision = (x, minP, maxP, mode) => {
    const p = maxP
    let m
    let e
    let rounded
    if (x.d === "") {
      m = "0".repeat(p)
      e = 0
      rounded = decimalOf(false, "", 0)
    } else {
      e = decMagnitude(x)
      let n = roundAt(x, e - p + 1, 1, mode)
      if (n === pow10(p)) {
        n = pow10(p - 1)
        e += 1
      }
      m = n.toString()
      rounded = decimalOf(false, m, e - p + 1)
    }
    let int
    if (e >= p - 1) {
      m += "0".repeat(e - p + 1)
      int = e + 1
    } else if (e >= 0) {
      m = `${m.slice(0, e + 1)}.${m.slice(e + 1)}`
      int = e + 1
    } else {
      m = `0.${"0".repeat(-(e + 1))}${m}`
      int = 1
    }
    if (m.includes(".") && maxP > minP) {
      let cut = maxP - minP
      while (cut > 0 && m.endsWith("0")) {
        m = m.slice(0, -1)
        cut--
      }
      if (m.endsWith(".")) m = m.slice(0, -1)
    }
    return { str: m, rounded, int, mag: e - p + 1 }
  }
  const rawFixed = (x, minF, maxF, inc, mode) => {
    const f = maxF
    const n = roundAt(x, -f, inc, mode)
    let m = n === 0n ? "0" : n.toString()
    let int
    if (f !== 0) {
      let k = m.length
      if (k <= f) {
        m = "0".repeat(f + 1 - k) + m
        k = f + 1
      }
      const a = m.slice(0, k - f)
      m = `${a}.${m.slice(k - f)}`
      int = a.length
    } else {
      int = m.length
    }
    let cut = maxF - minF
    while (cut > 0 && m.endsWith("0")) {
      m = m.slice(0, -1)
      cut--
    }
    if (m.endsWith(".")) m = m.slice(0, -1)
    return { str: m, rounded: decimalOf(false, n.toString(), -f), int, mag: -f }
  }
  const UNSIGNED_MODES = {
    ceil: ["infinity", "zero"], floor: ["zero", "infinity"], expand: ["infinity", "infinity"], trunc: ["zero", "zero"],
    halfCeil: ["half-infinity", "half-zero"], halfFloor: ["half-zero", "half-infinity"], halfExpand: ["half-infinity", "half-infinity"],
    halfTrunc: ["half-zero", "half-zero"], halfEven: ["half-even", "half-even"],
  }
  const formatNumeric = (o, x, negative) => {
    const mode = UNSIGNED_MODES[o.roundingMode][negative ? 1 : 0]
    let result
    if (o.roundingType === "significantDigits") result = rawPrecision(x, o.minSD, o.maxSD, mode)
    else if (o.roundingType === "fractionDigits") result = rawFixed(x, o.minFD, o.maxFD, o.roundingIncrement, mode)
    else {
      const s = rawPrecision(x, o.minSD, o.maxSD, mode)
      const f = rawFixed(x, o.minFD, o.maxFD, o.roundingIncrement, mode)
      if (o.roundingType === "morePrecision") result = s.mag <= f.mag ? s : f
      else result = s.mag <= f.mag ? f : s
    }
    let str = result.str
    if (o.trailingZeroDisplay === "stripIfInteger" && (result.rounded.d === "" || result.rounded.e >= 0)) {
      const i = str.indexOf(".")
      if (i !== -1) str = str.slice(0, i)
    }
    if (result.int < o.minimumIntegerDigits) str = "0".repeat(o.minimumIntegerDigits - result.int) + str
    return { str, rounded: result.rounded }
  }

  const COMPACT_DATA = {
    en: { short: [[3, "K"], [6, "M"], [9, "B"], [12, "T"]], long: [[3, " thousand"], [6, " million"], [9, " billion"], [12, " trillion"]] },
    "en-IN": { short: [[3, "K"], [5, "L"], [7, "Cr"]], long: [[3, " thousand"], [5, " lakh"], [7, " crore"]] },
    de: {
      short: [[6, NBSP + "Mio."], [9, NBSP + "Mrd."], [12, NBSP + "Bio."]],
      long: [[3, " Tausend"], [6, [" Million", " Millionen"]], [9, [" Milliarde", " Milliarden"]], [12, [" Billion", " Billionen"]]],
    },
    ja: { short: [[4, "万"], [8, "億"], [12, "兆"]] },
    ko: { short: [[3, "천"], [4, "만"], [8, "억"], [12, "조"]] },
    zh: { short: [[4, "万"], [8, "亿"], [12, "万亿"]] },
    "zh-Hant": { short: [[4, "萬"], [8, "億"], [12, "兆"]] },
  }
  const compactTable = (locale, display) => {
    const { base } = parseExtension(locale)
    const parts = base.split("-")
    const lg = parts[0]
    let entry = COMPACT_DATA[`${lg}-${parts[parts.length - 1]}`]
    if (lg === "zh" && (parts.includes("Hant") || parts.includes("TW") || parts.includes("HK") || parts.includes("MO"))) entry = COMPACT_DATA["zh-Hant"]
    entry ??= COMPACT_DATA[lg] ?? COMPACT_DATA.en
    return (display === "long" ? entry.long : undefined) ?? entry.short
  }
  const exponentForMagnitude = (o, mag) => {
    if (o.notation === "scientific") return mag
    if (o.notation === "engineering") return Math.floor(mag / 3) * 3
    if (o.notation === "compact") {
      let exp = 0
      for (const [threshold] of compactTable(o.locale, o.compactDisplay)) if (mag >= threshold) exp = threshold
      return exp
    }
    return 0
  }
  const computeExponent = (o, x) => {
    if (x.d === "") return 0
    const mag = decMagnitude(x)
    const exponent = exponentForMagnitude(o, mag)
    const r = formatNumeric(o, { ...x, e: x.e - exponent }, false)
    if (r.rounded.d === "") return exponent
    if (decMagnitude(r.rounded) === mag - exponent) return exponent
    return exponentForMagnitude(o, mag + 1)
  }
  const compactSuffix = (o, exponent, shown) => {
    const hit = compactTable(o.locale, o.compactDisplay).find(([threshold]) => threshold === exponent)
    if (!hit) return ""
    const word = hit[1]
    return Array.isArray(word) ? word[shown === "1" ? 0 : 1] : word
  }

  const NUMBER_DATA_REGION = {
    "pt-PT": { decimal: ",", group: NBSP, minGroup: 2, percent: "{n}%", currency: "{n}" + NBSP + "{s}" },
    "de-CH": { decimal: ".", group: "’", minGroup: 1, percent: "{n}%", currency: "{s}" + NBSP + "{n}" },
    "de-AT": { decimal: ",", group: NBSP, minGroup: 1, percent: "{n}" + NBSP + "%", currency: "{s}" + NBSP + "{n}" },
  }
  const localeData = (locale, nu) => {
    const { base } = parseExtension(locale)
    const parts = base.split("-")
    const region = parts.find((p, i) => i > 0 && (p.length === 2 || /^\d{3}$/.test(p)))
    let data = (region && NUMBER_DATA_REGION[`${parts[0]}-${region}`]) ?? numberData(base)
    if (nu === "arab" || nu === "arabext") data = { ...data, decimal: "٫", group: "٬" }
    return data
  }
  const LOCAL_SYMBOLS = {
    ko: { USD: "US$" }, zh: { USD: "US$", CNY: "¥" }, ja: { JPY: "￥", CNY: "元" }, fr: { USD: "$US" }, es: { USD: "US$" },
    pt: { USD: "US$" }, nl: { USD: "US$" }, sv: { USD: "US$" }, uk: { UAH: "₴", USD: "USD" }, ru: { RUB: "₽" },
    pl: { PLN: "zł", USD: "USD" }, it: { USD: "USD" }, tr: { TRY: "₺" },
  }
  const ACCOUNTING_PARENS = new Set(["en", "ja", "ko", "zh"])
  const currencySymbol = (o) => {
    if (o.currencyDisplay === "code") return o.currency
    const info = CURRENCY[o.currency]
    const symbol = LOCAL_SYMBOLS[lang(o.locale)]?.[o.currency] ?? info?.[0] ?? o.currency
    if (o.currencyDisplay === "narrowSymbol") return symbol.replace(/^[A-Z]+(?=[^A-Z])/, "").replace(/(?<=[^A-Z])[A-Z]+$/, "") || symbol
    return symbol
  }
  const currencyDigits = (code) => CURRENCY[code]?.[1] ?? 2
  const SANCTIONED_UNITS = new Set(
    "acre bit byte celsius centimeter day degree fahrenheit fluid-ounce foot gallon gigabit gigabyte gram hectare hour inch kilobit kilobyte kilogram kilometer liter megabit megabyte meter microsecond mile mile-scandinavian milliliter millimeter millisecond minute month nanosecond ounce percent petabyte pound second stone terabit terabyte week yard year".split(" "),
  )
  const wellFormedUnit = (u) => {
    if (SANCTIONED_UNITS.has(u)) return true
    const i = u.indexOf("-per-")
    return i !== -1 && SANCTIONED_UNITS.has(u.slice(0, i)) && SANCTIONED_UNITS.has(u.slice(i + 5))
  }
  const unitNames = (unit) => {
    if (UNITS[unit]) return UNITS[unit]
    const i = unit.indexOf("-per-")
    if (i !== -1) {
      const [ns, nl] = unitNames(unit.slice(0, i))
      const [ds, dl] = unitNames(unit.slice(i + 5))
      return [`${ns}/${ds}`, `${nl} per ${singularUnit(dl)}`]
    }
    return [unit, unit]
  }
  const singularUnit = (name) =>
    name === "feet" ? "foot" : name === "inches" ? "inch" : name === "centuries" ? "century" : name.replace(/ies$/, "y").replace(/(?<=\w)s(?= |$)/, "")
  const UNIT_TIGHT = { short: new Set(["ko"]), narrow: new Set(["en", "ja", "ko", "zh"]) }
  const RANGE_SEPARATOR = { pt: " - ", es: "-", ja: "～", ko: "~", zh: "-" }
  const kFallback = Symbol("IntlLegacyConstructedSymbol")
  const ROUNDING_INCREMENTS = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 2500, 5000]
  const ROUNDING_MODES = ["ceil", "floor", "expand", "trunc", "halfCeil", "halfFloor", "halfExpand", "halfTrunc", "halfEven"]

  class NumberFormat {
    #o
    #data
    #digits
    #bound
    constructor(locales, options) {
      const list = localeList(locales)
      if (options === null) throw new TypeError("Cannot convert undefined or null to object")
      const opts = options === undefined ? Object.create(null) : Object(options)
      getOption(opts, "localeMatcher", ["lookup", "best fit"], "best fit")
      const numberingSystem = getOption(opts, "numberingSystem", undefined, undefined)
      if (numberingSystem !== undefined && !/^[a-zA-Z0-9]{3,8}(-[a-zA-Z0-9]{3,8})*$/.test(numberingSystem)) {
        throw new RangeError(`Invalid numberingSystem : ${numberingSystem}`)
      }
      const { locale, nu } = resolveNumberLocale(list, numberingSystem)
      const style = getOption(opts, "style", ["decimal", "percent", "currency", "unit"], "decimal")
      const currency = getOption(opts, "currency", undefined, undefined)
      if (currency !== undefined && !/^[A-Za-z]{3}$/.test(currency)) throw new RangeError(`Invalid currency code : ${currency}`)
      const currencyDisplay = getOption(opts, "currencyDisplay", ["code", "symbol", "narrowSymbol", "name"], "symbol")
      const currencySign = getOption(opts, "currencySign", ["standard", "accounting"], "standard")
      const unit = getOption(opts, "unit", undefined, undefined)
      if (unit !== undefined && !wellFormedUnit(unit)) throw new RangeError(`Invalid unit argument for Intl.NumberFormat() '${unit}'`)
      const unitDisplay = getOption(opts, "unitDisplay", ["short", "narrow", "long"], "short")
      if (style === "currency" && currency === undefined) throw new TypeError("Currency code is required with currency style.")
      if (style === "unit" && unit === undefined) throw new TypeError("Unit is required with unit style.")
      const notation = getOption(opts, "notation", ["standard", "scientific", "engineering", "compact"], "standard")
      let mnfdDefault = 0
      let mxfdDefault = style === "percent" ? 0 : 3
      if (style === "currency" && notation === "standard") {
        mnfdDefault = currencyDigits(currency.toUpperCase())
        mxfdDefault = mnfdDefault
      }
      const mnid = getNumberOption(opts, "minimumIntegerDigits", 1, 21, 1)
      const mnfd = opts.minimumFractionDigits
      const mxfd = opts.maximumFractionDigits
      const mnsd = opts.minimumSignificantDigits
      const mxsd = opts.maximumSignificantDigits
      const roundingIncrement = getNumberOption(opts, "roundingIncrement", 1, 5000, 1)
      if (!ROUNDING_INCREMENTS.includes(roundingIncrement)) throw new RangeError("roundingIncrement value is out of range.")
      const roundingMode = getOption(opts, "roundingMode", ROUNDING_MODES, "halfExpand")
      const roundingPriority = getOption(opts, "roundingPriority", ["auto", "morePrecision", "lessPrecision"], "auto")
      const trailingZeroDisplay = getOption(opts, "trailingZeroDisplay", ["auto", "stripIfInteger"], "auto")
      if (roundingIncrement !== 1) mxfdDefault = mnfdDefault
      const hasSd = mnsd !== undefined || mxsd !== undefined
      const hasFd = mnfd !== undefined || mxfd !== undefined
      let needSd = true
      let needFd = true
      if (roundingPriority === "auto") {
        needSd = hasSd
        if (needSd || (!hasFd && notation === "compact")) needFd = false
      }
      const digits = {}
      if (needSd) {
        if (hasSd) {
          digits.minSD = defaultNumberOption(mnsd, 1, 21, 1, "minimumSignificantDigits")
          digits.maxSD = defaultNumberOption(mxsd, digits.minSD, 21, 21, "maximumSignificantDigits")
        } else {
          digits.minSD = 1
          digits.maxSD = 2
        }
      }
      if (needFd) {
        if (hasFd) {
          let lo = defaultNumberOption(mnfd, 0, 100, undefined, "minimumFractionDigits")
          let hi = defaultNumberOption(mxfd, 0, 100, undefined, "maximumFractionDigits")
          if (lo === undefined) lo = Math.min(mnfdDefault, hi)
          else if (hi === undefined) hi = Math.max(mxfdDefault, lo)
          else if (lo > hi) throw new RangeError("maximumFractionDigits value is out of range.")
          digits.minFD = lo
          digits.maxFD = hi
        } else {
          digits.minFD = mnfdDefault
          digits.maxFD = mxfdDefault
        }
      }
      let roundingType
      let computedPriority = roundingPriority
      if (!needSd && !needFd) {
        Object.assign(digits, { minFD: 0, maxFD: 0, minSD: 1, maxSD: 2 })
        roundingType = "morePrecision"
        computedPriority = "morePrecision"
      } else if (roundingPriority === "auto") {
        roundingType = hasSd ? "significantDigits" : "fractionDigits"
      } else {
        roundingType = roundingPriority
      }
      if (roundingIncrement !== 1) {
        if (roundingType !== "fractionDigits") throw new TypeError("roundingIncrement is only allowed with fraction digits rounding")
        if (digits.maxFD !== digits.minFD) throw new RangeError("maximumFractionDigits and minimumFractionDigits must be equal when roundingIncrement is used")
      }
      const compactDisplay = getOption(opts, "compactDisplay", ["short", "long"], "short")
      const defaultGrouping = notation === "compact" ? "min2" : "auto"
      let useGrouping = opts.useGrouping
      if (useGrouping === undefined) useGrouping = defaultGrouping
      else if (useGrouping === true) useGrouping = "always"
      else if (!useGrouping) useGrouping = false
      else {
        useGrouping = String(useGrouping)
        if (useGrouping === "true" || useGrouping === "false") useGrouping = defaultGrouping
        else if (!["min2", "auto", "always"].includes(useGrouping)) {
          throw new RangeError(`Value ${useGrouping} out of range for Intl.NumberFormat options property useGrouping`)
        }
      }
      const signDisplay = getOption(opts, "signDisplay", ["auto", "never", "always", "exceptZero", "negative"], "auto")
      this.#o = {
        locale,
        numberingSystem: nu,
        style,
        currency: currency === undefined ? undefined : currency.toUpperCase(),
        currencyDisplay,
        currencySign,
        unit,
        unitDisplay,
        minimumIntegerDigits: mnid,
        ...digits,
        roundingType,
        roundingIncrement,
        roundingMode,
        roundingPriority: computedPriority,
        trailingZeroDisplay,
        useGrouping,
        notation,
        compactDisplay,
        signDisplay,
      }
      this.#data = localeData(locale, nu)
      this.#digits = nuDigits(nu)
    }
    static #unwrap(x, method) {
      if (x !== null && (typeof x === "object" || typeof x === "function") && !(#o in x) && Object.prototype.isPrototypeOf.call(NumberFormat.prototype, x)) {
        x = x[kFallback]
      }
      if (x === null || (typeof x !== "object" && typeof x !== "function") || !(#o in x)) {
        throw new TypeError(`Method Intl.NumberFormat.prototype.${method} called on incompatible receiver ${typeof x === "object" && x !== null ? "#<Object>" : String(x)}`)
      }
      return x
    }
    static legacy(nf, self) {
      if (self !== undefined && self !== null && (typeof self === "object" || typeof self === "function") && Object.prototype.isPrototypeOf.call(NumberFormat.prototype, self)) {
        Object.defineProperty(self, kFallback, { value: nf, writable: false, enumerable: false, configurable: false })
        return self
      }
      return nf
    }
    #tr(s) {
      const d = this.#digits
      return d === null ? s : s.replace(/[0-9]/g, (c) => d[c.charCodeAt(0) - 48])
    }
    #layered(x) {
      const o = this.#o
      const data = this.#data
      const nan = x.kind === "nan"
      const body = []
      const inner = []
      let rounded = null
      let shown = ""
      if (nan) body.push({ type: "nan", value: "NaN", layer: "number" })
      else if (x.kind === "inf") body.push({ type: "infinity", value: "∞", layer: "number" })
      else {
        const v = { kind: "finite", neg: false, d: x.d, e: x.e }
        if (o.style === "percent" && v.d !== "") v.e += 2
        let exponent = 0
        if (o.notation !== "standard") {
          exponent = computeExponent(o, v)
          v.e -= exponent
        }
        const r = formatNumeric(o, v, x.neg)
        rounded = r.rounded
        shown = r.str
        const dot = r.str.indexOf(".")
        const int = dot === -1 ? r.str : r.str.slice(0, dot)
        const g = o.useGrouping
        const minGroup = g === "always" ? 1 : g === "min2" ? 2 : data.minGroup
        const chunks = g === false || int.length < 3 + minGroup ? [int] : groupDigits(int, "\u0000", 1, data.indian === true).split("\u0000")
        chunks.forEach((chunk, i) => {
          if (i > 0) body.push({ type: "group", value: data.group, layer: "number" })
          body.push({ type: "integer", value: this.#tr(chunk), layer: "number" })
        })
        if (dot !== -1) {
          body.push({ type: "decimal", value: data.decimal, layer: "number" })
          body.push({ type: "fraction", value: this.#tr(r.str.slice(dot + 1)), layer: "number" })
        }
        if (o.notation === "scientific" || o.notation === "engineering") {
          inner.push({ type: "exponentSeparator", value: "E", layer: "inner" })
          if (exponent < 0) inner.push({ type: "exponentMinusSign", value: "-", layer: "inner" })
          inner.push({ type: "exponentInteger", value: this.#tr(String(Math.abs(exponent))), layer: "inner" })
        } else if (o.notation === "compact" && exponent > 0) {
          const suffix = compactSuffix(o, exponent, r.str)
          const lead = /^[\s ]*/.exec(suffix)[0]
          if (lead) inner.push({ type: "literal", value: lead, layer: "inner" })
          if (suffix.length > lead.length) inner.push({ type: "compact", value: suffix.slice(lead.length), layer: "inner" })
        }
      }
      const zero = rounded !== null && rounded.d === ""
      const neg = !nan && x.neg
      let sign = ""
      if (o.signDisplay === "auto") sign = neg ? "-" : ""
      else if (o.signDisplay === "always") sign = neg ? "-" : "+"
      else if (o.signDisplay === "exceptZero") sign = nan || zero ? "" : neg ? "-" : "+"
      else if (o.signDisplay === "negative") sign = neg && !zero ? "-" : ""
      const signParts = sign ? [{ type: sign === "-" ? "minusSign" : "plusSign", value: sign, layer: "middle" }] : []
      const core = [...body, ...inner]
      const out = []
      if (o.style === "percent") {
        const [before, after] = data.percent.split("{n}")
        out.push(...signParts)
        if (before) out.push({ type: "percentSign", value: "%", layer: "middle" })
        out.push(...core)
        if (after) {
          const lit = after.slice(0, -1)
          if (lit) out.push({ type: "literal", value: lit, layer: "middle" })
          out.push({ type: "percentSign", value: "%", layer: "middle" })
        }
        return out
      }
      if (o.style === "currency") {
        if (o.currencyDisplay === "name") {
          const name = CURRENCY[o.currency]?.[2] ?? o.currency
          const singular = shown === "1" && name.endsWith("s") ? name.replace(/(?<=[a-z])s$/, "") : name
          out.push(...signParts, ...core, { type: "literal", value: " ", layer: "outer" }, { type: "currency", value: singular, layer: "outer" })
          return out
        }
        const symbol = currencySymbol(o)
        const paren = o.currencySign === "accounting" && sign === "-" && ACCOUNTING_PARENS.has(lang(o.locale))
        const [before, after] = data.currency.split("{n}")
        if (paren) out.push({ type: "literal", value: "(", layer: "middle" })
        else out.push(...signParts)
        if (before) {
          const sym = before.replace("{s}", symbol)
          out.push({ type: "currency", value: sym.trim(), layer: "middle" })
          if (sym !== sym.trim() || /\p{L}$/u.test(symbol)) out.push({ type: "literal", value: NBSP, layer: "middle" })
        }
        out.push(...core)
        if (after) {
          out.push({ type: "literal", value: after.slice(0, after.indexOf("{s}")), layer: "middle" })
          out.push({ type: "currency", value: symbol, layer: "middle" })
        }
        if (paren) out.push({ type: "literal", value: ")", layer: "middle" })
        return out
      }
      if (o.style === "unit") {
        const [short, long] = unitNames(o.unit)
        out.push(...signParts, ...core)
        if (o.unitDisplay === "long") {
          out.push({ type: "literal", value: " ", layer: "outer" }, { type: "unit", value: shown === "1" ? singularUnit(long) : long, layer: "outer" })
        } else if (short === "%" || short.startsWith("°") || UNIT_TIGHT[o.unitDisplay].has(lang(o.locale))) {
          out.push({ type: "unit", value: short, layer: "outer" })
        } else {
          out.push({ type: "literal", value: " ", layer: "outer" }, { type: "unit", value: short, layer: "outer" })
        }
        return out
      }
      out.push(...signParts, ...core)
      return out
    }
    #range(start, end) {
      if (start === undefined || end === undefined) throw new TypeError("start or end is undefined")
      const x = toIntlMV(start)
      const y = toIntlMV(end)
      if (x.kind === "nan" || y.kind === "nan") throw new RangeError(`Invalid ${x.kind === "nan" ? "start" : "end"} : NaN`)
      const a = this.#layered(x)
      const b = this.#layered(y)
      const text = (ps, layer) => ps.filter((p) => layer === undefined || p.layer === layer).map((p) => p.value).join("")
      const tag = (ps, source) => ps.map(({ type, value }) => ({ type, value, source }))
      if (text(a) === text(b)) return [{ type: "approximatelySign", value: "~", source: "shared" }, ...tag(a, "shared")]
      const collapseOuter = text(a, "outer") === text(b, "outer")
      const collapseMiddle = collapseOuter && text(a, "middle") === text(b, "middle") && [...text(a, "middle")].length > 1
      const collapsed = (p) => (p.layer === "middle" && collapseMiddle) || (p.layer === "outer" && collapseOuter)
      const edges = (ps) => {
        let lead = 0
        while (lead < ps.length && collapsed(ps[lead])) lead++
        let trail = 0
        while (trail < ps.length - lead && collapsed(ps[ps.length - 1 - trail])) trail++
        return [lead, trail]
      }
      const [leadA, trailA] = edges(a)
      const [leadB, trailB] = edges(b)
      const repeated = (!collapseMiddle && text(a, "middle") !== "") || text(a, "inner") !== "" || (!collapseOuter && text(a, "outer") !== "")
      const sep = RANGE_SEPARATOR[lang(this.#o.locale)] ?? "–"
      const literal = repeated && !/^\s/.test(sep) ? ` ${sep} ` : sep
      return [
        ...tag(a.slice(0, leadA), "shared"),
        ...tag(a.slice(leadA, a.length - trailA), "startRange"),
        { type: "literal", value: literal, source: "shared" },
        ...tag(b.slice(leadB, b.length - trailB), "endRange"),
        ...tag(a.slice(a.length - trailA), "shared"),
      ]
    }
    get format() {
      const nf = NumberFormat.#unwrap(this, "format")
      if (nf.#bound === undefined) {
        const bound = (value) => nf.#layered(toIntlMV(value)).map((p) => p.value).join("")
        Object.defineProperty(bound, "name", { value: "", configurable: true })
        nf.#bound = bound
      }
      return nf.#bound
    }
    formatToParts(value) {
      const nf = NumberFormat.#unwrap(this, "formatToParts")
      return nf.#layered(toIntlMV(value)).map(({ type, value: v }) => ({ type, value: v }))
    }
    formatRange(start, end) {
      const nf = NumberFormat.#unwrap(this, "formatRange")
      return nf.#range(start, end).map((p) => p.value).join("")
    }
    formatRangeToParts(start, end) {
      const nf = NumberFormat.#unwrap(this, "formatRangeToParts")
      return nf.#range(start, end)
    }
    resolvedOptions() {
      const o = NumberFormat.#unwrap(this, "resolvedOptions").#o
      const out = { locale: o.locale, numberingSystem: o.numberingSystem, style: o.style }
      if (o.style === "currency") Object.assign(out, { currency: o.currency, currencyDisplay: o.currencyDisplay, currencySign: o.currencySign })
      if (o.style === "unit") Object.assign(out, { unit: o.unit, unitDisplay: o.unitDisplay })
      out.minimumIntegerDigits = o.minimumIntegerDigits
      if (o.roundingType !== "significantDigits") Object.assign(out, { minimumFractionDigits: o.minFD, maximumFractionDigits: o.maxFD })
      if (o.roundingType !== "fractionDigits") Object.assign(out, { minimumSignificantDigits: o.minSD, maximumSignificantDigits: o.maxSD })
      out.useGrouping = o.useGrouping
      out.notation = o.notation
      if (o.notation === "compact") out.compactDisplay = o.compactDisplay
      out.signDisplay = o.signDisplay
      out.roundingIncrement = o.roundingIncrement
      out.roundingMode = o.roundingMode
      out.roundingPriority = o.roundingPriority
      out.trailingZeroDisplay = o.trailingZeroDisplay
      return out
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
  const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
  const EN_DATES = {
    months: MONTHS,
    short: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    days: DAYS,
    daysShort: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    order: "mdy", sep: "/", pad: false, h12: true, am: "AM", pm: "PM",
    full: "{weekday}, {month} {day}, {year}", long: "{month} {day}, {year}", medium: "{short} {day}, {year}", shortDate: "{M}/{d}/{yy}",
    md: "{month} {day}", mdy: "{month} {day}, {year}", my: "{month} {year}", wsep: ", ", dtsep: ", ", at: " at ",
  }
  const DATE_DATA = {
    en: EN_DATES,
    "en-GB": { ...EN_DATES, order: "dmy", pad: true, h12: false, full: "{weekday}, {day} {month} {year}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{dd}/{MM}/{year}", md: "{day} {month}", mdy: "{day} {month} {year}", wsep: " " },
    "en-AU": { ...EN_DATES, order: "dmy", pad: true, h12: true, full: "{weekday} {day} {month} {year}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{dd}/{MM}/{year}", md: "{day} {month}", mdy: "{day} {month} {year}", wsep: " ", am: "am", pm: "pm" },
    "en-CA": { ...EN_DATES, order: "ymd", sep: "-", pad: true, shortDate: "{year}-{MM}-{dd}" },
    uk: {
      months: ["січень", "лютий", "березень", "квітень", "травень", "червень", "липень", "серпень", "вересень", "жовтень", "листопад", "грудень"],
      gen: ["січня", "лютого", "березня", "квітня", "травня", "червня", "липня", "серпня", "вересня", "жовтня", "листопада", "грудня"],
      short: ["січ.", "лют.", "бер.", "квіт.", "трав.", "черв.", "лип.", "серп.", "вер.", "жовт.", "лист.", "груд."],
      days: ["неділя", "понеділок", "вівторок", "середа", "четвер", "пʼятниця", "субота"],
      daysShort: ["нд", "пн", "вт", "ср", "чт", "пт", "сб"],
      order: "dmy", sep: ".", pad: true, h12: false, am: "дп", pm: "пп",
      full: "{weekday}, {day} {gen} {year} р.", long: "{day} {gen} {year} р.", medium: "{day} {short} {year} р.", shortDate: "{dd}.{MM}.{yy}",
      md: "{day} {gen}", mdy: "{day} {gen} {year} р.", my: "{month} {year} р.", wsep: ", ", dtsep: ", ", at: " о ",
    },
    ru: {
      months: ["январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь"],
      gen: ["января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря"],
      short: ["янв.", "февр.", "мар.", "апр.", "мая", "июн.", "июл.", "авг.", "сент.", "окт.", "нояб.", "дек."],
      days: ["воскресенье", "понедельник", "вторник", "среда", "четверг", "пятница", "суббота"],
      daysShort: ["вс", "пн", "вт", "ср", "чт", "пт", "сб"],
      order: "dmy", sep: ".", pad: true, h12: false, am: "AM", pm: "PM",
      full: "{weekday}, {day} {gen} {year} г.", long: "{day} {gen} {year} г.", medium: "{day} {short} {year} г.", shortDate: "{dd}.{MM}.{year}",
      md: "{day} {gen}", mdy: "{day} {gen} {year} г.", my: "{month} {year} г.", wsep: ", ", dtsep: ", ", at: " в ",
    },
    de: {
      months: ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
      short: ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."],
      days: ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"],
      daysShort: ["So.", "Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa."],
      order: "dmy", sep: ".", pad: false, h12: false, am: "AM", pm: "PM",
      full: "{weekday}, {day}. {month} {year}", long: "{day}. {month} {year}", medium: "{dd}.{MM}.{year}", shortDate: "{dd}.{MM}.{yy}",
      md: "{day}. {month}", mdy: "{day}. {month} {year}", my: "{month} {year}", wsep: ", ", dtsep: ", ", at: " um ",
    },
    fr: {
      months: ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
      short: ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
      days: ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"],
      daysShort: ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."],
      order: "dmy", sep: "/", pad: true, h12: false, am: "AM", pm: "PM",
      full: "{weekday} {day} {month} {year}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{dd}/{MM}/{year}",
      md: "{day} {month}", mdy: "{day} {month} {year}", my: "{month} {year}", wsep: " ", dtsep: " ", at: " à ",
    },
    es: {
      months: ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
      short: ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"],
      days: ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"],
      daysShort: ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
      order: "dmy", sep: "/", pad: false, h12: false, am: "a. m.", pm: "p. m.",
      full: "{weekday}, {day} de {month} de {year}", long: "{day} de {month} de {year}", medium: "{day} {short} {year}", shortDate: "{d}/{M}/{yy}",
      md: "{day} de {month}", mdy: "{day} de {month} de {year}", my: "{month} de {year}", wsep: ", ", dtsep: ", ", at: ", ",
    },
    it: {
      months: ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"],
      short: ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"],
      days: ["domenica", "lunedì", "martedì", "mercoledì", "giovedì", "venerdì", "sabato"],
      daysShort: ["dom", "lun", "mar", "mer", "gio", "ven", "sab"],
      order: "dmy", sep: "/", pad: false, h12: false, am: "AM", pm: "PM",
      full: "{weekday} {day} {month} {year}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{dd}/{MM}/{yy}",
      md: "{day} {month}", mdy: "{day} {month} {year}", my: "{month} {year}", wsep: " ", dtsep: ", ", at: " alle ore ",
    },
    pt: {
      months: ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"],
      short: ["jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.", "dez."],
      days: ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"],
      daysShort: ["dom.", "seg.", "ter.", "qua.", "qui.", "sex.", "sáb."],
      order: "dmy", sep: "/", pad: true, h12: false, am: "AM", pm: "PM",
      full: "{weekday}, {day} de {month} de {year}", long: "{day} de {month} de {year}", medium: "{day} de {short} de {year}", shortDate: "{dd}/{MM}/{fullYear}",
      md: "{day} de {month}", mdy: "{day} de {month} de {year}", my: "{month} de {year}", wsep: ", ", dtsep: ", ", at: " às ",
    },
    pl: {
      months: ["styczeń", "luty", "marzec", "kwiecień", "maj", "czerwiec", "lipiec", "sierpień", "wrzesień", "październik", "listopad", "grudzień"],
      gen: ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"],
      short: ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"],
      days: ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"],
      daysShort: ["niedz.", "pon.", "wt.", "śr.", "czw.", "pt.", "sob."],
      order: "dmy", sep: ".", pad: "month", h12: false, am: "AM", pm: "PM",
      full: "{weekday}, {day} {gen} {year}", long: "{day} {gen} {year}", medium: "{day} {short} {year}", shortDate: "{d}.{MM}.{year}",
      md: "{day} {gen}", mdy: "{day} {gen} {year}", my: "{month} {year}", wsep: ", ", dtsep: ", ", at: " o ",
    },
    nl: {
      months: ["januari", "februari", "maart", "april", "mei", "juni", "juli", "augustus", "september", "oktober", "november", "december"],
      short: ["jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec"],
      days: ["zondag", "maandag", "dinsdag", "woensdag", "donderdag", "vrijdag", "zaterdag"],
      daysShort: ["zo", "ma", "di", "wo", "do", "vr", "za"],
      order: "dmy", sep: "-", pad: false, h12: false, am: "a.m.", pm: "p.m.",
      full: "{weekday} {day} {month} {year}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{dd}-{MM}-{year}",
      md: "{day} {month}", mdy: "{day} {month} {year}", my: "{month} {year}", wsep: " ", dtsep: ", ", at: " om ",
    },
    sv: {
      months: ["januari", "februari", "mars", "april", "maj", "juni", "juli", "augusti", "september", "oktober", "november", "december"],
      short: ["jan.", "feb.", "mars", "apr.", "maj", "juni", "juli", "aug.", "sep.", "okt.", "nov.", "dec."],
      days: ["söndag", "måndag", "tisdag", "onsdag", "torsdag", "fredag", "lördag"],
      daysShort: ["sön", "mån", "tis", "ons", "tors", "fre", "lör"],
      order: "ymd", sep: "-", pad: true, h12: false, am: "fm", pm: "em",
      full: "{weekday} {day} {month} {year}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{year}-{MM}-{dd}",
      md: "{day} {month}", mdy: "{day} {month} {year}", my: "{month} {year}", wsep: " ", dtsep: " ", at: " kl. ",
    },
    tr: {
      months: ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"],
      short: ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"],
      days: ["Pazar", "Pazartesi", "Salı", "Çarşamba", "Perşembe", "Cuma", "Cumartesi"],
      daysShort: ["Paz", "Pzt", "Sal", "Çar", "Per", "Cum", "Cmt"],
      order: "dmy", sep: ".", pad: true, h12: false, am: "ÖÖ", pm: "ÖS",
      full: "{day} {month} {year} {weekday}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{dd}.{MM}.{year}",
      md: "{day} {month}", mdy: "{day} {month} {year}", my: "{month} {year}", wsep: " ", dtsep: " ", at: " ", weekdayAfter: true,
    },
    ja: {
      months: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
      short: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
      days: ["日曜日", "月曜日", "火曜日", "水曜日", "木曜日", "金曜日", "土曜日"],
      daysShort: ["日", "月", "火", "水", "木", "金", "土"],
      order: "ymd", sep: "/", pad: false, h12: false, am: "午前", pm: "午後",
      full: "{year}年{M}月{d}日{weekday}", long: "{year}年{M}月{d}日", medium: "{year}/{MM}/{dd}", shortDate: "{year}/{MM}/{dd}",
      md: "{M}月{d}日", mdy: "{year}年{M}月{d}日", my: "{year}年{M}月", wsep: "", dtsep: " ", at: " ", weekdayAfter: true,
    },
    zh: {
      months: ["一月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "十一月", "十二月"],
      short: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
      days: ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"],
      daysShort: ["周日", "周一", "周二", "周三", "周四", "周五", "周六"],
      order: "ymd", sep: "/", pad: false, h12: false, am: "上午", pm: "下午",
      full: "{year}年{M}月{d}日{weekday}", long: "{year}年{M}月{d}日", medium: "{year}年{M}月{d}日", shortDate: "{year}/{M}/{d}",
      md: "{M}月{d}日", mdy: "{year}年{M}月{d}日", my: "{year}年{M}月", wsep: "", dtsep: " ", at: " ", weekdayAfter: true,
    },
  }
  const dateData = (locale) => DATE_DATA[locale] ?? DATE_DATA[lang(locale)] ?? EN_DATES

  const tzInfo = globalThis["%tzInfo%"]
  const tzCanonicalName = globalThis["%tzCanonical%"]
  const tzList = globalThis["%tzList%"]
  const tzSystem = globalThis["%tzSystem%"]
  const UTC_ALIASES = ["UTC", "ETC/UTC", "ETC/GMT", "GMT", "ETC/UCT", "UCT", "ETC/ZULU", "ZULU", "ETC/UNIVERSAL", "UNIVERSAL", "ETC/GREENWICH", "GREENWICH", "GMT0", "ETC/GMT0", "ETC/GMT+0", "ETC/GMT-0", "GMT+0", "GMT-0", "Z"]
  const parseZoneOffset = (tz) => {
    const m = /^(?:UTC|GMT)?([+-])(\d{1,2})(?::?(\d{2}))?$/.exec(tz)
    if (!m) return undefined
    const hours = Number(m[2])
    const minutes = Number(m[3] ?? 0)
    if (hours > 23 || minutes > 59) return undefined
    return (m[1] === "-" ? -1 : 1) * (hours * 60 + minutes)
  }
  const canonicalTimeZone = (tz) => {
    if (UTC_ALIASES.includes(tz.toUpperCase())) return "UTC"
    const offset = parseZoneOffset(tz)
    if (offset !== undefined) {
      const a = Math.abs(offset)
      return `${offset < 0 ? "-" : "+"}${pad2(Math.floor(a / 60))}:${pad2(a % 60)}`
    }
    const canonical = tzCanonicalName(tz)
    if (canonical === undefined) throw new RangeError(`Invalid time zone specified: ${tz}`)
    return canonical
  }
  const zoneOffset = (tz, date) => {
    if (tz === undefined) return -date.getTimezoneOffset()
    if (tz === "UTC") return 0
    const fixed = parseZoneOffset(tz)
    if (fixed !== undefined) return fixed
    const info = tzInfo(tz, date.getTime())
    return info === undefined ? -date.getTimezoneOffset() : info[0]
  }
  const zoneName = (tz, style, offset, date) => {
    const gmt = (long) => {
      if (offset === 0) return "GMT"
      const sign = offset < 0 ? "-" : "+"
      const a = Math.abs(offset)
      const h = Math.floor(a / 60)
      const m = a % 60
      return long ? `GMT${sign}${pad2(h)}:${pad2(m)}` : `GMT${sign}${h}${m ? ":" + pad2(m) : ""}`
    }
    if (tz === "UTC") {
      if (style === "shortOffset") return "GMT+0"
      if (style === "longOffset") return "GMT+00:00"
      return style === "long" || style === "longGeneric" ? "Coordinated Universal Time" : "UTC"
    }
    if (style === "shortOffset") return gmt(false)
    if (style === "longOffset") return gmt(true)
    if (parseZoneOffset(tz) !== undefined) return gmt(style === "long")
    const info = tzInfo(tz, date.getTime())
    const abbreviation = info !== undefined && KNOWN_ABBREVIATIONS.includes(info[1]) ? info[1] : undefined
    if ((style === "short" || style === "shortGeneric") && abbreviation !== undefined) return abbreviation
    return gmt(style === "long" || style === "longGeneric")
  }
  const KNOWN_ABBREVIATIONS = ["EST", "EDT", "CST", "CDT", "MST", "MDT", "PST", "PDT", "AKST", "AKDT", "HST", "HDT", "GMT", "UTC"]

  const fields = (date, offset) => {
    const d = new Date(date.getTime() + offset * 60000)
    return {
      year: d.getUTCFullYear(),
      month: d.getUTCMonth(),
      day: d.getUTCDate(),
      weekday: d.getUTCDay(),
      hour: d.getUTCHours(),
      minute: d.getUTCMinutes(),
      second: d.getUTCSeconds(),
      ms: d.getUTCMilliseconds(),
    }
  }

  const pad2 = (n) => String(n).padStart(2, "0")

  class DateTimeFormat {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      const locale = resolve(locales)
      const nuOpt = opts.numberingSystem === undefined ? undefined : String(opts.numberingSystem)
      if (nuOpt !== undefined && !/^[a-zA-Z0-9]{3,8}(-[a-zA-Z0-9]{3,8})*$/.test(nuOpt)) throw new RangeError(`Invalid numberingSystem : ${nuOpt}`)
      const nu = resolveNumberLocale(localeList(locales), nuOpt).nu
      const pick = (k, allowed) => {
        const v = opts[k]
        if (v === undefined) return undefined
        const s = String(v)
        if (allowed && !allowed.includes(s)) throw new RangeError(`Value ${s} out of range for Intl.DateTimeFormat options property ${k}`)
        return s
      }
      const o = {
        locale,
        calendar: "gregory",
        numberingSystem: nu,
        timeZone: opts.timeZone === undefined ? undefined : canonicalTimeZone(String(opts.timeZone)),
        weekday: pick("weekday", ["long", "short", "narrow"]),
        era: pick("era", ["long", "short", "narrow"]),
        year: pick("year", ["numeric", "2-digit"]),
        month: pick("month", ["numeric", "2-digit", "long", "short", "narrow"]),
        day: pick("day", ["numeric", "2-digit"]),
        hour: pick("hour", ["numeric", "2-digit"]),
        minute: pick("minute", ["numeric", "2-digit"]),
        second: pick("second", ["numeric", "2-digit"]),
        fractionalSecondDigits: opts.fractionalSecondDigits === undefined ? undefined : Number(opts.fractionalSecondDigits),
        timeZoneName: pick("timeZoneName", ["short", "long", "shortOffset", "longOffset", "shortGeneric", "longGeneric"]),
        dayPeriod: pick("dayPeriod", ["narrow", "short", "long"]),
        hour12: opts.hour12 === undefined ? (opts.hourCycle === undefined ? undefined : opts.hourCycle === "h12" || opts.hourCycle === "h11") : Boolean(opts.hour12),
        dateStyle: pick("dateStyle", ["full", "long", "medium", "short"]),
        timeStyle: pick("timeStyle", ["full", "long", "medium", "short"]),
      }
      const anyField = ["weekday", "era", "year", "month", "day", "dayPeriod", "hour", "minute", "second", "fractionalSecondDigits"].some((k) => o[k] !== undefined)
      if ((o.dateStyle || o.timeStyle) && anyField) throw new TypeError("Can't set option dateStyle when other date-time options are set")
      if (!anyField && !o.dateStyle && !o.timeStyle) {
        o.year = "numeric"
        o.month = "numeric"
        o.day = "numeric"
        o.defaulted = true
      }
      this.#o = o
    }
    #parts(date) {
      const d = date === undefined ? new Date() : new Date(typeof date === "object" ? date.getTime() : Number(date))
      if (Number.isNaN(d.getTime())) throw new RangeError("Invalid time value")
      const o = this.#o
      const L = dateData(o.locale)
      const offset = zoneOffset(o.timeZone, d)
      const f = fields(d, offset)
      const parts = []
      const lit = (v) => {
        if (v !== "") parts.push({ type: "literal", value: v })
      }
      const h12 = o.hour12 === undefined ? L.h12 : o.hour12
      const hourText = (style) => {
        let h = f.hour
        if (h12) h = h % 12 === 0 ? 12 : h % 12
        return style === "2-digit" ? pad2(h) : String(h)
      }
      const tzName = () => zoneName(o.timeZone ?? tzSystem(), o.timeZoneName ?? "short", offset, d)
      const monthName = (style) => {
        if (style === "short") return L.short[f.month]
        if (style === "narrow") return [...L.months[f.month]][0]
        return L.months[f.month]
      }
      const weekdayName = (style) => (style === "long" ? L.days[f.weekday] : style === "short" ? L.daysShort[f.weekday] : [...L.days[f.weekday]][0])
      const values = (monthStyle, dayStyle, yearStyle, weekdayStyle) => ({
        weekday: weekdayStyle ? weekdayName(weekdayStyle) : "",
        month: monthStyle === "numeric" || monthStyle === "2-digit" ? String(f.month + 1) : monthName(monthStyle),
        gen: monthStyle === "short" ? L.short[f.month] : monthStyle === "narrow" ? [...L.months[f.month]][0] : (L.gen ?? L.months)[f.month],
        short: L.short[f.month],
        day: dayStyle === "2-digit" ? pad2(f.day) : String(f.day),
        dd: pad2(f.day),
        d: String(f.day),
        MM: pad2(f.month + 1),
        M: String(f.month + 1),
        year: yearStyle === "2-digit" ? pad2(f.year % 100) : String(f.year),
        yy: pad2(f.year % 100),
        fullYear: String(f.year),
      })
      const typeOf = { weekday: "weekday", month: "month", gen: "month", short: "month", day: "day", dd: "day", d: "day", MM: "month", M: "month", year: "year", yy: "year", fullYear: "year" }
      const render = (template, v) => {
        const re = /\{(\w+)\}/g
        let last = 0
        let m
        while ((m = re.exec(template))) {
          lit(template.slice(last, m.index))
          const val = v[m[1]]
          if (val !== undefined && val !== "") parts.push({ type: typeOf[m[1]] ?? "literal", value: val })
          last = re.lastIndex
        }
        lit(template.slice(last))
      }
      const trimParts = () => {
        while (parts.length && parts[0].type === "literal" && parts[0].value.trim() === "") parts.shift()
        if (parts.length && parts[0].type === "literal") parts[0].value = parts[0].value.replace(/^[,\s]+/, "")
        if (parts.length && parts[parts.length - 1].type === "literal" && parts[parts.length - 1].value.trim() === "") parts.pop()
      }
      const numericDate = (monthStyle, dayStyle, yearStyle) => {
        const segs = []
        const padMonth = monthStyle === "2-digit" || L.pad === true || L.pad === "month"
        const padDay = dayStyle === "2-digit" || L.pad === true
        for (const k of L.order) {
          if (k === "m" && monthStyle) segs.push({ type: "month", value: padMonth ? pad2(f.month + 1) : String(f.month + 1) })
          if (k === "d" && dayStyle) segs.push({ type: "day", value: padDay ? pad2(f.day) : String(f.day) })
          if (k === "y" && yearStyle) segs.push({ type: "year", value: yearStyle === "2-digit" ? pad2(f.year % 100) : String(f.year) })
        }
        segs.forEach((seg, i) => {
          if (i > 0) lit(L.sep)
          parts.push(seg)
        })
      }
      const datePart = (style) => {
        if (style === "full") render(L.full, values("long", "numeric", "numeric", "long"))
        else if (style === "long") render(L.long, values("long", "numeric", "numeric"))
        else if (style === "medium") render(L.medium, values("short", "numeric", "numeric"))
        else render(L.shortDate, values("numeric", "numeric", "2-digit"))
      }
      const timePart = (style) => {
        parts.push({ type: "hour", value: hourText(h12 ? "numeric" : "2-digit") })
        lit(":")
        parts.push({ type: "minute", value: pad2(f.minute) })
        if (style !== "short") {
          lit(":")
          parts.push({ type: "second", value: pad2(f.second) })
        }
        if (h12) {
          lit(" ")
          parts.push({ type: "dayPeriod", value: f.hour < 12 ? L.am : L.pm })
        }
        if (style === "full" || style === "long") {
          lit(" ")
          parts.push({ type: "timeZoneName", value: zoneName(o.timeZone ?? tzSystem(), style === "full" ? "long" : "short", offset, d) })
        }
      }
      if (o.dateStyle || o.timeStyle) {
        if (o.dateStyle) datePart(o.dateStyle)
        if (o.dateStyle && o.timeStyle) lit(o.dateStyle === "full" || o.dateStyle === "long" ? L.at : L.dtsep)
        if (o.timeStyle) timePart(o.timeStyle)
        return parts
      }
      const hasDate = o.year || o.month || o.day || o.weekday
      const textMonth = o.month === "long" || o.month === "short" || o.month === "narrow"
      const weekdayFirst = o.weekday && !L.weekdayAfter
      if (weekdayFirst) {
        parts.push({ type: "weekday", value: weekdayName(o.weekday) })
        if (o.year || o.month || o.day) lit(L.wsep)
      }
      if (textMonth) {
        const v = values(o.month, o.day, o.year)
        if (o.day && o.year) render(L.mdy, v)
        else if (o.day) render(L.md, v)
        else if (o.year) render(L.my, v)
        else parts.push({ type: "month", value: monthName(o.month) })
      } else if (o.year || o.month || o.day) {
        if (L.textNumeric && o.year && o.month && o.day) render(L.long, values("numeric", o.day, o.year))
        else if (L.textNumeric && o.month && o.day) render(L.md, values("numeric", o.day, o.year))
        else numericDate(o.month, o.day, o.year)
      }
      if (o.weekday && L.weekdayAfter) {
        if (o.year || o.month || o.day) lit(L.wsep)
        parts.push({ type: "weekday", value: weekdayName(o.weekday) })
      }
      if (o.era) {
        const names = o.era === "long" ? ["Before Christ", "Anno Domini"] : o.era === "narrow" ? ["B", "A"] : ["BC", "AD"]
        if (hasDate) lit(" ")
        parts.push({ type: "era", value: names[f.year > 0 ? 1 : 0] })
      }
      const dayPeriodName = () => {
        const h = f.hour
        if (h === 12) return "noon"
        if (h < 6 || h >= 21) return "at night"
        if (h < 12) return "in the morning"
        if (h < 18) return "in the afternoon"
        return "in the evening"
      }
      const hasTime = o.hour || o.minute || o.second
      if (hasTime) {
        if (hasDate) lit(L.dtsep)
        const items = []
        if (o.hour) items.push({ type: "hour", value: hourText(o.hour === "2-digit" || (!h12 && o.minute) ? "2-digit" : "numeric") })
        if (o.minute) items.push({ type: "minute", value: o.hour || o.second ? pad2(f.minute) : String(f.minute) })
        if (o.second) items.push({ type: "second", value: o.minute ? pad2(f.second) : String(f.second) })
        items.forEach((seg, i) => {
          if (i > 0) lit(":")
          parts.push(seg)
        })
        if (o.fractionalSecondDigits) {
          lit(".")
          parts.push({ type: "fractionalSecond", value: String(f.ms).padStart(3, "0").slice(0, o.fractionalSecondDigits) })
        }
        if (o.hour && h12) {
          lit(" ")
          parts.push({ type: "dayPeriod", value: o.dayPeriod ? dayPeriodName() : f.hour < 12 ? L.am : L.pm })
        }
      }
      if (o.timeZoneName) {
        lit(hasDate && !hasTime ? L.dtsep : " ")
        parts.push({ type: "timeZoneName", value: tzName() })
      }
      trimParts()
      const digits = nuDigits(o.numberingSystem)
      if (digits === null) return parts
      return parts.map((p) => ({ type: p.type, value: p.value.replace(/[0-9]/g, (c) => digits[c.charCodeAt(0) - 48]) }))
    }
    get format() {
      return (date) => this.#parts(date).map((p) => p.value).join("")
    }
    formatToParts(date) {
      return this.#parts(date)
    }
    formatRange(a, b) {
      return `${this.format(a)} – ${this.format(b)}`
    }
    resolvedOptions() {
      const o = this.#o
      const out = { locale: o.locale, calendar: o.calendar, numberingSystem: o.numberingSystem, timeZone: o.timeZone ?? tzSystem() }
      if (o.hour !== undefined || o.timeStyle !== undefined) {
        const h12 = o.hour12 === undefined ? dateData(o.locale).h12 : o.hour12
        out.hourCycle = h12 ? "h12" : "h23"
        out.hour12 = h12
      }
      for (const k of ["weekday", "era", "year", "month", "day", "dayPeriod", "hour", "minute", "second", "fractionalSecondDigits", "timeZoneName", "dateStyle", "timeStyle"]) {
        if (o[k] !== undefined) out[k] = o[k]
      }
      return out
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  const fold = (s) => s.normalize("NFD").replace(/[̀-ͯ]/g, "")
  const tailoring = (letters) => {
    const map = {}
    letters.forEach((c, i) => {
      const tail = String.fromCharCode(0x7b + i)
      map[c] = "z" + tail
      map[c.toUpperCase()] = "Z" + tail
    })
    return { re: new RegExp(`[${Object.keys(map).join("")}]`, "g"), map }
  }
  const NORDIC_SV = tailoring(["å", "ä", "ö"])
  const NORDIC_DA = tailoring(["æ", "ø", "å"])
  const COLLATION_TAILORING = { sv: NORDIC_SV, fi: NORDIC_SV, da: NORDIC_DA, nb: NORDIC_DA, nn: NORDIC_DA, no: NORDIC_DA }
  const COLLATIONS = { de: ["phonebk"], es: ["trad"], zh: ["big5han", "gb2312", "pinyin", "stroke", "unihan", "zhuyin"], ja: ["unihan"], ko: ["searchjl", "unihan"] }
  const collationSupported = (base, co) => co === "emoji" || co === "eor" || (COLLATIONS[lang(base)] ?? []).includes(co)
  const PHONEBOOK = { "ä": "a\u0308e", "ö": "o\u0308e", "ü": "u\u0308e", "Ä": "A\u0308e", "Ö": "O\u0308e", "Ü": "U\u0308e" }
  class Collator {
    #o
    #bound
    constructor(locales, options) {
      const list = localeList(locales)
      if (options === null) throw new TypeError("Cannot convert undefined or null to object")
      const opts = options === undefined ? Object.create(null) : Object(options)
      const usage = getOption(opts, "usage", ["sort", "search"], "sort")
      getOption(opts, "localeMatcher", ["lookup", "best fit"], "best fit")
      const collationOpt = getOption(opts, "collation", undefined, undefined)
      if (collationOpt !== undefined && !/^[a-zA-Z0-9]{3,8}(-[a-zA-Z0-9]{3,8})*$/.test(collationOpt)) throw new RangeError(`Invalid collation : ${collationOpt}`)
      const numericOpt = opts.numeric === undefined ? undefined : String(Boolean(opts.numeric))
      const caseFirstOpt = getOption(opts, "caseFirst", ["upper", "lower", "false"], undefined)
      const requested = list.find((l) => KNOWN_LANGUAGES.has(l.split("-")[0].toLowerCase())) ?? "en-US"
      const { base, keywords } = parseExtension(requested)
      const additions = {}
      let collation = "default"
      if (keywords.co !== undefined && collationSupported(base, keywords.co)) {
        collation = keywords.co
        additions.co = `-co-${keywords.co}`
      }
      if (collationOpt !== undefined && collationSupported(base, collationOpt) && collationOpt !== collation) {
        collation = collationOpt
        delete additions.co
      }
      let caseFirst = "false"
      if (keywords.kf !== undefined && ["upper", "lower", "false"].includes(keywords.kf)) {
        caseFirst = keywords.kf
        additions.kf = `-kf-${keywords.kf}`
      }
      if (caseFirstOpt !== undefined && caseFirstOpt !== caseFirst) {
        caseFirst = caseFirstOpt
        delete additions.kf
      }
      let numeric = "false"
      if (keywords.kn === "true" || keywords.kn === "false") {
        numeric = keywords.kn
        additions.kn = keywords.kn === "true" ? "-kn" : "-kn-false"
      }
      if (numericOpt !== undefined && numericOpt !== numeric) {
        numeric = numericOpt
        delete additions.kn
      }
      const ext = ["co", "kf", "kn"].map((k) => additions[k] ?? "").join("")
      const locale = ext ? `${base}-u${ext}` : base
      const sensitivity = getOption(opts, "sensitivity", ["base", "accent", "case", "variant"], undefined) ?? "variant"
      const ignorePunctuation = opts.ignorePunctuation === undefined ? lang(base) === "th" : Boolean(opts.ignorePunctuation)
      this.#o = { locale, usage, sensitivity, ignorePunctuation, collation, numeric: numeric === "true", caseFirst }
    }
    static #unwrap(x, method) {
      if (x === null || (typeof x !== "object" && typeof x !== "function") || !(#o in x)) {
        throw new TypeError(`Method Intl.Collator.prototype.${method} called on incompatible receiver ${typeof x === "object" && x !== null ? "#<Object>" : String(x)}`)
      }
      return x
    }
    #compare(a, b) {
      const o = this.#o
      const prepare = (v) => {
        let t = v.normalize("NFC")
        if (o.ignorePunctuation) t = t.replace(/[\p{P}\s]/gu, "")
        const tailor = COLLATION_TAILORING[lang(o.locale)]
        if (tailor) t = t.replace(tailor.re, (c) => tailor.map[c])
        if (o.collation === "phonebk" || (o.usage === "search" && lang(o.locale) === "de")) t = t.replace(/[äöüÄÖÜ]/g, (c) => PHONEBOOK[c])
        return t.normalize("NFD")
      }
      const x = prepare(a)
      const y = prepare(b)
      const unmarked = (t) => t.replace(/\p{M}/gu, "")
      const cmp = (p, q) => {
        if (o.numeric) {
          const re = /(\d+)|(\D+)/g
          const ps = p.match(re) ?? []
          const qs = q.match(re) ?? []
          for (let i = 0; i < Math.min(ps.length, qs.length); i++) {
            const pa = ps[i]
            const qa = qs[i]
            if (/^\d/.test(pa) && /^\d/.test(qa)) {
              const d = BigInt(pa) - BigInt(qa)
              if (d !== 0n) return d < 0n ? -1 : 1
            } else if (pa !== qa) return pa < qa ? -1 : 1
          }
          return ps.length === qs.length ? 0 : ps.length < qs.length ? -1 : 1
        }
        return p === q ? 0 : p < q ? -1 : 1
      }
      const r1 = cmp(unmarked(x).toLowerCase(), unmarked(y).toLowerCase())
      if (r1 !== 0 || o.sensitivity === "base") return r1
      if (o.sensitivity !== "case") {
        const r2 = cmp(x.toLowerCase(), y.toLowerCase())
        if (r2 !== 0) return r2
        if (o.sensitivity === "accent") return 0
      }
      const p = unmarked(x)
      const q = unmarked(y)
      for (let i = 0; i < Math.min(p.length, q.length); i++) {
        if (p[i] !== q[i]) {
          const lower = p[i] === p[i].toLowerCase()
          if (o.caseFirst === "upper") return lower ? 1 : -1
          return lower ? -1 : 1
        }
      }
      return p.length === q.length ? 0 : p.length < q.length ? -1 : 1
    }
    get compare() {
      const c = Collator.#unwrap(this, "compare")
      if (c.#bound === undefined) {
        const bound = (x, y) => c.#compare(String(x), String(y))
        Object.defineProperty(bound, "name", { value: "", configurable: true })
        c.#bound = bound
      }
      return c.#bound
    }
    resolvedOptions() {
      return { ...Collator.#unwrap(this, "resolvedOptions").#o }
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  const pluralCardinal = (l, n, i, v) => {
    switch (l) {
      case "fr":
      case "pt":
        return i === 0 || i === 1 ? "one" : "other"
      case "ar": {
        if (n === 0) return "zero"
        if (n === 1) return "one"
        if (n === 2) return "two"
        const m100 = i % 100
        if (v === 0 && m100 >= 3 && m100 <= 10) return "few"
        if (v === 0 && m100 >= 11 && m100 <= 99) return "many"
        return "other"
      }
      case "he":
        return n === 1 ? "one" : n === 2 && v === 0 ? "two" : "other"
      case "cs":
      case "sk":
        if (v !== 0) return "many"
        return i === 1 ? "one" : i >= 2 && i <= 4 ? "few" : "other"
      case "uk":
      case "ru":
        if (v !== 0) return "other"
        if (i % 10 === 1 && i % 100 !== 11) return "one"
        if (i % 10 >= 2 && i % 10 <= 4 && (i % 100 < 12 || i % 100 > 14)) return "few"
        return "many"
      case "pl":
        if (v !== 0) return "other"
        if (i === 1) return "one"
        if (i % 10 >= 2 && i % 10 <= 4 && (i % 100 < 12 || i % 100 > 14)) return "few"
        return "many"
      case "ja":
      case "zh":
      case "ko":
      case "tr":
        return l === "tr" && n === 1 ? "one" : "other"
      default:
        return n === 1 && v === 0 ? "one" : "other"
    }
  }
  class PluralRules {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      this.#o = {
        locale: resolve(locales),
        type: opts.type === undefined ? "cardinal" : String(opts.type),
        minimumIntegerDigits: 1,
        minimumFractionDigits: 0,
        maximumFractionDigits: 3,
        pluralCategories: [],
      }
      const l = lang(this.#o.locale)
      this.#o.pluralCategories = this.#o.type === "ordinal" ? (l === "en" ? ["few", "one", "two", "other"] : ["other"]) : l === "uk" || l === "ru" || l === "pl" ? ["few", "many", "one", "other"] : l === "ja" || l === "zh" || l === "ko" ? ["other"] : ["one", "other"]
    }
    select(value) {
      const n = Math.abs(Number(value))
      const l = lang(this.#o.locale)
      if (this.#o.type === "ordinal") {
        if (l !== "en") return "other"
        const m10 = n % 10
        const m100 = n % 100
        if (m10 === 1 && m100 !== 11) return "one"
        if (m10 === 2 && m100 !== 12) return "two"
        if (m10 === 3 && m100 !== 13) return "few"
        return "other"
      }
      const s = String(n)
      const v = s.includes(".") ? s.split(".")[1].length : 0
      return pluralCardinal(l, n, Math.trunc(n), v)
    }
    selectRange(a, b) {
      return this.select(b)
    }
    resolvedOptions() {
      return { ...this.#o, pluralCategories: [...this.#o.pluralCategories] }
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  const RT_UNITS = ["year", "quarter", "month", "week", "day", "hour", "minute", "second"]
  const RT_SHORT = { year: "yr.", quarter: "qtr.", month: "mo.", week: "wk.", day: "day", hour: "hr.", minute: "min.", second: "sec." }
  const RT_AUTO = {
    day: { "-1": "yesterday", 0: "today", 1: "tomorrow" },
    year: { "-1": "last year", 0: "this year", 1: "next year" },
    month: { "-1": "last month", 0: "this month", 1: "next month" },
    week: { "-1": "last week", 0: "this week", 1: "next week" },
    quarter: { "-1": "last quarter", 0: "this quarter", 1: "next quarter" },
    hour: { 0: "this hour" },
    minute: { 0: "this minute" },
    second: { 0: "now" },
  }
  const RT_LOCALES = {
    uk: {
      future: "через {n} {u}",
      past: "{n} {u} тому",
      units: { year: ["рік", "роки", "років"], quarter: ["квартал", "квартали", "кварталів"], month: ["місяць", "місяці", "місяців"], week: ["тиждень", "тижні", "тижнів"], day: ["день", "дні", "днів"], hour: ["годину", "години", "годин"], minute: ["хвилину", "хвилини", "хвилин"], second: ["секунду", "секунди", "секунд"] },
      short: { year: "р.", quarter: "кв.", month: "міс.", week: "тиж.", day: "дн.", hour: "год.", minute: "хв.", second: "с" },
      auto: { day: { "-2": "позавчора", "-1": "вчора", 0: "сьогодні", 1: "завтра", 2: "післязавтра" }, year: { "-1": "минулого року", 0: "цього року", 1: "наступного року" }, month: { "-1": "минулого місяця", 0: "цього місяця", 1: "наступного місяця" }, week: { "-1": "минулого тижня", 0: "цього тижня", 1: "наступного тижня" }, quarter: { "-1": "минулого кварталу", 0: "цього кварталу", 1: "наступного кварталу" }, hour: { 0: "цієї години" }, minute: { 0: "цієї хвилини" }, second: { 0: "зараз" } },
    },
    ru: {
      future: "через {n} {u}",
      past: "{n} {u} назад",
      units: { year: ["год", "года", "лет"], quarter: ["квартал", "квартала", "кварталов"], month: ["месяц", "месяца", "месяцев"], week: ["неделю", "недели", "недель"], day: ["день", "дня", "дней"], hour: ["час", "часа", "часов"], minute: ["минуту", "минуты", "минут"], second: ["секунду", "секунды", "секунд"] },
      short: { year: "г.", quarter: "кв.", month: "мес.", week: "нед.", day: "дн.", hour: "ч.", minute: "мин.", second: "с" },
      auto: { day: { "-2": "позавчера", "-1": "вчера", 0: "сегодня", 1: "завтра", 2: "послезавтра" }, year: { "-1": "в прошлом году", 0: "в этом году", 1: "в следующем году" }, month: { "-1": "в прошлом месяце", 0: "в этом месяце", 1: "в следующем месяце" }, week: { "-1": "на прошлой неделе", 0: "на этой неделе", 1: "на следующей неделе" }, second: { 0: "сейчас" } },
    },
    de: {
      future: "in {n} {u}",
      past: "vor {n} {u}",
      units: { year: ["Jahr", "Jahren", "Jahren"], quarter: ["Quartal", "Quartalen", "Quartalen"], month: ["Monat", "Monaten", "Monaten"], week: ["Woche", "Wochen", "Wochen"], day: ["Tag", "Tagen", "Tagen"], hour: ["Stunde", "Stunden", "Stunden"], minute: ["Minute", "Minuten", "Minuten"], second: ["Sekunde", "Sekunden", "Sekunden"] },
      short: { year: "J.", quarter: "Quart.", month: "Mon.", week: "Wo.", day: "Tg.", hour: "Std.", minute: "Min.", second: "Sek." },
      auto: { day: { "-2": "vorgestern", "-1": "gestern", 0: "heute", 1: "morgen", 2: "übermorgen" }, year: { "-1": "letztes Jahr", 0: "dieses Jahr", 1: "nächstes Jahr" }, month: { "-1": "letzten Monat", 0: "diesen Monat", 1: "nächsten Monat" }, week: { "-1": "letzte Woche", 0: "diese Woche", 1: "nächste Woche" }, second: { 0: "jetzt" } },
    },
    fr: {
      future: "dans {n} {u}",
      past: "il y a {n} {u}",
      units: { year: ["an", "ans", "ans"], quarter: ["trimestre", "trimestres", "trimestres"], month: ["mois", "mois", "mois"], week: ["semaine", "semaines", "semaines"], day: ["jour", "jours", "jours"], hour: ["heure", "heures", "heures"], minute: ["minute", "minutes", "minutes"], second: ["seconde", "secondes", "secondes"] },
      short: { year: "a", quarter: "trim.", month: "m.", week: "sem.", day: "j", hour: "h", minute: "min", second: "s" },
      auto: { day: { "-2": "avant-hier", "-1": "hier", 0: "aujourd’hui", 1: "demain", 2: "après-demain" }, year: { "-1": "l’année dernière", 0: "cette année", 1: "l’année prochaine" }, month: { "-1": "le mois dernier", 0: "ce mois-ci", 1: "le mois prochain" }, week: { "-1": "la semaine dernière", 0: "cette semaine", 1: "la semaine prochaine" }, second: { 0: "maintenant" } },
    },
    es: {
      future: "dentro de {n} {u}",
      past: "hace {n} {u}",
      units: { year: ["año", "años", "años"], quarter: ["trimestre", "trimestres", "trimestres"], month: ["mes", "meses", "meses"], week: ["semana", "semanas", "semanas"], day: ["día", "días", "días"], hour: ["hora", "horas", "horas"], minute: ["minuto", "minutos", "minutos"], second: ["segundo", "segundos", "segundos"] },
      short: { year: "a", quarter: "trim.", month: "m", week: "sem.", day: "d", hour: "h", minute: "min", second: "s" },
      auto: { day: { "-2": "anteayer", "-1": "ayer", 0: "hoy", 1: "mañana", 2: "pasado mañana" }, year: { "-1": "el año pasado", 0: "este año", 1: "el próximo año" }, month: { "-1": "el mes pasado", 0: "este mes", 1: "el próximo mes" }, week: { "-1": "la semana pasada", 0: "esta semana", 1: "la próxima semana" }, second: { 0: "ahora" } },
    },
    pl: {
      future: "za {n} {u}",
      past: "{n} {u} temu",
      units: { year: ["rok", "lata", "lat"], quarter: ["kwartał", "kwartały", "kwartałów"], month: ["miesiąc", "miesiące", "miesięcy"], week: ["tydzień", "tygodnie", "tygodni"], day: ["dzień", "dni", "dni"], hour: ["godzinę", "godziny", "godzin"], minute: ["minutę", "minuty", "minut"], second: ["sekundę", "sekundy", "sekund"] },
      short: { year: "rok", quarter: "kw.", month: "mies.", week: "tyg.", day: "dzień", hour: "godz.", minute: "min", second: "s" },
      auto: { day: { "-2": "przedwczoraj", "-1": "wczoraj", 0: "dzisiaj", 1: "jutro", 2: "pojutrze" }, year: { "-1": "w zeszłym roku", 0: "w tym roku", 1: "w przyszłym roku" }, month: { "-1": "w zeszłym miesiącu", 0: "w tym miesiącu", 1: "w przyszłym miesiącu" }, week: { "-1": "w zeszłym tygodniu", 0: "w tym tygodniu", 1: "w przyszłym tygodniu" }, second: { 0: "teraz" } },
    },
  }
  const slavicForm = (n) => {
    const i = Math.trunc(Math.abs(n))
    if (i !== Math.abs(n)) return 1
    const m10 = i % 10
    const m100 = i % 100
    if (m10 === 1 && m100 !== 11) return 0
    if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return 1
    return 2
  }
  class RelativeTimeFormat {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      const nuOpt = opts.numberingSystem === undefined ? undefined : String(opts.numberingSystem)
      if (nuOpt !== undefined && !/^[a-zA-Z0-9]{3,8}(-[a-zA-Z0-9]{3,8})*$/.test(nuOpt)) throw new RangeError(`Invalid numberingSystem : ${nuOpt}`)
      this.#o = {
        locale: resolve(locales),
        style: opts.style === undefined ? "long" : String(opts.style),
        numeric: opts.numeric === undefined ? "always" : String(opts.numeric),
        numberingSystem: resolveNumberLocale(localeList(locales), nuOpt).nu,
      }
    }
    format(value, unit) {
      const n = Number(value)
      let u = String(unit)
      if (u.endsWith("s")) u = u.slice(0, -1)
      if (!RT_UNITS.includes(u)) throw new RangeError(`Invalid unit argument for format() '${unit}'`)
      const l = lang(this.#o.locale)
      const data = RT_LOCALES[l]
      if (this.#o.numeric === "auto") {
        const t = (data ? data.auto : RT_AUTO)[u]?.[String(n)]
        if (t) return t
      }
      const abs = Math.abs(n)
      const num = new NumberFormat(this.#o.locale, { numberingSystem: this.#o.numberingSystem }).format(abs)
      const past = n < 0 || Object.is(n, -0)
      if (data) {
        const forms = data.units[u]
        const form = l === "uk" || l === "ru" || l === "pl" ? slavicForm(abs) : abs === 1 ? 0 : 1
        const label = this.#o.style === "long" ? forms[form] : data.short[u]
        return (past ? data.past : data.future).replace("{n}", num).replace("{u}", label)
      }
      const label = this.#o.style === "long" ? (abs === 1 ? u : u + "s") : RT_SHORT[u] === "day" ? (abs === 1 ? "day" : "days") : RT_SHORT[u]
      return past ? `${num} ${label} ago` : `in ${num} ${label}`
    }
    formatToParts(value, unit) {
      return [{ type: "literal", value: this.format(value, unit) }]
    }
    resolvedOptions() {
      return { ...this.#o }
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  const LIST_WORDS = {
    en: { and: "and", or: "or", sep: ", " },
    de: { and: "und", or: "oder", sep: ", " },
    fr: { and: "et", or: "ou", sep: ", " },
    es: { and: "y", or: "o", sep: ", " },
    it: { and: "e", or: "o", sep: ", " },
    pt: { and: "e", or: "ou", sep: ", " },
    nl: { and: "en", or: "of", sep: ", " },
    uk: { and: "і", or: "або", sep: ", " },
    ru: { and: "и", or: "или", sep: ", " },
    pl: { and: "i", or: "lub", sep: ", " },
    sv: { and: "och", or: "eller", sep: ", " },
    tr: { and: "ve", or: "veya", sep: ", " },
    ko: { and: "및", or: "또는", sep: ", " },
    ja: { and: "、", or: "、または", sep: "、" },
    zh: { and: "和", or: "或", sep: "、" },
  }
  class ListFormat {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      this.#o = {
        locale: resolve(locales),
        type: opts.type === undefined ? "conjunction" : String(opts.type),
        style: opts.style === undefined ? "long" : String(opts.style),
      }
    }
    format(list) {
      const items = [...list].map(String)
      const o = this.#o
      const l = lang(o.locale)
      const words = LIST_WORDS[l] ?? LIST_WORDS.en
      if (items.length === 0) return ""
      if (items.length === 1) return items[0]
      if (o.type === "unit") return items.join(o.style === "narrow" ? " " : words.sep)
      if (l === "ja" || l === "zh") {
        if (o.type === "disjunction") return items.slice(0, -1).join(words.sep) + words.or + items[items.length - 1]
        return l === "zh" ? items.slice(0, -1).join(words.sep) + words.and + items[items.length - 1] : items.join(words.sep)
      }
      const word = o.type === "disjunction" ? words.or : o.style === "long" || l !== "en" ? words.and : "&"
      const oxford = l === "en" && o.type !== "unit"
      if (items.length === 2) return `${items[0]} ${word} ${items[1]}`
      return `${items.slice(0, -1).join(words.sep)}${oxford ? "," : ""} ${word} ${items[items.length - 1]}`
    }
    formatToParts(list) {
      return [{ type: "literal", value: this.format(list) }]
    }
    resolvedOptions() {
      return { ...this.#o }
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  class Segmenter {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      this.#o = { locale: resolve(locales), granularity: opts.granularity === undefined ? "grapheme" : String(opts.granularity) }
    }
    segment(input) {
      const s = String(input)
      const g = this.#o.granularity
      const out = []
      if (g === "word") {
        const re = /[\p{L}\p{N}_']+|\s+|[^\p{L}\p{N}_'\s]/gu
        let m
        while ((m = re.exec(s))) {
          out.push({ segment: m[0], index: m.index, input: s, isWordLike: /[\p{L}\p{N}]/u.test(m[0]) })
        }
      } else if (g === "sentence") {
        const re = /[^.!?]+[.!?]*\s*/g
        let m
        while ((m = re.exec(s))) out.push({ segment: m[0], index: m.index, input: s })
      } else {
        let index = 0
        const chars = [...s]
        for (let i = 0; i < chars.length; i++) {
          let seg = chars[i]
          while (i + 1 < chars.length && /[̀-ͯ‍️\u{1f3fb}-\u{1f3ff}]/u.test(chars[i + 1])) {
            seg += chars[++i]
            if (seg.endsWith("‍") && i + 1 < chars.length) seg += chars[++i]
          }
          out.push({ segment: seg, index, input: s })
          index += seg.length
        }
      }
      return {
        [Symbol.iterator]: () => out[Symbol.iterator](),
        containing: (i) => out.find((x) => i >= x.index && i < x.index + x.segment.length),
      }
    }
    resolvedOptions() {
      return { ...this.#o }
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  class DisplayNames {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      if (opts.type === undefined) throw new TypeError("Required option 'type' is missing")
      this.#o = { locale: resolve(locales), type: String(opts.type), style: "long", fallback: "code" }
    }
    of(code) {
      const tables = {
        language: { en: "English", de: "German", fr: "French", es: "Spanish", it: "Italian", uk: "Ukrainian", ru: "Russian", pl: "Polish", ja: "Japanese", zh: "Chinese", ko: "Korean", pt: "Portuguese", nl: "Dutch", tr: "Turkish", sv: "Swedish", "en-US": "American English", "en-GB": "British English" },
        region: { US: "United States", GB: "United Kingdom", DE: "Germany", FR: "France", ES: "Spain", IT: "Italy", UA: "Ukraine", PL: "Poland", JP: "Japan", CN: "China", KR: "South Korea", BR: "Brazil", CA: "Canada", NL: "Netherlands", SE: "Sweden", TR: "Turkey" },
        currency: Object.fromEntries(Object.entries(CURRENCY).map(([k, v]) => [k, v[2].replace(/(^|\s)\S/g, (c) => c.toUpperCase()).replace(/s$/, "")])),
      }
      return tables[this.#o.type]?.[code] ?? code
    }
    resolvedOptions() {
      return { ...this.#o }
    }
    static supportedLocalesOf(locales) {
      return supportedList(locales)
    }
  }

  class Locale {
    #tag
    constructor(tag) {
      this.#tag = canonical(String(tag))
    }
    get baseName() {
      return this.#tag
    }
    get language() {
      return this.#tag.split("-")[0]
    }
    get region() {
      return this.#tag.split("-").find((p, i) => i > 0 && /^[A-Z]{2}$/.test(p))
    }
    get script() {
      return this.#tag.split("-").find((p, i) => i > 0 && /^[A-Z][a-z]{3}$/.test(p))
    }
    toString() {
      return this.#tag
    }
    maximize() {
      return this
    }
    minimize() {
      return this
    }
  }

  const callable = (C, chain = false) => {
    const f = function (...args) {
      if (new.target) return new.target !== f ? Reflect.construct(C, args, new.target) : new C(...args)
      const instance = new C(...args)
      if (chain && this !== undefined && this !== null && (typeof this === "object" || typeof this === "function") && Object.prototype.isPrototypeOf.call(C.prototype, this)) {
        Object.defineProperty(this, kFallback, { value: instance, writable: false, enumerable: false, configurable: false })
        return this
      }
      return instance
    }
    Object.defineProperty(f, "name", { value: C.name, configurable: true })
    Object.defineProperty(f, "length", { value: 0, configurable: true })
    Object.defineProperty(f, "prototype", { value: C.prototype, writable: false, enumerable: false, configurable: false })
    Object.defineProperty(C.prototype, "constructor", { value: f, writable: true, configurable: true })
    for (const k of Object.getOwnPropertyNames(C)) if (!["length", "name", "prototype"].includes(k)) Object.defineProperty(f, k, Object.getOwnPropertyDescriptor(C, k))
    return f
  }
  const Intl = {}
  for (const [name, value] of Object.entries({ NumberFormat: callable(NumberFormat, true), DateTimeFormat: callable(DateTimeFormat, true), Collator: callable(Collator), PluralRules, RelativeTimeFormat, ListFormat, Segmenter, DisplayNames, Locale })) {
    Object.defineProperty(Intl, name, { value, writable: true, configurable: true, enumerable: false })
    Object.defineProperty(value.prototype, Symbol.toStringTag, { value: `Intl.${name}`, configurable: true })
  }
  Object.defineProperty(Intl, "getCanonicalLocales", { value: (l) => localeList(l), writable: true, configurable: true })
  Object.defineProperty(Intl, "supportedValuesOf", {
    value: (key) => {
      const k = String(key)
      if (k === "currency") return Object.keys(CURRENCY).sort()
      if (k === "unit") return Object.keys(UNITS).sort()
      if (k === "calendar") return ["gregory"]
      if (k === "numberingSystem") return [...Object.keys(NU_ZERO), "hanidec"].sort()
      if (k === "timeZone") return tzList()
      if (k === "collation") return ["big5han", "emoji", "eor", "gb2312", "phonebk", "pinyin", "searchjl", "stroke", "trad", "unihan", "zhuyin"]
      throw new RangeError(`Invalid key : ${k}`)
    },
    writable: true,
    configurable: true,
  })
  Object.defineProperty(Intl, Symbol.toStringTag, { value: "Intl", configurable: true })
  return Intl
})()
"##;

fn tz_lookup(name: &str) -> Option<chrono_tz::Tz> {
    use std::str::FromStr;
    if let Ok(tz) = chrono_tz::Tz::from_str(name) {
        return Some(tz);
    }
    let lower = name.to_ascii_lowercase();
    chrono_tz::TZ_VARIANTS
        .iter()
        .copied()
        .find(|tz| tz.name().to_ascii_lowercase() == lower)
}

fn tz_info(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    use chrono::{Offset, TimeZone};
    use chrono_tz::OffsetName;
    let name = vm.to_string(&super::arg(args, 0))?;
    let t = vm.to_number(&super::arg(args, 1))?;
    let Some(tz) = tz_lookup(&name) else {
        return Ok(Value::Undefined);
    };
    if !t.is_finite() {
        return Ok(Value::Undefined);
    }
    let Some(utc) = chrono::DateTime::from_timestamp((t / 1000.0).floor() as i64, 0) else {
        return Ok(Value::Undefined);
    };
    let offset = tz.offset_from_utc_datetime(&utc.naive_utc());
    let minutes = offset.fix().local_minus_utc() / 60;
    let abbreviation = offset.abbreviation().unwrap_or("");
    Ok(vm.new_array(vec![
        Value::Number(minutes as f64),
        Value::from(abbreviation),
    ]))
}

fn tz_canonical(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    let name = vm.to_string(&super::arg(args, 0))?;
    Ok(match tz_lookup(&name) {
        Some(tz) => Value::from(tz.name()),
        None => Value::Undefined,
    })
}

fn tz_list(vm: &mut Vm, _this: Value, _args: &[Value]) -> Result<Value, Value> {
    let mut names: Vec<&str> = chrono_tz::TZ_VARIANTS
        .iter()
        .map(|tz| tz.name())
        .filter(|name| {
            name.contains('/')
                && !name.starts_with("Etc/")
                && !name.starts_with("SystemV/")
                && !name.starts_with("US/")
                && !name.starts_with("Canada/")
                && !name.starts_with("Mexico/")
                && !name.starts_with("Brazil/")
                && !name.starts_with("Chile/")
        })
        .collect();
    names.push("UTC");
    names.sort_unstable();
    names.dedup();
    let values = names.into_iter().map(Value::from).collect();
    Ok(vm.new_array(values))
}

fn tz_system(_vm: &mut Vm, _this: Value, _args: &[Value]) -> Result<Value, Value> {
    let name = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
    let utc = [
        "UTC",
        "GMT",
        "UCT",
        "Zulu",
        "Universal",
        "Greenwich",
        "GMT0",
        "GMT+0",
        "GMT-0",
    ];
    let bare = name.strip_prefix("Etc/").unwrap_or(&name);
    if utc.contains(&bare) || tz_lookup(&name).is_none() {
        return Ok(Value::from("UTC"));
    }
    Ok(Value::from(name))
}

pub fn load(vm: &mut Vm) -> Result<Value, Value> {
    if let Some(v) = vm.globals.get("Intl").cloned() {
        return Ok(v);
    }
    let natives: [(&str, u32, crate::object::NativeFn); 4] = [
        ("%tzInfo%", 2, tz_info),
        ("%tzCanonical%", 1, tz_canonical),
        ("%tzList%", 0, tz_list),
        ("%tzSystem%", 0, tz_system),
    ];
    for (name, arity, f) in natives {
        let v = super::native_fn(vm, name, arity, f);
        vm.define_global(name, v);
    }
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = match vm.compile(INTL_SRC, "<squirt:intl>", dir, Mode::Repl) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<squirt:intl>")),
    };
    let intl = vm.run_proto(proto)?;
    vm.define_global("Intl", intl.clone());
    vm.globals
        .attrs
        .insert("Intl".into(), crate::object::NON_ENUM);
    Ok(intl)
}
