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
    "en-GB": { ...EN_DATES, order: "dmy", pad: true, h12: false, full: "{weekday} {day} {month} {year}", long: "{day} {month} {year}", medium: "{day} {short} {year}", shortDate: "{dd}/{MM}/{year}", md: "{day} {month}", mdy: "{day} {month} {year}", wsep: " " },
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
      full: "{weekday}, {day} de {month} de {year}", long: "{day} de {month} de {year}", medium: "{day} de {short} de {year}", shortDate: "{dd}/{MM}/{year}",
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
    if (tz === "UTC") return style === "long" || style === "longGeneric" ? "Coordinated Universal Time" : "UTC"
    if (style === "shortOffset") return gmt(false)
    if (style === "longOffset") return gmt(true)
    if (parseZoneOffset(tz) !== undefined) return gmt(style === "long")
    const info = tzInfo(tz, date.getTime())
    const abbreviation = info !== undefined && info[1] !== "" && !/^[+-]?\d/.test(info[1]) ? info[1] : undefined
    if ((style === "short" || style === "shortGeneric") && abbreviation !== undefined) return abbreviation
    return gmt(style === "long" || style === "longGeneric")
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
        month: monthStyle === "numeric" || monthStyle === "2-digit" ? String(f.month + 1) : monthName(monthStyle === "narrow" ? "narrow" : "long"),
        gen: monthStyle === "short" ? L.short[f.month] : monthStyle === "narrow" ? [...L.months[f.month]][0] : (L.gen ?? L.months)[f.month],
        short: L.short[f.month],
        day: dayStyle === "2-digit" ? pad2(f.day) : String(f.day),
        dd: pad2(f.day),
        d: String(f.day),
        MM: pad2(f.month + 1),
        M: String(f.month + 1),
        year: yearStyle === "2-digit" ? pad2(f.year % 100) : String(f.year),
        yy: pad2(f.year % 100),
      })
      const typeOf = { weekday: "weekday", month: "month", gen: "month", short: "month", day: "day", dd: "day", d: "day", MM: "month", M: "month", year: "year", yy: "year" }
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
          parts.push({ type: "dayPeriod", value: f.hour < 12 ? L.am : L.pm })
        }
      }
      if (o.timeZoneName) {
        lit(hasDate && !hasTime ? L.dtsep : " ")
        parts.push({ type: "timeZoneName", value: tzName() })
      }
      trimParts()
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
      out.timeZone ??= tzSystem()
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

  const callable = (C) => {
    const f = function (...args) {
      return new.target && new.target !== f ? Reflect.construct(C, args, new.target) : new C(...args)
    }
    Object.defineProperty(f, "name", { value: C.name, configurable: true })
    Object.defineProperty(f, "length", { value: 0, configurable: true })
    Object.defineProperty(f, "prototype", { value: C.prototype, writable: false, enumerable: false, configurable: false })
    Object.defineProperty(C.prototype, "constructor", { value: f, writable: true, configurable: true })
    for (const k of Object.getOwnPropertyNames(C)) if (!["length", "name", "prototype"].includes(k)) Object.defineProperty(f, k, Object.getOwnPropertyDescriptor(C, k))
    return f
  }
  const Intl = {}
  for (const [name, value] of Object.entries({ NumberFormat: callable(NumberFormat), DateTimeFormat: callable(DateTimeFormat), Collator: callable(Collator), PluralRules, RelativeTimeFormat, ListFormat, Segmenter, DisplayNames, Locale })) {
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
      if (k === "timeZone") return tzList()
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
