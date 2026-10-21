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
    return parts
      .map((p, i) => (i === 0 ? p.toLowerCase() : p.length === 2 ? p.toUpperCase() : p.length === 4 ? p[0].toUpperCase() + p.slice(1).toLowerCase() : p.toLowerCase()))
      .join("-")
  }
  const localeList = (locales) => {
    if (locales === undefined) return []
    const list = typeof locales === "string" ? [locales] : [...locales]
    const out = []
    for (const l of list) {
      const c = canonical(typeof l === "object" && l !== null && "baseName" in l ? l.toString() : l)
      if (!out.includes(c)) out.push(c)
    }
    return out
  }
  const resolve = (locales) => localeList(locales)[0] ?? "en-US"
  const lang = (locale) => locale.split("-")[0]
  const numberData = (locale) => NUMBER_DATA[lang(locale)] ?? NUMBER_DATA.en

  const getOpt = (options, key, allowed, fallback) => {
    const v = options[key]
    if (v === undefined) return fallback
    const s = typeof allowed === "boolean" ? Boolean(v) : String(v)
    if (Array.isArray(allowed) && !allowed.includes(s)) {
      throw new RangeError(`Value ${s} out of range for Intl.NumberFormat options property ${key}`)
    }
    return s
  }
  const getNum = (options, key, min, max, fallback) => {
    const v = options[key]
    if (v === undefined) return fallback
    const n = Number(v)
    if (!Number.isFinite(n) || n < min || n > max) throw new RangeError(`${key} value is out of range.`)
    return Math.floor(n)
  }

  const roundTo = (x, digits) => {
    const s = x.toFixed(Math.min(digits, 100))
    return s
  }

  const groupDigits = (int, sep, minGroup) => {
    if (int.length < 4 + (minGroup - 1)) return int
    let out = ""
    for (let i = 0; i < int.length; i++) {
      if (i > 0 && (int.length - i) % 3 === 0) out += sep
      out += int[i]
    }
    return out
  }

  const COMPACT = [
    [1e12, "T", " trillion"],
    [1e9, "B", " billion"],
    [1e6, "M", " million"],
    [1e3, "K", " thousand"],
  ]

  class NumberFormat {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      const locale = resolve(locales)
      const style = getOpt(opts, "style", ["decimal", "percent", "currency", "unit"], "decimal")
      const currency = opts.currency === undefined ? undefined : String(opts.currency).toUpperCase()
      if (style === "currency" && currency === undefined) throw new TypeError("Currency code is required with currency style.")
      const unit = opts.unit === undefined ? undefined : String(opts.unit)
      if (style === "unit" && unit === undefined) throw new TypeError("Unit is required with unit style.")
      const notation = getOpt(opts, "notation", ["standard", "scientific", "engineering", "compact"], "standard")
      const cdigits = style === "currency" ? (CURRENCY[currency]?.[1] ?? 2) : 0
      const minFD = getNum(opts, "minimumFractionDigits", 0, 100, undefined)
      const maxFD = getNum(opts, "maximumFractionDigits", 0, 100, undefined)
      const defMin = style === "currency" && notation !== "compact" ? cdigits : 0
      const defMax = style === "currency" && notation !== "compact" ? cdigits : style === "percent" ? 0 : notation === "compact" ? 0 : 3
      let minimumFractionDigits = minFD ?? defMin
      let maximumFractionDigits = maxFD ?? Math.max(defMax, minimumFractionDigits)
      if (minFD !== undefined && maxFD === undefined) maximumFractionDigits = Math.max(defMax, minFD)
      if (maxFD !== undefined && minFD === undefined) minimumFractionDigits = Math.min(defMin, maxFD)
      if (minimumFractionDigits > maximumFractionDigits) throw new RangeError("maximumFractionDigits value is out of range.")
      this.#o = {
        locale,
        numberingSystem: "latn",
        style,
        currency,
        currencyDisplay: getOpt(opts, "currencyDisplay", ["symbol", "narrowSymbol", "code", "name"], "symbol"),
        currencySign: getOpt(opts, "currencySign", ["standard", "accounting"], "standard"),
        unit,
        unitDisplay: getOpt(opts, "unitDisplay", ["short", "narrow", "long"], "short"),
        minimumIntegerDigits: getNum(opts, "minimumIntegerDigits", 1, 21, 1),
        minimumFractionDigits,
        maximumFractionDigits,
        minimumSignificantDigits: getNum(opts, "minimumSignificantDigits", 1, 21, undefined),
        maximumSignificantDigits: getNum(opts, "maximumSignificantDigits", 1, 21, undefined),
        useGrouping: opts.useGrouping === undefined ? (notation === "compact" ? "min2" : "auto") : opts.useGrouping === false ? false : String(opts.useGrouping) === "true" ? "always" : String(opts.useGrouping),
        notation,
        compactDisplay: getOpt(opts, "compactDisplay", ["short", "long"], "short"),
        signDisplay: getOpt(opts, "signDisplay", ["auto", "never", "always", "exceptZero", "negative"], "auto"),
        roundingMode: "halfExpand",
      }
      if (this.#o.maximumSignificantDigits !== undefined && this.#o.minimumSignificantDigits === undefined) this.#o.minimumSignificantDigits = 1
      if (this.#o.minimumSignificantDigits !== undefined && this.#o.maximumSignificantDigits === undefined) this.#o.maximumSignificantDigits = 21
    }
    #digits(x) {
      const o = this.#o
      if (o.maximumSignificantDigits !== undefined) {
        if (x === 0) return "0"
        let s = Number(x.toPrecision(o.maximumSignificantDigits)).toString()
        if (s.includes("e")) s = Number(s).toLocaleString === undefined ? s : BigInt(Math.round(Number(s))).toString()
        const [i, f = ""] = s.split(".")
        let frac = f
        const sig = (i.replace(/^0+/, "") + frac).length
        if (sig < o.minimumSignificantDigits) frac = frac.padEnd(frac.length + (o.minimumSignificantDigits - sig), "0")
        return frac ? `${i}.${frac}` : i
      }
      let s = roundTo(x, o.maximumFractionDigits)
      if (s.includes("e")) s = BigInt(Math.round(x)).toString()
      let [i, f = ""] = s.split(".")
      while (f.length > o.minimumFractionDigits && f.endsWith("0")) f = f.slice(0, -1)
      return f ? `${i}.${f}` : i
    }
    #parts(value) {
      const o = this.#o
      const data = numberData(o.locale)
      let x = typeof value === "bigint" ? Number(value) : Number(value)
      const parts = []
      const negative = x < 0 || Object.is(x, -0)
      if (Number.isNaN(x)) return [{ type: "nan", value: "NaN" }]
      let abs = Math.abs(x)
      if (o.style === "percent") abs *= 100
      let suffix = ""
      let exponent = null
      if (o.notation === "compact" && Number.isFinite(abs)) {
        for (const [limit, short, long] of COMPACT) {
          if (abs >= limit) {
            abs /= limit
            suffix = o.compactDisplay === "long" ? long : short
            break
          }
        }
        if (o.maximumSignificantDigits === undefined && o.maximumFractionDigits === 0 && abs < 100) {
          abs = Number(abs.toPrecision(2))
          const s = abs.toString()
          if (s.includes(".")) {
            const [i, f] = s.split(".")
            const shown = this.#withGroup(i, data)
            const res = [{ type: "integer", value: shown }, { type: "decimal", value: data.decimal }, { type: "fraction", value: f }]
            return this.#decorate(res, suffix, negative, x, data, exponent)
          }
        }
      } else if ((o.notation === "scientific" || o.notation === "engineering") && abs !== 0 && Number.isFinite(abs)) {
        let e = Math.floor(Math.log10(abs))
        if (o.notation === "engineering") e = Math.floor(e / 3) * 3
        abs = abs / 10 ** e
        if (abs >= 10 && o.notation === "scientific") {
          abs /= 10
          e += 1
        }
        exponent = e
      }
      if (!Number.isFinite(abs)) {
        return this.#decorate([{ type: "infinity", value: "∞" }], suffix, negative, x, data, exponent)
      }
      const text = exponent !== null && o.maximumSignificantDigits === undefined && o.maximumFractionDigits === 3 ? Number(abs.toPrecision(4)).toString() : this.#digits(abs)
      let [int, frac = ""] = text.split(".")
      int = int.padStart(o.minimumIntegerDigits, "0")
      const grouped = this.#withGroup(int, data)
      grouped.split(data.group).forEach((chunk, idx) => {
        if (idx > 0) parts.push({ type: "group", value: data.group })
        parts.push({ type: "integer", value: chunk })
      })
      if (frac) {
        parts.push({ type: "decimal", value: data.decimal })
        parts.push({ type: "fraction", value: frac })
      }
      return this.#decorate(parts, suffix, negative, x, data, exponent)
    }
    #withGroup(int, data) {
      const g = this.#o.useGrouping
      if (g === false) return int
      return groupDigits(int, data.group, g === "min2" ? 2 : g === "always" ? 1 : data.minGroup)
    }
    #decorate(parts, suffix, negative, x, data, exponent) {
      const o = this.#o
      const out = []
      const isZero = parts.every((p) => p.type !== "integer" || /^0*$/.test(p.value)) && parts.every((p) => p.type !== "fraction" || /^0*$/.test(p.value))
      let sign = ""
      if (o.signDisplay === "always") sign = negative ? "-" : "+"
      else if (o.signDisplay === "exceptZero") sign = isZero ? "" : negative ? "-" : "+"
      else if (o.signDisplay === "never") sign = ""
      else if (o.signDisplay === "negative") sign = negative && !isZero ? "-" : ""
      else sign = negative ? "-" : ""
      if (sign) out.push({ type: sign === "-" ? "minusSign" : "plusSign", value: sign })
      const body = [...parts]
      if (exponent !== null) {
        body.push({ type: "exponentSeparator", value: "E" })
        if (exponent < 0) body.push({ type: "exponentMinusSign", value: "-" })
        body.push({ type: "exponentInteger", value: String(Math.abs(exponent)) })
      }
      if (suffix) {
        if (suffix.startsWith(" ")) body.push({ type: "literal", value: " " }, { type: "compact", value: suffix.trim() })
        else body.push({ type: "compact", value: suffix })
      }
      if (o.style === "percent") {
        const [before, after] = data.percent.split("{n}")
        if (before) out.push({ type: "percentSign", value: before })
        out.push(...body)
        if (after) {
          const lit = after.slice(0, -1)
          if (lit) out.push({ type: "literal", value: lit })
          out.push({ type: "percentSign", value: "%" })
        }
        return out
      }
      if (o.style === "currency") {
        const info = CURRENCY[o.currency] ?? [o.currency, 2, o.currency]
        if (o.currencyDisplay === "name") {
          out.push(...body, { type: "literal", value: " " }, { type: "currency", value: info[2] })
          return out
        }
        const symbol = o.currencyDisplay === "code" ? o.currency : o.currencyDisplay === "narrowSymbol" ? info[0].replace(/^[A-Z]+(?=[^A-Z])/, "") : info[0]
        const [before, after] = data.currency.split("{n}")
        if (before) {
          const sym = before.replace("{s}", symbol)
          out.push({ type: "currency", value: sym.trim() })
          if (sym !== sym.trim() || (o.currencyDisplay === "code" && lang(o.locale) === "en")) out.push({ type: "literal", value: NBSP })
        }
        out.push(...body)
        if (after) {
          out.push({ type: "literal", value: after.slice(0, after.indexOf("{s}")) })
          out.push({ type: "currency", value: symbol })
        }
        return out
      }
      if (o.style === "unit") {
        const info = UNITS[o.unit] ?? [o.unit, o.unit]
        out.push(...body)
        if (o.unitDisplay === "long") {
          const n = Math.abs(x)
          let name = info[1]
          if (n === 1 && name.endsWith("s")) name = name.replace(/ies$/, "y").replace(/s$/, "").replace(/^feet$/, "foot")
          out.push({ type: "literal", value: " " }, { type: "unit", value: name })
        } else if (o.unitDisplay === "narrow") {
          out.push({ type: "unit", value: info[0] })
        } else if (info[0] === "%" || info[0].startsWith("°")) {
          out.push({ type: "unit", value: info[0] })
        } else {
          out.push({ type: "literal", value: " " }, { type: "unit", value: info[0] })
        }
        return out
      }
      out.push(...body)
      return out
    }
    get format() {
      return (value) => this.#parts(value).map((p) => p.value).join("")
    }
    formatToParts(value) {
      return this.#parts(value)
    }
    formatRange(a, b) {
      return `${this.format(a)}–${this.format(b)}`
    }
    resolvedOptions() {
      const out = {}
      for (const [k, v] of Object.entries(this.#o)) if (v !== undefined) out[k] = v
      return out
    }
    static supportedLocalesOf(locales) {
      return localeList(locales)
    }
  }

  const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
  const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]

  const zoneOffset = (tz, date) => {
    if (tz === undefined) return -date.getTimezoneOffset()
    const z = tz.toUpperCase()
    if (z === "UTC" || z === "ETC/UTC" || z === "GMT" || z === "ETC/GMT" || z === "Z") return 0
    const m = /^(?:UTC|GMT)?([+-])(\d{1,2})(?::?(\d{2}))?$/.exec(tz)
    if (m) return (m[1] === "-" ? -1 : 1) * (Number(m[2]) * 60 + Number(m[3] ?? 0))
    return -date.getTimezoneOffset()
  }

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
        numberingSystem: "latn",
        timeZone: opts.timeZone === undefined ? undefined : String(opts.timeZone),
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
        hour12: opts.hour12 === undefined ? (opts.hourCycle === undefined ? undefined : opts.hourCycle === "h12" || opts.hourCycle === "h11") : Boolean(opts.hour12),
        dateStyle: pick("dateStyle", ["full", "long", "medium", "short"]),
        timeStyle: pick("timeStyle", ["full", "long", "medium", "short"]),
      }
      const anyField = ["weekday", "year", "month", "day", "hour", "minute", "second", "fractionalSecondDigits"].some((k) => o[k] !== undefined)
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
      const offset = zoneOffset(o.timeZone, d)
      const f = fields(d, offset)
      const parts = []
      const lit = (v) => parts.push({ type: "literal", value: v })
      const h12 = o.hour12 !== false
      const hourText = (style) => {
        let h = f.hour
        if (h12) h = h % 12 === 0 ? 12 : h % 12
        return style === "2-digit" ? pad2(h) : String(h)
      }
      const tzName = () => {
        if (offset === 0) return o.timeZoneName === "short" || o.timeZoneName === undefined ? "UTC" : "Coordinated Universal Time"
        const sign = offset < 0 ? "-" : "+"
        const a = Math.abs(offset)
        return `GMT${sign}${Math.floor(a / 60)}${a % 60 ? ":" + pad2(a % 60) : ""}`
      }
      const datePart = (style) => {
        if (style === "full") {
          parts.push({ type: "weekday", value: DAYS[f.weekday] })
          lit(", ")
        }
        if (style === "full" || style === "long") {
          parts.push({ type: "month", value: MONTHS[f.month] })
          lit(" ")
          parts.push({ type: "day", value: String(f.day) })
          lit(", ")
          parts.push({ type: "year", value: String(f.year) })
        } else if (style === "medium") {
          parts.push({ type: "month", value: MONTHS[f.month].slice(0, 3) })
          lit(" ")
          parts.push({ type: "day", value: String(f.day) })
          lit(", ")
          parts.push({ type: "year", value: String(f.year) })
        } else {
          parts.push({ type: "month", value: String(f.month + 1) })
          lit("/")
          parts.push({ type: "day", value: String(f.day) })
          lit("/")
          parts.push({ type: "year", value: String(f.year % 100).padStart(2, "0") })
        }
      }
      const timePart = (style) => {
        parts.push({ type: "hour", value: hourText("numeric") })
        lit(":")
        parts.push({ type: "minute", value: pad2(f.minute) })
        if (style !== "short") {
          lit(":")
          parts.push({ type: "second", value: pad2(f.second) })
        }
        if (h12) {
          lit(" ")
          parts.push({ type: "dayPeriod", value: f.hour < 12 ? "AM" : "PM" })
        }
        if (style === "full" || style === "long") {
          lit(" ")
          parts.push({ type: "timeZoneName", value: style === "full" && offset === 0 ? "Coordinated Universal Time" : tzName() })
        }
      }
      if (o.dateStyle || o.timeStyle) {
        if (o.dateStyle) datePart(o.dateStyle)
        if (o.dateStyle && o.timeStyle) lit(o.dateStyle === "full" || o.dateStyle === "long" ? " at " : ", ")
        if (o.timeStyle) timePart(o.timeStyle)
        return parts
      }
      const hasDate = o.year || o.month || o.day || o.weekday
      const textMonth = o.month === "long" || o.month === "short" || o.month === "narrow"
      if (o.weekday) {
        parts.push({ type: "weekday", value: o.weekday === "long" ? DAYS[f.weekday] : o.weekday === "short" ? DAYS[f.weekday].slice(0, 3) : DAYS[f.weekday][0] })
        if (o.year || o.month || o.day) lit(", ")
      }
      if (textMonth) {
        const m = o.month === "long" ? MONTHS[f.month] : o.month === "short" ? MONTHS[f.month].slice(0, 3) : MONTHS[f.month][0]
        parts.push({ type: "month", value: m })
        if (o.day) {
          lit(" ")
          parts.push({ type: "day", value: o.day === "2-digit" ? pad2(f.day) : String(f.day) })
        }
        if (o.year) {
          lit(o.day ? ", " : " ")
          parts.push({ type: "year", value: o.year === "2-digit" ? pad2(f.year % 100) : String(f.year) })
        }
      } else {
        const segs = []
        if (o.month) segs.push({ type: "month", value: o.month === "2-digit" ? pad2(f.month + 1) : String(f.month + 1) })
        if (o.day) segs.push({ type: "day", value: o.day === "2-digit" ? pad2(f.day) : String(f.day) })
        if (o.year) segs.push({ type: "year", value: o.year === "2-digit" ? pad2(f.year % 100) : String(f.year) })
        segs.forEach((s, i) => {
          if (i > 0) lit("/")
          parts.push(s)
        })
      }
      const hasTime = o.hour || o.minute || o.second
      if (hasTime) {
        if (hasDate) lit(", ")
        const items = []
        if (o.hour) items.push({ type: "hour", value: hourText(o.hour === "2-digit" || (!h12 && o.minute) ? "2-digit" : "numeric") })
        if (o.minute) items.push({ type: "minute", value: o.hour || o.second ? pad2(f.minute) : String(f.minute) })
        if (o.second) items.push({ type: "second", value: o.minute ? pad2(f.second) : String(f.second) })
        items.forEach((s, i) => {
          if (i > 0) lit(":")
          parts.push(s)
        })
        if (o.fractionalSecondDigits) {
          lit(".")
          parts.push({ type: "fractionalSecond", value: String(f.ms).padStart(3, "0").slice(0, o.fractionalSecondDigits) })
        }
        if (o.hour && h12) {
          lit(" ")
          parts.push({ type: "dayPeriod", value: f.hour < 12 ? "AM" : "PM" })
        }
      }
      if (o.timeZoneName) {
        lit(hasDate && !hasTime ? ", " : " ")
        parts.push({ type: "timeZoneName", value: tzName() })
      }
      return parts
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
      const out = {}
      for (const [k, v] of Object.entries(this.#o)) if (v !== undefined && k !== "defaulted") out[k] = v
      out.timeZone ??= process.env.TZ ?? "UTC"
      return out
    }
    static supportedLocalesOf(locales) {
      return localeList(locales)
    }
  }

  const fold = (s) => s.normalize("NFD").replace(/[̀-ͯ]/g, "")
  class Collator {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      this.#o = {
        locale: resolve(locales),
        usage: opts.usage === undefined ? "sort" : String(opts.usage),
        sensitivity: opts.sensitivity === undefined ? "variant" : String(opts.sensitivity),
        ignorePunctuation: Boolean(opts.ignorePunctuation),
        collation: "default",
        numeric: Boolean(opts.numeric),
        caseFirst: opts.caseFirst === undefined ? "false" : String(opts.caseFirst),
      }
    }
    get compare() {
      return (a, b) => {
        const o = this.#o
        let x = String(a)
        let y = String(b)
        if (o.ignorePunctuation) {
          x = x.replace(/[\p{P}\s]/gu, "")
          y = y.replace(/[\p{P}\s]/gu, "")
        }
        const base = (s) => fold(s).toLowerCase()
        const cmpStr = (p, q) => {
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
        const r1 = cmpStr(base(x), base(y))
        if (r1 !== 0 || o.sensitivity === "base") return r1
        const r2 = cmpStr(x.toLowerCase(), y.toLowerCase())
        if (r2 !== 0) {
          if (o.sensitivity === "case") return 0
          return r2
        }
        if (o.sensitivity === "accent") return 0
        if (x === y) return 0
        for (let i = 0; i < Math.min(x.length, y.length); i++) {
          if (x[i] !== y[i]) {
            const xl = x[i] === x[i].toLowerCase()
            if (o.caseFirst === "upper") return xl ? 1 : -1
            return xl ? -1 : 1
          }
        }
        return x.length - y.length < 0 ? -1 : 1
      }
    }
    resolvedOptions() {
      return { ...this.#o }
    }
    static supportedLocalesOf(locales) {
      return localeList(locales)
    }
  }

  const pluralCardinal = (l, n, i, v) => {
    switch (l) {
      case "fr":
      case "pt":
        return i === 0 || i === 1 ? "one" : "other"
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
      return localeList(locales)
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
  class RelativeTimeFormat {
    #o
    constructor(locales, options) {
      const opts = options === undefined ? {} : Object(options)
      this.#o = {
        locale: resolve(locales),
        style: opts.style === undefined ? "long" : String(opts.style),
        numeric: opts.numeric === undefined ? "always" : String(opts.numeric),
        numberingSystem: "latn",
      }
    }
    format(value, unit) {
      const n = Number(value)
      let u = String(unit)
      if (u.endsWith("s")) u = u.slice(0, -1)
      if (!RT_UNITS.includes(u)) throw new RangeError(`Invalid unit argument for format() '${unit}'`)
      if (this.#o.numeric === "auto") {
        const t = RT_AUTO[u]?.[String(n)]
        if (t) return t
      }
      const abs = Math.abs(n)
      const num = new NumberFormat(this.#o.locale).format(abs)
      const label = this.#o.style === "long" ? (abs === 1 ? u : u + "s") : RT_SHORT[u] === "day" ? (abs === 1 ? "day" : "days") : RT_SHORT[u]
      return n < 0 || Object.is(n, -0) ? `${num} ${label} ago` : `in ${num} ${label}`
    }
    formatToParts(value, unit) {
      return [{ type: "literal", value: this.format(value, unit) }]
    }
    resolvedOptions() {
      return { ...this.#o }
    }
    static supportedLocalesOf(locales) {
      return localeList(locales)
    }
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
      const word = o.type === "disjunction" ? "or" : o.type === "unit" ? "" : o.style === "long" ? "and" : "&"
      if (items.length === 0) return ""
      if (items.length === 1) return items[0]
      if (o.type === "unit") return items.join(o.style === "narrow" ? " " : ", ")
      if (items.length === 2) return `${items[0]} ${word} ${items[1]}`
      return `${items.slice(0, -1).join(", ")}, ${word} ${items[items.length - 1]}`
    }
    formatToParts(list) {
      return [{ type: "literal", value: this.format(list) }]
    }
    resolvedOptions() {
      return { ...this.#o }
    }
    static supportedLocalesOf(locales) {
      return localeList(locales)
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
      return localeList(locales)
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
      return localeList(locales)
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

  const Intl = {}
  for (const [name, value] of Object.entries({ NumberFormat, DateTimeFormat, Collator, PluralRules, RelativeTimeFormat, ListFormat, Segmenter, DisplayNames, Locale })) {
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
      if (k === "numberingSystem") return ["latn"]
      if (k === "timeZone") return ["UTC"]
      if (k === "collation") return ["default"]
      throw new RangeError(`Invalid key : ${k}`)
    },
    writable: true,
    configurable: true,
  })
  Object.defineProperty(Intl, Symbol.toStringTag, { value: "Intl", configurable: true })
  return Intl
})()
"##;

pub fn load(vm: &mut Vm) -> Result<Value, Value> {
    if let Some(v) = vm.globals.get("Intl").cloned() {
        return Ok(v);
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
