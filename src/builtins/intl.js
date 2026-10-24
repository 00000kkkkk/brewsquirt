(() => {
  const LOC = globalThis["%intlLocale%"]
  const NBSP = " "
  const NNBSP = " "
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
    const c = LOC.canonicalize(tag)
    if (c === null) throw new RangeError(`Incorrect locale information provided`)
    return c
  }
  const localeList = (locales) => {
    if (locales === undefined) return []
    if (locales === null) throw new TypeError("Cannot convert undefined or null to object")
    const out = []
    const add = (c) => {
      for (let i = 0; i < out.length; i++) if (out[i] === c) return
      out[out.length] = c
    }
    if (typeof locales === "string" || LOC.isLocaleObject(locales)) {
      add(LOC.isLocaleObject(locales) ? LOC.localeTag(locales) : canonical(locales))
      return out
    }
    const source = Object(locales)
    const len = Math.min(Math.max(Math.trunc(Number(source.length)) || 0, 0), 2 ** 53 - 1)
    for (let k = 0; k < len; k++) {
      if (!(k in source)) continue
      const item = source[k]
      if (typeof item !== "string" && (item === null || typeof item !== "object")) throw new TypeError(`Language ID should be string or object.`)
      add(LOC.isLocaleObject(item) ? LOC.localeTag(item) : canonical(typeof item === "object" ? String(item) : item))
    }
    return out
  }
  const KNOWN_LANGUAGES = new Set("af am ar as az be bg bn bo br bs ca ce chr cs cy da de dsb dz ee el en eo es et eu fa ff fi fil fo fr fy ga gd gl gu gv ha haw he hi hr hsb hu hy ia id ig is it ja jv ka kk kl km kn ko kok ks ku kw ky lb lg ln lo lt lu lv mg mi mk ml mn mr ms mt my nb nd ne nl nn no om or os pa pl ps pt qu rm rn ro ru rw sa sc sd se sg si sk sl sn so sq sr su sv sw ta te tg th ti tk to tr tt ug uk ur uz vi wo xh yi yo yue zh zu".split(" "))
  const supportedList = (locales) => localeList(locales).filter((l) => KNOWN_LANGUAGES.has(l.split("-")[0].toLowerCase()))
  const resolve = (locales) => supportedList(locales)[0] ?? "en-US"
  const lang = (locale) => locale.split("-")[0]
  const getOption = (options, key, values, fallback) => {
    const v = options[key]
    if (v === undefined) return fallback
    const s = `${v}`
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
    if (i === -1) return { base: tag, keywords: { __proto__: null } }
    const base = parts.slice(0, i).join("-")
    const keywords = { __proto__: null }
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
    if (requested === undefined) return { locale: "en-US", nu: optNu !== undefined && nuSupported(optNu) ? optNu : defaultNumberingSystem("en-US") }
    const { base, keywords } = parseExtension(requested)
    let nu = defaultNumberingSystem(base)
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

  const numberSymbols = (locale, nu) => {
    const key = `${locale}\u0000${nu}`
    const cached = symbolCache.get(key)
    if (cached !== undefined) return cached
    const dataLocale = bestAvailable("NumberFormat", parseExtension(locale).base) ?? "en"
    const d = sectionData("num", dataLocale) ?? sectionData("num", "en")
    let sym = d
    if (nu !== undefined && nu !== d.nu) {
      sym = { ...d, ...(d.latn ?? {}) }
      if (nu === "arab") sym = { ...sym, ...ARAB_SYMBOLS }
      else if (nu === "arabext") sym = { ...sym, ...ARABEXT_SYMBOLS }
    }
    const result = { ...sym, dataLocale, rules: pluralRulesFor(dataLocale, "cardinal").rules }
    symbolCache.set(key, result)
    return result
  }
  const symbolCache = new Map()
  const ARAB_SYMBOLS = { decimal: "٫", group: "٬", currencyDecimal: "٫", currencyGroup: "٬", minus: "\u061c-", plus: "\u061c+", percentSign: "٪\u061c", exponent: "اس", exponentMinus: "\u061c-" }
  const ARABEXT_SYMBOLS = { decimal: "٫", group: "٬", currencyDecimal: "٫", currencyGroup: "٬", minus: "\u200e-\u200e", plus: "\u200e+\u200e", percentSign: "٪", exponent: "×۱۰^", exponentMinus: "\u200e-\u200e" }
  const BIDI_MARKS = /^([\u200e\u200f\u061c]*)(.*?)([\u200e\u200f\u061c]*)$/su
  const markedParts = (type, symbol, layer) => {
    const [, before, core, after] = BIDI_MARKS.exec(symbol)
    const out = []
    if (before) out.push({ type: "literal", value: before, layer })
    out.push({ type, value: core, layer })
    if (after) out.push({ type: "literal", value: after, layer })
    return out
  }
  const defaultNumberingSystem = (base) => sectionData("num", bestAvailable("NumberFormat", base) ?? "en")?.nu ?? "latn"
  const compactEntry = (o, mag) => {
    const table = numberSymbols(o.locale, o.numberingSystem).compact[o.compactDisplay === "long" ? "long" : "short"]
    return table[Math.max(0, Math.min(mag, table.length - 1))]
  }
  const exponentForMagnitude = (o, mag) => {
    if (o.notation === "scientific") return mag
    if (o.notation === "engineering") return Math.floor(mag / 3) * 3
    if (o.notation === "compact") {
      if (mag < 0) return 0
      const entry = compactEntry(o, mag)
      return entry ? entry[0] : 0
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
  const currencySymbol = (o, data) => {
    if (o.currencyDisplay === "code") return o.currency
    const symbol = data.sym?.[o.currency] ?? o.currency
    if (o.currencyDisplay === "narrowSymbol") return data.narrow?.[o.currency] ?? symbol
    return symbol
  }
  let CURRENCY_DIGITS
  const currencyDigits = (code) => {
    CURRENCY_DIGITS ??= sectionData("num", "#digits") ?? {}
    return CURRENCY_DIGITS[code] ?? 2
  }
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
  const INTL_DATA = globalThis["%intlData%"]
  const dataCache = new Map()
  let PARENTS
  const parentLocale = (tag, truncate) => {
    PARENTS ??= JSON.parse(INTL_DATA("meta", "parents") ?? "{}")
    if (!truncate && Object.hasOwn(PARENTS, tag)) return PARENTS[tag] === "root" ? undefined : PARENTS[tag]
    const i = tag.lastIndexOf("-")
    return i < 0 ? undefined : tag.slice(0, i)
  }
  const withLikelyScript = (tag) => {
    const parts = tag.split("-")
    if (parts.length < 2 || parts[1].length === 4) return tag
    try {
      const script = new LOC.Locale(tag).maximize().script
      if (script !== new LOC.Locale(parts[0]).maximize().script) return [parts[0], script, ...parts.slice(1)].join("-")
    } catch {}
    return tag
  }
  const sectionData = (section, tag) => {
    const key = `${section}\u0000${tag}`
    if (dataCache.has(key)) return dataCache.get(key)
    let t = withLikelyScript(tag)
    let value
    while (t !== undefined) {
      const raw = INTL_DATA(section, t)
      if (raw !== undefined) {
        value = JSON.parse(raw)
        while (typeof value === "string") value = JSON.parse(INTL_DATA(section, value))
        break
      }
      t = parentLocale(t, section === "plural")
    }
    dataCache.set(key, value)
    return value
  }
  const availCache = new Map()
  const availableSet = (service) => {
    let set = availCache.get(service)
    if (set === undefined) {
      set = new Set(JSON.parse(INTL_DATA("avail", service) ?? "[]"))
      availCache.set(service, set)
    }
    return set
  }
  const bestAvailable = (service, base) => {
    const set = availableSet(service)
    let t = base
    for (;;) {
      if (set.has(t)) return t
      let i = t.lastIndexOf("-")
      if (i < 0) return undefined
      if (i >= 2 && t[i - 2] === "-") i -= 2
      t = t.slice(0, i)
    }
  }
  const isObject = (v) => v !== null && (typeof v === "object" || typeof v === "function")
  const getOptionsObject = (options) => {
    if (options === undefined) return Object.create(null)
    if (isObject(options)) return options
    throw new TypeError("Options must be an object")
  }
  const coerceOptionsToObject = (options) => {
    if (options === undefined) return Object.create(null)
    if (options === null) throw new TypeError("Cannot convert undefined or null to object")
    return Object(options)
  }
  const getOpt = (options, key, type, values, fallback) => {
    let v = options[key]
    if (v === undefined) return fallback
    v = type === "boolean" ? Boolean(v) : `${v}`
    if (values !== undefined && !values.includes(v)) throw new RangeError(`Value ${v} out of range for options property ${key}`)
    return v
  }
  const LOCALE_MATCHERS = ["lookup", "best fit"]
  const resolveService = (service, requested, relevant) => {
    let found
    let keywords = { __proto__: null }
    for (const tag of requested) {
      const p = parseExtension(tag)
      found = bestAvailable(service, p.base)
      if (found !== undefined) {
        keywords = p.keywords
        break
      }
    }
    if (found === undefined) found = bestAvailable(service, "en-US") ?? "en"
    const result = { __proto__: null, locale: found, dataLocale: found }
    if (relevant !== undefined && "nu" in relevant) {
      let nu = defaultNumberingSystem(found)
      let addition = ""
      const ext = keywords.nu
      if (ext !== undefined && nuSupported(ext)) {
        nu = ext
        addition = `-u-nu-${ext}`
      }
      if (relevant.nu !== undefined) {
        const lower = relevant.nu.toLowerCase()
        if (nuSupported(lower) && lower !== nu) {
          nu = lower
          addition = ""
        }
      }
      result.nu = nu
      result.locale = found + addition
    }
    return result
  }
  const supportedLocalesOf = (service, locales, options) => {
    const requested = localeList(locales)
    const opts = coerceOptionsToObject(options)
    getOpt(opts, "localeMatcher", "string", LOCALE_MATCHERS, "best fit")
    const out = []
    for (const tag of requested) if (bestAvailable(service, parseExtension(tag).base) !== undefined) out.push(tag)
    return out
  }
  const receiverText = (x) => (isObject(x) ? "#<Object>" : typeof x === "string" ? x : String(x))
  const checkNumberingSystem = (nu) => {
    if (nu !== undefined && !/^[a-zA-Z0-9]{3,8}(-[a-zA-Z0-9]{3,8})*$/.test(nu)) throw new RangeError(`Invalid numberingSystem : ${nu}`)
    return nu
  }
  const ROUNDING_INCREMENTS = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 2500, 5000]
  const ROUNDING_MODES = ["ceil", "floor", "expand", "trunc", "halfCeil", "halfFloor", "halfExpand", "halfTrunc", "halfEven"]

  const digitOptions = (opts, mnfdDefault, mxfdDefault, notation) => {
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
    return { minimumIntegerDigits: mnid, ...digits, roundingType, roundingIncrement, roundingMode, roundingPriority: computedPriority, trailingZeroDisplay }
  }
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
      const d = digitOptions(opts, mnfdDefault, mxfdDefault, notation)
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
        ...d,
        useGrouping,
        notation,
        compactDisplay,
        signDisplay,
      }
      this.#data = numberSymbols(locale, nu)
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
      let compactForm
      if (nan) body.push({ type: "nan", value: data.nan, layer: "number" })
      else if (x.kind === "inf") body.push({ type: "infinity", value: data.infinity, layer: "number" })
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
        const money = o.style === "currency" && o.currencyDisplay !== "name"
        const primary = (money && data.currencyPrimary) || data.primary
        const secondary = (money && data.currencySecondary) || data.secondary
        let chunks = [int]
        if (g !== false && int.length >= primary + minGroup) {
          chunks = []
          let end = int.length
          let size = primary
          while (end > size) {
            chunks.unshift(int.slice(end - size, end))
            end -= size
            size = secondary
          }
          chunks.unshift(int.slice(0, end))
        }
        const groupSymbol = (money && data.currencyGroup) || data.group
        chunks.forEach((chunk, i) => {
          if (i > 0) body.push({ type: "group", value: groupSymbol, layer: "number" })
          body.push({ type: "integer", value: this.#tr(chunk), layer: "number" })
        })
        if (dot !== -1) {
          body.push({ type: "decimal", value: (money && data.currencyDecimal) || data.decimal, layer: "number" })
          body.push({ type: "fraction", value: this.#tr(r.str.slice(dot + 1)), layer: "number" })
        }
        if (o.notation === "scientific" || o.notation === "engineering") {
          inner.push({ type: "exponentSeparator", value: data.exponent, layer: "inner" })
          if (exponent < 0) inner.push(...markedParts("exponentMinusSign", data.exponentMinus, "inner"))
          inner.push({ type: "exponentInteger", value: this.#tr(String(Math.abs(exponent))), layer: "inner" })
        } else if (o.notation === "compact" && exponent > 0) {
          const entry = compactEntry(o, exponent + int.length - 1)
          if (entry) {
            const forms = entry[1]
            compactForm = (r.str === "1" && forms["1"]) || forms[pluralCategory(data.rules, pluralOperands(r.str, 0))] || forms.other
          }
        }
      }
      const zero = rounded !== null && rounded.d === ""
      const neg = !nan && x.neg
      let sign = ""
      if (o.signDisplay === "auto") sign = neg ? "-" : ""
      else if (o.signDisplay === "always") sign = neg ? "-" : "+"
      else if (o.signDisplay === "exceptZero") sign = nan || zero ? "" : neg ? "-" : "+"
      else if (o.signDisplay === "negative") sign = neg && !zero ? "-" : ""
      const signParts = sign ? markedParts(sign === "-" ? "minusSign" : "plusSign", sign === "-" ? data.minus : data.plus, "middle") : []
      let core = [...body, ...inner]
      let signInside = false
      if (compactForm !== undefined) {
        core = []
        signInside = o.style === "decimal" && compactForm[0] !== 0 && compactForm.includes(0)
        for (const t of compactForm) {
          if (t === 0) core.push(...(signInside ? signParts : []), ...body)
          else if (typeof t === "object") core.push({ type: "compact", value: t.c, layer: "inner" })
          else if (t !== "") core.push({ type: "literal", value: t, layer: "inner" })
        }
      }
      const choose = (patterns) => {
        if (!sign || signInside) return patterns[0]
        if (sign === "-" || patterns[1].includes(1)) return patterns[1]
        return [1, ...patterns[0]]
      }
      const fill = (tokens, symbol) => {
        const out = []
        tokens.forEach((t, i) => {
          if (t === 0) out.push(...core)
          else if (t === 1) out.push(...signParts)
          else if (t === 2) out.push({ type: "percentSign", value: data.percentSign, layer: "middle" })
          else if (t === 3) {
            const cur = { type: "currency", value: symbol, layer: "middle" }
            const space = { type: "literal", value: NBSP, layer: "middle" }
            if (tokens[i + 1] === 0 && !/[\p{S}\p{Z}]$/u.test(symbol)) out.push(cur, space)
            else if (tokens[i - 1] === 0 && !/^[\p{S}\p{Z}]/u.test(symbol)) out.push(space, cur)
            else out.push(cur)
          } else if (t !== "") out.push({ type: "literal", value: t, layer: "middle" })
        })
        return out
      }
      if (o.style === "percent") return fill(choose(data.percentPattern))
      if (o.style === "currency") {
        if (o.currencyDisplay === "name") {
          const name = CURRENCY[o.currency]?.[2] ?? o.currency
          const singular = shown === "1" && name.endsWith("s") ? name.replace(/(?<=[a-z])s$/, "") : name
          return [...fill(choose(data.decimalPattern)), { type: "literal", value: " ", layer: "outer" }, { type: "currency", value: singular, layer: "outer" }]
        }
        return fill(choose(o.currencySign === "accounting" ? data.accountingPattern : data.currencyPattern), currencySymbol(o, data))
      }
      if (o.style === "unit") {
        const [short, long] = unitNames(o.unit)
        const out = fill(choose(data.decimalPattern))
        if (o.unitDisplay === "long") {
          out.push({ type: "literal", value: " ", layer: "outer" }, { type: "unit", value: shown === "1" ? singularUnit(long) : long, layer: "outer" })
        } else if (short === "%" || short.startsWith("°") || UNIT_TIGHT[o.unitDisplay].has(lang(o.locale))) {
          out.push({ type: "unit", value: short, layer: "outer" })
        } else {
          out.push({ type: "literal", value: " ", layer: "outer" }, { type: "unit", value: short, layer: "outer" })
        }
        return out
      }
      return fill(choose(data.decimalPattern))
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
  const collationSupported = (base, co) => co === "emoji" || co === "eor" || (COLLATIONS[lang(base)] ?? []).includes(co) || new Locale(base).getCollations().includes(co)
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

  const PLURAL_ORDER = ["zero", "one", "two", "few", "many", "other"]
  const ruleCache = new Map()
  const parseRule = (src) => {
    const cached = ruleCache.get(src)
    if (cached !== undefined) return cached
    const tokens = src.match(/[a-z]+|\d+|\.\.|!=|=|%|,/g) ?? []
    let pos = 0
    const peek = () => tokens[pos]
    const next = () => tokens[pos++]
    const ors = []
    let ands = []
    while (pos < tokens.length) {
      const operand = next()
      let mod = 0
      if (peek() === "%") {
        next()
        mod = Number(next())
      }
      const neg = next() === "!="
      const ranges = []
      for (;;) {
        const a = Number(next())
        let b = a
        if (peek() === "..") {
          next()
          b = Number(next())
        }
        ranges.push([a, b])
        if (peek() !== ",") break
        next()
      }
      ands.push({ operand, mod, neg, ranges })
      const joiner = next()
      if (joiner === "and") continue
      ors.push(ands)
      ands = []
    }
    if (ands.length) ors.push(ands)
    ruleCache.set(src, ors)
    return ors
  }
  const pluralOperands = (str, exponent) => {
    let [intPart, fracPart = ""] = str.split(".")
    if (exponent > 0) {
      const shift = fracPart.padEnd(exponent, "0")
      intPart += shift.slice(0, exponent)
      fracPart = shift.slice(exponent)
    }
    const trimmed = fracPart.replace(/0+$/, "")
    const whole = BigInt(intPart || "0")
    return {
      n: whole,
      i: whole % 1000000000000000000n,
      v: fracPart.length,
      w: trimmed.length,
      f: BigInt(fracPart || "0"),
      t: BigInt(trimmed || "0"),
      frac: trimmed ? Number(`0.${trimmed}`) : 0,
      c: exponent,
      e: exponent,
    }
  }
  const operandValue = (ops, name, mod) => {
    if (name === "n") {
      const whole = mod ? Number(ops.n % BigInt(mod)) : Number(ops.n)
      return [whole + ops.frac, ops.frac === 0]
    }
    let v = ops[name]
    if (typeof v === "bigint") v = mod ? Number(v % BigInt(mod)) : Number(v)
    else if (mod) v %= mod
    return [v, true]
  }
  const ruleMatches = (rule, ops) =>
    parseRule(rule).some((ands) =>
      ands.every(({ operand, mod, neg, ranges }) => {
        const [value, integral] = operandValue(ops, operand, mod)
        const hit = integral && ranges.some(([a, b]) => value >= a && value <= b)
        return neg ? !hit : hit
      }),
    )
  const pluralCategory = (rules, ops) => {
    for (const [cat, rule] of Object.entries(rules ?? {})) if (ruleMatches(rule, ops)) return cat
    return "other"
  }
  const pluralRulesFor = (dataLocale, type) => {
    const data = sectionData("plural", dataLocale) ?? {}
    return { rules: (type === "ordinal" ? data.o : data.c) ?? {}, ranges: data.r ?? {} }
  }
  const DEFAULT_DIGITS = { minimumIntegerDigits: 1, roundingType: "fractionDigits", minFD: 0, maxFD: 3, roundingIncrement: 1, roundingMode: "halfExpand", trailingZeroDisplay: "auto" }
  const formattedDecimal = (o, value) => {
    const x = toIntlMV(value)
    const v = { kind: "finite", neg: false, d: x.d, e: x.e }
    let exponent = 0
    if (o.notation !== undefined && o.notation !== "standard" && v.d !== "") {
      exponent = computeExponent(o, v)
      v.e -= exponent
    }
    return { str: formatNumeric(o, v, false).str, exponent }
  }

  class PluralRules {
    #o
    #rules
    constructor(locales = undefined, options = undefined) {
      const requested = localeList(locales)
      const opts = coerceOptionsToObject(options)
      getOpt(opts, "localeMatcher", "string", LOCALE_MATCHERS, "best fit")
      const type = getOpt(opts, "type", "string", ["cardinal", "ordinal"], "cardinal")
      const notation = getOpt(opts, "notation", "string", ["standard", "scientific", "engineering", "compact"], "standard")
      const compactDisplay = getOpt(opts, "compactDisplay", "string", ["short", "long"], "short")
      const digits = digitOptions(opts, 0, 3, notation)
      const r = resolveService("PluralRules", requested)
      this.#rules = pluralRulesFor(r.dataLocale, type)
      this.#o = { locale: r.locale, type, notation, compactDisplay, ...digits }
    }
    static #unwrap(x, method) {
      if (!isObject(x) || !(#o in x)) throw new TypeError(`Method Intl.PluralRules.prototype.${method} called on incompatible receiver ${receiverText(x)}`)
      return x
    }
    #select(n) {
      if (!Number.isFinite(n)) return "other"
      const { str, exponent } = formattedDecimal(this.#o, n)
      return pluralCategory(this.#rules.rules, pluralOperands(str, exponent))
    }
    select(value) {
      const pr = PluralRules.#unwrap(this, "select")
      return pr.#select(+value)
    }
    selectRange(start, end) {
      const pr = PluralRules.#unwrap(this, "selectRange")
      if (start === undefined || end === undefined) throw new TypeError("start or end is undefined")
      const x = +start
      const y = +end
      if (Number.isNaN(x) || Number.isNaN(y)) throw new RangeError(`Invalid ${Number.isNaN(x) ? "start" : "end"} : NaN`)
      const a = pr.#select(x)
      const b = pr.#select(y)
      return pr.#rules.ranges[`${a}+${b}`] ?? "other"
    }
    resolvedOptions() {
      const pr = PluralRules.#unwrap(this, "resolvedOptions")
      const o = pr.#o
      const out = { locale: o.locale, type: o.type, notation: o.notation }
      if (o.notation === "compact") out.compactDisplay = o.compactDisplay
      out.minimumIntegerDigits = o.minimumIntegerDigits
      if (o.roundingType !== "significantDigits") Object.assign(out, { minimumFractionDigits: o.minFD, maximumFractionDigits: o.maxFD })
      if (o.roundingType !== "fractionDigits") Object.assign(out, { minimumSignificantDigits: o.minSD, maximumSignificantDigits: o.maxSD })
      const present = new Set([...Object.keys(pr.#rules.rules), "other"])
      out.pluralCategories = PLURAL_ORDER.filter((c) => present.has(c))
      out.roundingIncrement = o.roundingIncrement
      out.roundingMode = o.roundingMode
      out.roundingPriority = o.roundingPriority
      out.trailingZeroDisplay = o.trailingZeroDisplay
      return out
    }
    static supportedLocalesOf(locales, options = undefined) {
      return supportedLocalesOf("PluralRules", locales, options)
    }
  }

  const RT_UNIT_NAMES = { __proto__: null, second: "second", seconds: "second", minute: "minute", minutes: "minute", hour: "hour", hours: "hour", day: "day", days: "day", week: "week", weeks: "week", month: "month", months: "month", quarter: "quarter", quarters: "quarter", year: "year", years: "year" }
  class RelativeTimeFormat {
    #o
    #data
    #nf
    #plural
    constructor(locales = undefined, options = undefined) {
      const requested = localeList(locales)
      const opts = coerceOptionsToObject(options)
      getOpt(opts, "localeMatcher", "string", LOCALE_MATCHERS, "best fit")
      const nuOption = checkNumberingSystem(getOpt(opts, "numberingSystem", "string", undefined, undefined))
      const r = resolveService("RelativeTimeFormat", requested, { nu: nuOption })
      const style = getOpt(opts, "style", "string", ["long", "short", "narrow"], "long")
      const numeric = getOpt(opts, "numeric", "string", ["always", "auto"], "always")
      this.#o = { locale: r.locale, style, numeric, numberingSystem: r.nu }
      this.#data = sectionData("rtf", r.dataLocale) ?? sectionData("rtf", "en")
      this.#nf = new NumberFormat(r.locale, { __proto__: null, numberingSystem: r.nu })
      this.#plural = pluralRulesFor(r.dataLocale, "cardinal").rules
    }
    static #unwrap(x, method) {
      if (!isObject(x) || !(#o in x)) throw new TypeError(`Method Intl.RelativeTimeFormat.prototype.${method} called on incompatible receiver ${receiverText(x)}`)
      return x
    }
    #parts(value, unit) {
      const n = +value
      const name = `${unit}`
      if (!Number.isFinite(n)) throw new RangeError("Invalid value: must be a finite number")
      const u = RT_UNIT_NAMES[name]
      if (u === undefined) throw new RangeError(`Invalid unit argument for format() '${name}'`)
      const entry = this.#data[`${u}-${this.#o.style}`]
      if (this.#o.numeric === "auto") {
        const key = Object.is(n, -0) ? "-0" : String(n)
        const phrase = entry.a[key] ?? (key === "-0" ? entry.a["0"] : undefined)
        if (phrase !== undefined) return [{ type: "literal", value: phrase }]
      }
      const past = n < 0 || Object.is(n, -0)
      const abs = Math.abs(n)
      const cat = pluralCategory(this.#plural, pluralOperands(formattedDecimal(DEFAULT_DIGITS, abs).str, 0))
      const table = past ? entry.p : entry.f
      const pattern = table[cat] ?? table.other
      const at = pattern.indexOf("{0}")
      if (at === -1) return [{ type: "literal", value: pattern }]
      const out = []
      if (at > 0) out.push({ type: "literal", value: pattern.slice(0, at) })
      for (const p of this.#nf.formatToParts(abs)) out.push({ type: p.type, value: p.value, unit: u })
      if (at + 3 < pattern.length) out.push({ type: "literal", value: pattern.slice(at + 3) })
      return out
    }
    format(value, unit) {
      const rtf = RelativeTimeFormat.#unwrap(this, "format")
      return rtf.#parts(value, unit).map((p) => p.value).join("")
    }
    formatToParts(value, unit) {
      const rtf = RelativeTimeFormat.#unwrap(this, "formatToParts")
      return rtf.#parts(value, unit)
    }
    resolvedOptions() {
      const o = RelativeTimeFormat.#unwrap(this, "resolvedOptions").#o
      return { locale: o.locale, style: o.style, numeric: o.numeric, numberingSystem: o.numberingSystem }
    }
    static supportedLocalesOf(locales, options = undefined) {
      return supportedLocalesOf("RelativeTimeFormat", locales, options)
    }
  }

  const changesToE = (text) => /^(?:[iI]|[hH][iI](?![aAeE]))/.test(text)
  const changesToU = (text) => /^(?:[oO8]|[hH][oO]|11(?: |$))/.test(text)
  const contextualPattern = (locale, pattern, next) => {
    const l = lang(locale)
    if (l === "es") {
      if (pattern.includes(" y {1}") && changesToE(next)) return pattern.replace(" y {1}", " e {1}")
      if (pattern.includes(" o {1}") && changesToU(next)) return pattern.replace(" o {1}", " u {1}")
    } else if (l === "he" && pattern.includes("ו-{1}") && /^\p{Script=Hebrew}/u.test(next)) {
      return pattern.replace("ו-{1}", "ו{1}")
    }
    return pattern
  }
  const stringListFromIterable = (iterable) => {
    if (iterable === undefined) return []
    const iterator = iterable[Symbol.iterator]()
    const next = iterator.next
    const out = []
    for (;;) {
      const step = next.call(iterator)
      if (!isObject(step)) throw new TypeError(`Iterator result ${String(step)} is not an object`)
      if (step.done) return out
      const value = step.value
      if (typeof value !== "string") {
        const error = new TypeError(`Iterable yielded ${typeof value === "symbol" ? "a symbol" : String(value)} which is not a string`)
        try {
          const ret = iterator.return
          if (ret !== undefined && ret !== null) ret.call(iterator)
        } catch {}
        throw error
      }
      out.push(value)
    }
  }
  class ListFormat {
    #o
    #patterns
    constructor(locales = undefined, options = undefined) {
      const requested = localeList(locales)
      const opts = getOptionsObject(options)
      getOpt(opts, "localeMatcher", "string", LOCALE_MATCHERS, "best fit")
      const r = resolveService("ListFormat", requested)
      const type = getOpt(opts, "type", "string", ["conjunction", "disjunction", "unit"], "conjunction")
      const style = getOpt(opts, "style", "string", ["long", "short", "narrow"], "long")
      this.#o = { locale: r.locale, type, style }
      this.#patterns = (sectionData("list", r.dataLocale) ?? sectionData("list", "en"))[`${type}-${style}`]
    }
    static #unwrap(x, method) {
      if (!isObject(x) || !(#o in x)) throw new TypeError(`Method Intl.ListFormat.prototype.${method} called on incompatible receiver ${receiverText(x)}`)
      return x
    }
    #parts(items) {
      const n = items.length
      if (n === 0) return []
      const element = (value) => [{ type: "element", value }]
      if (n === 1) return element(items[0])
      const fill = (pattern, a, b) => {
        const i0 = pattern.indexOf("{0}")
        const i1 = pattern.indexOf("{1}")
        const pieces = i0 < i1 ? [[i0, a], [i1, b]] : [[i1, b], [i0, a]]
        const out = []
        let last = 0
        for (const [at, parts] of pieces) {
          if (at > last) out.push({ type: "literal", value: pattern.slice(last, at) })
          out.push(...parts)
          last = at + 3
        }
        if (last < pattern.length) out.push({ type: "literal", value: pattern.slice(last) })
        return out
      }
      const [pair, start, middle, end] = this.#patterns
      const locale = this.#o.locale
      if (n === 2) return fill(contextualPattern(locale, pair, items[1]), element(items[0]), element(items[1]))
      let parts = fill(contextualPattern(locale, end, items[n - 1]), element(items[n - 2]), element(items[n - 1]))
      for (let i = n - 3; i >= 1; i--) parts = fill(middle, element(items[i]), parts)
      return fill(start, element(items[0]), parts)
    }
    format(list) {
      const lf = ListFormat.#unwrap(this, "format")
      return lf.#parts(stringListFromIterable(list)).map((p) => p.value).join("")
    }
    formatToParts(list) {
      const lf = ListFormat.#unwrap(this, "formatToParts")
      return lf.#parts(stringListFromIterable(list))
    }
    resolvedOptions() {
      const o = ListFormat.#unwrap(this, "resolvedOptions").#o
      return { locale: o.locale, type: o.type, style: o.style }
    }
    static supportedLocalesOf(locales, options = undefined) {
      return supportedLocalesOf("ListFormat", locales, options)
    }
  }

  const SEGMENT_BREAKS = globalThis["%segmentBreaks%"]
  const segmentsState = new WeakMap()
  const segmentIteratorState = new WeakMap()
  const segmentBreaks = (st) => {
    if (st.breaks === undefined) {
      const [breaks, words] = SEGMENT_BREAKS(st.string.toWellFormed(), st.granularity)
      st.breaks = breaks
      st.words = words
    }
    return st.breaks
  }
  const segmentIndexAt = (st, n) => {
    const breaks = segmentBreaks(st)
    let lo = 0
    let hi = breaks.length - 2
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1
      if (breaks[mid] <= n) lo = mid
      else hi = mid - 1
    }
    return lo
  }
  const segmentData = (st, k) => {
    const start = st.breaks[k]
    const end = st.breaks[k + 1]
    const out = { segment: st.string.slice(start, end), index: start, input: st.string }
    if (st.granularity === "word") out.isWordLike = st.words[k]
    return out
  }
  const SegmentsPrototype = {}
  const SegmentIteratorPrototype = Object.create(Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]())))
  Object.defineProperty(SegmentsPrototype, "containing", {
    value: {
      containing(index) {
        const st = segmentsState.get(this)
        if (st === undefined) throw new TypeError("Method %Segments.prototype%.containing called on incompatible receiver")
        let n = Math.trunc(+index)
        if (Number.isNaN(n)) n = 0
        if (n < 0 || n >= st.string.length) return undefined
        return segmentData(st, segmentIndexAt(st, n))
      },
    }.containing,
    writable: true,
    configurable: true,
  })
  Object.defineProperty(SegmentsPrototype, Symbol.iterator, {
    value: {
      [Symbol.iterator]() {
        const st = segmentsState.get(this)
        if (st === undefined) throw new TypeError("Method %Segments.prototype%[@@iterator] called on incompatible receiver")
        const it = Object.create(SegmentIteratorPrototype)
        segmentIteratorState.set(it, { segments: st, position: 0 })
        return it
      },
    }[Symbol.iterator],
    writable: true,
    configurable: true,
  })
  Object.defineProperty(SegmentIteratorPrototype, "next", {
    value: {
      next() {
        const it = segmentIteratorState.get(this)
        if (it === undefined) throw new TypeError("Method %SegmentIterator.prototype%.next called on incompatible receiver")
        const st = it.segments
        if (it.position >= st.string.length) return { value: undefined, done: true }
        const k = segmentIndexAt(st, it.position)
        const value = segmentData(st, k)
        it.position = st.breaks[k + 1]
        return { value, done: false }
      },
    }.next,
    writable: true,
    configurable: true,
  })
  Object.defineProperty(SegmentIteratorPrototype, Symbol.toStringTag, { value: "Segmenter String Iterator", configurable: true })
  class Segmenter {
    #o
    constructor(locales = undefined, options = undefined) {
      const requested = localeList(locales)
      const opts = getOptionsObject(options)
      getOpt(opts, "localeMatcher", "string", LOCALE_MATCHERS, "best fit")
      const r = resolveService("Segmenter", requested)
      const granularity = getOpt(opts, "granularity", "string", ["grapheme", "word", "sentence"], "grapheme")
      this.#o = { locale: r.locale, granularity }
    }
    static #unwrap(x, method) {
      if (!isObject(x) || !(#o in x)) throw new TypeError(`Method Intl.Segmenter.prototype.${method} called on incompatible receiver ${receiverText(x)}`)
      return x
    }
    segment(string) {
      const o = Segmenter.#unwrap(this, "segment").#o
      const segments = Object.create(SegmentsPrototype)
      segmentsState.set(segments, { granularity: o.granularity, string: `${string}`, breaks: undefined, words: undefined })
      return segments
    }
    resolvedOptions() {
      const o = Segmenter.#unwrap(this, "resolvedOptions").#o
      return { locale: o.locale, granularity: o.granularity }
    }
    static supportedLocalesOf(locales, options = undefined) {
      return supportedLocalesOf("Segmenter", locales, options)
    }
  }

  const DN_TYPES = ["language", "region", "script", "currency", "calendar", "dateTimeField"]
  const DATE_TIME_FIELDS = ["era", "year", "quarter", "month", "weekOfYear", "weekday", "day", "dayPeriod", "hour", "minute", "second", "timeZoneName"]
  const CALENDAR_ALIASES = { __proto__: null, gregorian: "gregory", islamicc: "islamic-civil", "ethiopic-amete-alem": "ethioaa" }
  const LANGUAGE_ID = /^(?:[a-zA-Z]{2,3}|[a-zA-Z]{5,8})(?:-[a-zA-Z]{4})?(?:-(?:[a-zA-Z]{2}|\d{3}))?(?:-(?:[a-zA-Z0-9]{5,8}|\d[a-zA-Z0-9]{3}))*$/
  const pickStyle = (entry, style) => {
    if (entry === undefined || typeof entry === "string") return entry
    if (style === "long") return entry[0]
    if (style === "short") return entry[1] ?? entry[0]
    return entry[2] ?? entry[1] ?? entry[0]
  }
  const languageDisplayName = (names, tag, o) => {
    const parts = tag.split("-")
    const lang = parts[0]
    let i = 1
    let script
    let region
    if (parts[i]?.length === 4) script = parts[i++]
    if (parts[i] !== undefined && (parts[i].length === 2 || /^\d{3}$/.test(parts[i]))) region = parts[i++]
    const core = [lang, script, region].filter(Boolean).join("-")
    if (o.languageDisplay === "dialect" && names.dialect && Object.hasOwn(names.dialect, core)) return pickStyle(names.dialect[core], o.style)
    const base = pickStyle(names.language[lang], o.style)
    if (base === undefined) return undefined
    const qualifiers = []
    if (script) qualifiers.push(pickStyle(names.script[script], o.style) ?? script)
    if (region) qualifiers.push(pickStyle(names.region[region], o.style) ?? region)
    if (qualifiers.length === 0) return base
    const [two, three] = names.pattern
    if (qualifiers.length === 1) return two.replace("{0}", base).replace("{1}", qualifiers[0])
    return three.replace("{0}", base).replace("{1}", qualifiers[0]).replace("{2}", qualifiers[1])
  }
  class DisplayNames {
    #o
    #names
    constructor(locales, options) {
      const requested = localeList(locales)
      const opts = getOptionsObject(options)
      getOpt(opts, "localeMatcher", "string", LOCALE_MATCHERS, "best fit")
      const r = resolveService("DisplayNames", requested)
      const style = getOpt(opts, "style", "string", ["narrow", "short", "long"], "long")
      const type = getOpt(opts, "type", "string", DN_TYPES, undefined)
      if (type === undefined) throw new TypeError("Required option 'type' is missing")
      const fallback = getOpt(opts, "fallback", "string", ["code", "none"], "code")
      const languageDisplay = getOpt(opts, "languageDisplay", "string", ["dialect", "standard"], "dialect")
      this.#o = { locale: r.locale, style, type, fallback, languageDisplay }
      this.#names = sectionData("names", r.dataLocale) ?? sectionData("names", "en")
    }
    static #unwrap(x, method) {
      if (!isObject(x) || !(#o in x)) throw new TypeError(`Method Intl.DisplayNames.prototype.${method} called on incompatible receiver ${receiverText(x)}`)
      return x
    }
    of(code) {
      const dn = DisplayNames.#unwrap(this, "of")
      const o = dn.#o
      const names = dn.#names
      let c = `${code}`
      let name
      switch (o.type) {
        case "language":
          if (!LANGUAGE_ID.test(c)) throw new RangeError(`invalid_argument`)
          c = canonical(c)
          name = languageDisplayName(names, c, o)
          break
        case "region":
          if (!/^(?:[a-zA-Z]{2}|\d{3})$/.test(c)) throw new RangeError(`invalid_argument`)
          c = c.toUpperCase()
          name = pickStyle(names.region[c], o.style)
          break
        case "script":
          if (!/^[a-zA-Z]{4}$/.test(c)) throw new RangeError(`invalid_argument`)
          c = c[0].toUpperCase() + c.slice(1).toLowerCase()
          name = pickStyle(names.script[c], o.style)
          break
        case "currency":
          if (!/^[a-zA-Z]{3}$/.test(c)) throw new RangeError(`invalid_argument`)
          c = c.toUpperCase()
          name = pickStyle(names.currency[c], o.style)
          break
        case "calendar":
          if (!/^[a-zA-Z0-9]{3,8}(?:-[a-zA-Z0-9]{3,8})*$/.test(c)) throw new RangeError(`invalid_argument`)
          c = c.toLowerCase()
          c = CALENDAR_ALIASES[c] ?? c
          name = pickStyle(names.calendar[c], o.style)
          break
        default:
          if (!DATE_TIME_FIELDS.includes(c)) throw new RangeError(`invalid_argument`)
          name = pickStyle(names.dateTimeField[c], o.style)
      }
      if (name !== undefined) return name
      return o.fallback === "code" ? c : undefined
    }
    resolvedOptions() {
      const o = DisplayNames.#unwrap(this, "resolvedOptions").#o
      const out = { locale: o.locale, style: o.style, type: o.type, fallback: o.fallback }
      if (o.type === "language") out.languageDisplay = o.languageDisplay
      return out
    }
    static supportedLocalesOf(locales, options = undefined) {
      return supportedLocalesOf("DisplayNames", locales, options)
    }
  }

  const Locale = LOC.Locale

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
  Object.defineProperty(Intl, "getCanonicalLocales", {
    value: {
      getCanonicalLocales(locales) {
        const list = localeList(locales)
        const out = []
        for (let i = 0; i < list.length; i++) out[i] = list[i]
        return out
      },
    }.getCanonicalLocales,
    writable: true,
    configurable: true,
  })
  Object.defineProperty(Intl, "supportedValuesOf", {
    value: (key) => {
      const k = String(key)
      if (k === "currency") return [...(sectionData("num", "#codes") ?? Object.keys(CURRENCY))]
      if (k === "unit") return [...SANCTIONED_UNITS].sort()
      if (k === "calendar") return ["gregory"]
      if (k === "numberingSystem") return [...Object.keys(NU_ZERO), "hanidec"].sort()
      if (k === "timeZone") return tzList()
      if (k === "collation") return ["big5han", "compat", "dict", "emoji", "eor", "gb2312", "phonebk", "phonetic", "pinyin", "searchjl", "stroke", "trad", "unihan", "zhuyin"]
      throw new RangeError(`Invalid key : ${k}`)
    },
    writable: true,
    configurable: true,
  })
  Object.defineProperty(Intl, Symbol.toStringTag, { value: "Intl", configurable: true })
  return Intl
})()
