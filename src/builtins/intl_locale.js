(() => {
  let cache = null
  const data = () => (cache ??= JSON.parse(globalThis["%localeData%"]()))

  const ALPHA = /^[a-z]+$/i
  const ALNUM = /^[a-z0-9]+$/i
  const isLanguage = (s) => ALPHA.test(s) && (s.length === 2 || s.length === 3 || (s.length >= 5 && s.length <= 8))
  const isScript = (s) => s.length === 4 && ALPHA.test(s)
  const isRegion = (s) => (s.length === 2 && ALPHA.test(s)) || (s.length === 3 && /^[0-9]{3}$/.test(s))
  const isVariant = (s) => ALNUM.test(s) && ((s.length >= 5 && s.length <= 8) || (s.length === 4 && /^[0-9]/.test(s)))
  const isTypePart = (s) => s.length >= 3 && s.length <= 8 && ALNUM.test(s)
  const isUKey = (s) => s.length === 2 && /^[a-z0-9][a-z]$/i.test(s)
  const isTKey = (s) => s.length === 2 && /^[a-z][0-9]$/i.test(s)
  const isAttribute = (s) => s.length >= 3 && s.length <= 8 && ALNUM.test(s)
  const title = (s) => s[0].toUpperCase() + s.slice(1).toLowerCase()

  const parseLanguageId = (parts, i) => {
    if (i >= parts.length || !isLanguage(parts[i])) return null
    const id = { language: parts[i].toLowerCase(), script: undefined, region: undefined, variants: [] }
    i++
    if (i < parts.length && isScript(parts[i])) id.script = title(parts[i++])
    if (i < parts.length && isRegion(parts[i])) id.region = parts[i++].toUpperCase()
    const seen = new Set()
    while (i < parts.length && isVariant(parts[i])) {
      const v = parts[i++].toLowerCase()
      if (seen.has(v)) return null
      seen.add(v)
      id.variants.push(v)
    }
    return [id, i]
  }

  const parse = (tag) => {
    if (typeof tag !== "string" || tag.length === 0) return null
    const parts = tag.split("-")
    for (const p of parts) if (p.length === 0 || p.length > 8 || !ALNUM.test(p)) return null
    const head = parseLanguageId(parts, 0)
    if (!head) return null
    const [id, start] = head
    let i = start
    const extensions = []
    const singletons = new Set()
    let privateUse = null
    while (i < parts.length) {
      const single = parts[i].toLowerCase()
      if (single.length !== 1) return null
      if (single === "x") {
        const rest = parts.slice(i + 1)
        if (rest.length === 0) return null
        privateUse = rest.map((p) => p.toLowerCase())
        i = parts.length
        break
      }
      if (singletons.has(single)) return null
      singletons.add(single)
      i++
      if (single === "u") {
        const attributes = []
        const keywords = []
        while (i < parts.length && isAttribute(parts[i]) && !isUKey(parts[i])) attributes.push(parts[i++].toLowerCase())
        while (i < parts.length && isUKey(parts[i])) {
          const key = parts[i++].toLowerCase()
          const value = []
          while (i < parts.length && isTypePart(parts[i])) value.push(parts[i++].toLowerCase())
          keywords.push([key, value.join("-")])
        }
        if (attributes.length === 0 && keywords.length === 0) return null
        extensions.push({ type: "u", attributes, keywords })
      } else if (single === "t") {
        let tlang = null
        if (i < parts.length && isLanguage(parts[i])) {
          const r = parseLanguageId(parts, i)
          if (!r) return null
          tlang = r[0]
          i = r[1]
        }
        const fields = []
        while (i < parts.length && isTKey(parts[i])) {
          const key = parts[i++].toLowerCase()
          const value = []
          while (i < parts.length && isTypePart(parts[i])) value.push(parts[i++].toLowerCase())
          if (value.length === 0) return null
          fields.push([key, value.join("-")])
        }
        if (!tlang && fields.length === 0) return null
        extensions.push({ type: "t", tlang, fields })
      } else {
        const values = []
        while (i < parts.length && parts[i].length >= 2) values.push(parts[i++].toLowerCase())
        if (values.length === 0) return null
        extensions.push({ type: single, values })
      }
    }
    return { ...id, extensions, privateUse }
  }

  const idString = (id) => [id.language, id.script, id.region, ...id.variants].filter(Boolean).join("-")

  const U_VALUE_ALIASES = {
    ca: { "ethiopic-amete-alem": "ethioaa", islamicc: "islamic-civil" },
    co: { dictionary: "dict", phonebook: "phonebk", traditional: "trad", gb2312han: "gb2312" },
    ks: { primary: "level1", tertiary: "level3" },
    ms: { imperial: "uksystem" },
    tz: { cnckg: "cnsha", eire: "iedub", est: "papty", gmt0: "gmt", uct: "utc", zulu: "utc", aqams: "nzakl", cnhrb: "cnsha", cnkhg: "cnurc", cuba: "cuhav", egypt: "egcai", hongkong: "hkhkg", iceland: "isrey", iran: "irthr", israel: "jeruslm", jamaica: "jmkin", japan: "jptyo", libya: "lytip", mst7mdt: "usden", navajo: "usden", poland: "plwaw", portugal: "ptlis", prc: "cnsha", roc: "twtpe", rok: "krsel", turkey: "trist", universal: "utc", usnavajo: "usden", wsu: "rumow" },
  }
  const T_VALUE_ALIASES = { m0: { names: "prprname" }, d0: { name: "charname" } }
  const SUBDIVISION_ALIASES = { cn11: "cnbj", cn12: "cntj", cn13: "cnhe", cn14: "cnsx", cn15: "cnmn", cn21: "cnln", cn22: "cnjl", cn23: "cnhl", cn31: "cnsh", cn32: "cnjs", cn33: "cnzj", cn34: "cnah", cn35: "cnfj", cn36: "cnjx", cn37: "cnsd", cn41: "cnha", cn42: "cnhb", cn43: "cnhn", cn44: "cngd", cn45: "cngx", cn46: "cnhi", cn50: "cncq", cn51: "cnsc", cn52: "cngz", cn53: "cnyn", cn54: "cnxz", cn61: "cnsn", cn62: "cngs", cn63: "cnqh", cn64: "cnnx", cn65: "cnxj", cz10a: "cz110", cz10b: "cz111", cz10c: "cz112", cz10d: "cz113", cz10e: "cz114", cz10f: "cz115", cz611: "cz663", cz612: "cz632", cz613: "cz633", cz614: "cz634", cz615: "cz635", cz616: "cz636", cz617: "cz637", cz618: "cz638", cz619: "cz639", cz61a: "cz63a", cz61b: "cz63b", cz61c: "cz63c", cz621: "cz641", cz622: "cz642", cz623: "cz643", cz624: "cz644", cz626: "cz646", cz627: "cz647", cz628: "cz648", cz629: "cz649", cz62a: "cz64a", cz62b: "cz64b", cz62c: "cz64c", cz62d: "cz64d", cz62e: "cz64e", cz62f: "cz64f", fra: "frges", frb: "frnaq", frc: "frara", frd: "frbfc", fre: "frbre", frf: "frcvl", frg: "frges", frh: "frcor", fri: "frbfc", frj: "fridf", frk: "frocc", frl: "frnaq", frm: "frges", frn: "frocc", fro: "frhdf", frp: "frnor", frq: "frnor", frr: "frpdl", frs: "frhdf", frt: "frnaq", fru: "frpac", frv: "frara", lud: "lucl", lug: "luec", lul: "luca", mrnkc: "mr13", no23: "no50", nzn: "nzauk", nzs: "nzcan", omba: "ombj", omsh: "omsj", plds: "pl02", plkp: "pl04", pllb: "pl08", plld: "pl10", pllu: "pl06", plma: "pl12", plmz: "pl14", plop: "pl16", plpd: "pl20", plpk: "pl18", plpm: "pl22", plsk: "pl26", plsl: "pl24", plwn: "pl28", plwp: "pl30", plzp: "pl32", tteto: "tttob", ttrcm: "ttmrc", ttwto: "tttob", twkhq: "twkhh", twtnq: "twtnn", twtpq: "twnwt", twtxq: "twtxg" }

  const applyAliases = (id) => {
    const d = data()
    let changed = true
    let guard = 0
    while (changed && guard++ < 8) {
      changed = false
      if (id.variants.includes("hepburn") && id.variants.includes("heploc")) {
        id.variants = id.variants.filter((x) => x !== "hepburn" && x !== "heploc")
        if (!id.variants.includes("alalc97")) id.variants.push("alalc97")
        changed = true
      }
      const langRegion = id.region && d.langRegion[`${id.language}-${id.region}`]
      if (langRegion) {
        const r = parse(langRegion)
        if (r) {
          id.language = r.language
          id.region = r.region
          if (r.script && !id.script) id.script = r.script
          changed = true
        }
      }
      for (const v of [...id.variants]) {
        let rule = d.variant[`${id.language}-${v}`]
        let generic = false
        if (!rule) {
          rule = d.variant[`en-${v}`]
          generic = true
        }
        if (!rule) continue
        const r = parse(rule)
        if (!r) continue
        id.variants = id.variants.filter((x) => x !== v)
        if (!generic) id.language = r.language
        if (r.script && !id.script) id.script = r.script
        if (r.region && !id.region) id.region = r.region
        for (const nv of r.variants) if (!id.variants.includes(nv)) id.variants.push(nv)
        changed = true
        break
      }
      const langRule = d.lang[id.language]
      if (langRule) {
        const r = parse(langRule)
        if (r) {
          id.language = r.language
          if (r.script && !id.script) id.script = r.script
          if (r.region && !id.region) id.region = r.region
          for (const v of r.variants) if (!id.variants.includes(v)) id.variants.push(v)
          changed = true
        }
      }
      if (id.script && d.script[id.script]) {
        id.script = d.script[id.script]
        changed = true
      }
      if (id.region) {
        const multi = d.regionMulti[id.region]
        if (multi) id.region = pickRegion(id, multi)
        else if (d.region[id.region]) {
          id.region = d.region[id.region]
          changed = true
        }
      }
    }
    id.variants.sort()
    return id
  }

  const pickRegion = (id, options) => {
    const likely = maximizeId({ language: id.language, script: id.script, region: undefined, variants: [] })
    return likely.region && options.includes(likely.region) ? likely.region : options[0]
  }

  const canonicalUValue = (key, value) => {
    if (value === "true") return ""
    if (value === "yes" && ["kb", "kc", "kh", "kk", "kn"].includes(key)) return ""
    const table = U_VALUE_ALIASES[key]
    if (table && table[value] !== undefined) return table[value]
    if ((key === "rg" || key === "sd") && SUBDIVISION_ALIASES[value]) return SUBDIVISION_ALIASES[value]
    return value
  }

  const canonicalTValue = (key, value) => {
    const table = T_VALUE_ALIASES[key]
    if (table && table[value] !== undefined) return table[value]
    return value
  }

  const serialize = (p) => {
    let out = idString(p)
    const exts = [...p.extensions].sort((a, b) => (a.type < b.type ? -1 : a.type > b.type ? 1 : 0))
    for (const e of exts) {
      if (e.type === "u") {
        const parts = ["u", ...e.attributes]
        for (const [k, v] of e.keywords) {
          parts.push(k)
          if (v) parts.push(v)
        }
        out += "-" + parts.join("-")
      } else if (e.type === "t") {
        const parts = ["t"]
        if (e.tlang) parts.push(idString(e.tlang).toLowerCase())
        for (const [k, v] of e.fields) parts.push(k, v)
        out += "-" + parts.join("-")
      } else {
        out += "-" + [e.type, ...e.values].join("-")
      }
    }
    if (p.privateUse) out += "-x-" + p.privateUse.join("-")
    return out
  }

  const canonicalizeParsed = (p) => {
    const special = data().tag
    applyAliases(p)
    const exts = []
    for (const e of p.extensions) {
      if (e.type === "u") {
        const attributes = [...new Set(e.attributes)].sort()
        const seen = new Set()
        const keywords = []
        for (const [k, v] of e.keywords) {
          if (seen.has(k)) continue
          seen.add(k)
          keywords.push([k, canonicalUValue(k, v)])
        }
        keywords.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
        exts.push({ type: "u", attributes, keywords })
      } else if (e.type === "t") {
        const tlang = e.tlang ? applyAliases({ ...e.tlang, variants: [...e.tlang.variants] }) : null
        const fields = e.fields.map(([k, v]) => [k, canonicalTValue(k, v)]).sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
        exts.push({ type: "t", tlang, fields })
      } else {
        exts.push(e)
      }
    }
    p.extensions = exts
    const s = serialize(p)
    return special[s] ?? s
  }

  const canonicalize = (tag) => {
    const special = data().tag
    if (typeof tag === "string" && Object.prototype.hasOwnProperty.call(special, tag.toLowerCase()) && special[tag.toLowerCase()] === null) return null
    const p = parse(tag)
    if (!p) return null
    return canonicalizeParsed(p)
  }

  const maximizeId = (id, whole = true) => {
    const likely = data().likely
    const l = id.language
    if (whole && l !== "und" && id.script && id.region) return { ...id }
    const s = id.script === "Zzzz" ? undefined : id.script
    const r = id.region === "ZZ" ? undefined : id.region
    const tries = []
    if (s && r) tries.push(`${l}-${s}-${r}`)
    if (s) tries.push(`${l}-${s}`)
    if (r) tries.push(`${l}-${r}`)
    tries.push(l)
    for (const t of tries) {
      const hit = likely[t]
      if (!hit) continue
      const hp = hit.split("-")
      const hs = hp.find((x, i) => i > 0 && /^[A-Z][a-z]{3}$/.test(x))
      const hr = hp.find((x, i) => i > 0 && /^([A-Z]{2}|[0-9]{3})$/.test(x))
      return { language: hp[0], script: s ?? hs, region: r ?? hr, variants: id.variants }
    }
    return { ...id }
  }

  const sameId = (a, b) => a.language === b.language && a.script === b.script && a.region === b.region

  const minimizeId = (id) => {
    const max = maximizeId(id, false)
    const trials = [
      { language: max.language, script: undefined, region: undefined },
      { language: max.language, script: undefined, region: max.region },
      { language: max.language, script: max.script, region: undefined },
    ]
    for (const t of trials) {
      if (sameId(maximizeId({ ...t, variants: [] }, false), max)) return { ...t, variants: id.variants }
    }
    return max
  }

  const kLocale = Symbol("locale")
  let directions
  const WEEKDAYS = ["", "mon", "tue", "wed", "thu", "fri", "sat", "sun"]
  const TYPE_SEQUENCE = /^[a-z0-9]{3,8}(-[a-z0-9]{3,8})*$/i

  const rangeError = (msg) => new RangeError(msg)
  const branded = (o, name) => {
    if (o === null || typeof o !== "object" || !(kLocale in o)) throw new TypeError(`Method Intl.Locale.prototype.${name} called on incompatible receiver ${typeof o === "object" && o !== null ? "#<Object>" : String(o)}`)
    return o[kLocale]
  }

  const getStringOption = (options, key, check) => {
    const v = options[key]
    if (v === undefined) return undefined
    const s = String(v)
    if (check && !check(s)) throw rangeError(`Incorrect locale information provided`)
    return s
  }

  const keywordOf = (p, key) => {
    const u = p.extensions.find((e) => e.type === "u")
    if (!u) return undefined
    const kw = u.keywords.find(([k]) => k === key)
    if (!kw) return undefined
    return kw[1]
  }

  class Locale {
    constructor(tag, options) {
      if (new.target === undefined) throw new TypeError("Constructor Intl.Locale requires 'new'")
      if (typeof tag !== "string" && (tag === null || typeof tag !== "object")) {
        throw new TypeError(tag === undefined ? "First argument to Intl.Locale constructor can't be empty or missing" : "Incorrect locale information provided")
      }
      let text = tag !== null && typeof tag === "object" && kLocale in tag ? tag[kLocale].tag : String(tag)
      if (options === null) throw new TypeError("Cannot convert undefined or null to object")
      const opts = options === undefined ? Object.create(null) : Object(options)
      const p = parse(text)
      if (!p) throw rangeError(`Incorrect locale information provided`)
      const language = getStringOption(opts, "language", (s) => isLanguage(s))
      const script = getStringOption(opts, "script", (s) => isScript(s))
      const region = getStringOption(opts, "region", (s) => isRegion(s))
      const variantsOpt = getStringOption(opts, "variants", (s) => {
        const parts = s.split("-")
        const seen = new Set()
        for (const v of parts) {
          const low = v.toLowerCase()
          if (!isVariant(v) || seen.has(low)) return false
          seen.add(low)
        }
        return true
      })
      text = canonicalizeParsed(p)
      const q = parse(text)
      if (language !== undefined) q.language = language.toLowerCase()
      if (script !== undefined) q.script = title(script)
      if (region !== undefined) q.region = region.toUpperCase()
      if (variantsOpt !== undefined) q.variants = variantsOpt.toLowerCase().split("-")
      text = canonicalizeParsed(q)
      const relevant = [
        ["ca", getStringOption(opts, "calendar", (s) => TYPE_SEQUENCE.test(s))],
        ["co", getStringOption(opts, "collation", (s) => TYPE_SEQUENCE.test(s))],
        ["fw", (() => {
          const v = getStringOption(opts, "firstDayOfWeek")
          if (v === undefined) return undefined
          const w = /^[0-7]$/.test(v) ? WEEKDAYS[Number(v) === 0 ? 7 : Number(v)] : v
          if (!TYPE_SEQUENCE.test(w)) throw rangeError(`Incorrect locale information provided`)
          return w
        })()],
        ["hc", getStringOption(opts, "hourCycle", (s) => ["h11", "h12", "h23", "h24"].includes(s))],
        ["kf", getStringOption(opts, "caseFirst", (s) => ["upper", "lower", "false"].includes(s))],
        ["kn", (() => {
          const v = opts.numeric
          return v === undefined ? undefined : String(Boolean(v))
        })()],
        ["nu", getStringOption(opts, "numberingSystem", (s) => TYPE_SEQUENCE.test(s))],
      ]
      const r = parse(text)
      let u = r.extensions.find((e) => e.type === "u")
      for (const [key, value] of relevant) {
        if (value === undefined) continue
        if (!u) {
          u = { type: "u", attributes: [], keywords: [] }
          r.extensions.push(u)
        }
        const v = value.toLowerCase()
        const existing = u.keywords.find(([k]) => k === key)
        if (existing) existing[1] = v
        else u.keywords.push([key, v])
      }
      text = canonicalizeParsed(r)
      const final = parse(text)
      Object.defineProperty(this, kLocale, { value: { tag: text, parsed: final } })
    }
    get baseName() {
      return idString(branded(this, "baseName").parsed)
    }
    get language() {
      return branded(this, "language").parsed.language
    }
    get script() {
      return branded(this, "script").parsed.script
    }
    get region() {
      return branded(this, "region").parsed.region
    }
    get variants() {
      const v = branded(this, "variants").parsed.variants
      return v.length ? v.join("-") : undefined
    }
    get calendar() {
      return keywordOf(branded(this, "calendar").parsed, "ca")
    }
    get collation() {
      return keywordOf(branded(this, "collation").parsed, "co")
    }
    get firstDayOfWeek() {
      return keywordOf(branded(this, "firstDayOfWeek").parsed, "fw")
    }
    get hourCycle() {
      return keywordOf(branded(this, "hourCycle").parsed, "hc")
    }
    get caseFirst() {
      return keywordOf(branded(this, "caseFirst").parsed, "kf")
    }
    get numeric() {
      const v = keywordOf(branded(this, "numeric").parsed, "kn")
      return v === "" || v === "true"
    }
    get numberingSystem() {
      return keywordOf(branded(this, "numberingSystem").parsed, "nu")
    }
    toString() {
      return branded(this, "toString").tag
    }
    toJSON() {
      return branded(this, "toJSON").tag
    }
    maximize() {
      const { parsed } = branded(this, "maximize")
      const max = maximizeId(parsed)
      return new Locale(serialize({ ...parsed, ...max, variants: parsed.variants }))
    }
    minimize() {
      const { parsed } = branded(this, "minimize")
      const min = minimizeId(parsed)
      return new Locale(serialize({ ...parsed, ...min, variants: parsed.variants }))
    }
    getCalendars() {
      const { parsed } = branded(this, "getCalendars")
      const ca = keywordOf(parsed, "ca")
      if (ca !== undefined) return [ca]
      return [...(regionInfo(parsed)?.c ?? ["gregory"])]
    }
    getCollations() {
      const { parsed } = branded(this, "getCollations")
      const co = keywordOf(parsed, "co")
      if (co !== undefined) return [co]
      return [...resourceValue(parsed, "co", ["emoji", "eor"])]
    }
    getHourCycles() {
      const { parsed } = branded(this, "getHourCycles")
      const hc = keywordOf(parsed, "hc")
      if (hc !== undefined) return [hc]
      let language = parsed.language === "und" ? "" : parsed.language
      let country = keywordRegion(parsed, "rg") ?? parsed.region ?? keywordRegion(parsed, "sd") ?? ""
      if (!language || !country) {
        const max = maximizeId(parsed)
        language = max.language === "und" ? "" : max.language
        country = max.region ?? ""
      }
      const table = data().info.hc
      const key = `${language || "und"}_${country || "001"}`
      return [table[key] ?? table[country || "001"] ?? "h23"]
    }
    getNumberingSystems() {
      const { parsed } = branded(this, "getNumberingSystems")
      const nu = keywordOf(parsed, "nu")
      if (nu !== undefined) return [nu]
      return [...resourceValue(parsed, "nu", ["latn"])]
    }
    getTimeZones() {
      const { parsed } = branded(this, "getTimeZones")
      if (!parsed.region) return undefined
      return [...(data().info.region[parsed.region]?.tz ?? [])]
    }
    getTextInfo() {
      const { parsed } = branded(this, "getTextInfo")
      const script = parsed.script ?? maximizeId(parsed).script
      if (!directions) {
        directions = new Map()
        for (const s of data().info.rtl) directions.set(s, "rtl")
        for (const s of data().info.ltr) directions.set(s, "ltr")
      }
      return { direction: script ? directions.get(script) : undefined }
    }
    getWeekInfo() {
      const { parsed } = branded(this, "getWeekInfo")
      const base = regionInfo(parsed)?.w ?? { firstDay: 1, weekend: [6, 7] }
      const out = { firstDay: base.firstDay, weekend: [...base.weekend] }
      const day = WEEKDAYS.indexOf(keywordOf(parsed, "fw"))
      if (day > 0) out.firstDay = day
      return out
    }
  }
  const keywordRegion = (parsed, key) => {
    const v = keywordOf(parsed, key)
    if (!v || v.length < 3 || v.length > 7) return undefined
    return /^[a-z]/.test(v) ? v.slice(0, 2).toUpperCase() : v.slice(0, 3)
  }
  const regionFor = (parsed) => keywordRegion(parsed, "rg") ?? parsed.region ?? keywordRegion(parsed, "sd") ?? maximizeId(parsed).region
  const regionInfo = (parsed) => data().info.region[regionFor(parsed) ?? "001"]
  const resourceValue = (parsed, field, fallback) => {
    const res = data().info.res
    const l = parsed.language
    const s = parsed.script
    const r = parsed.region
    const hit = data().likely[l]
    const preferred = hit ? hit.split("-").find((x, i) => i > 0 && /^[A-Z][a-z]{3}$/.test(x)) : undefined
    const chain = s && s !== preferred ? [r && `${l}-${s}-${r}`, `${l}-${s}`] : [r && `${l}-${r}`, l]
    for (const key of chain) {
      const v = key && res[key]?.[field]
      if (v) return v
    }
    return fallback
  }
  for (const [legacy, method] of [["calendars", "getCalendars"], ["collations", "getCollations"], ["hourCycles", "getHourCycles"], ["numberingSystems", "getNumberingSystems"], ["timeZones", "getTimeZones"], ["textInfo", "getTextInfo"], ["weekInfo", "getWeekInfo"]]) {
    Object.defineProperty(Locale.prototype, legacy, {
      get() {
        return Locale.prototype[method].call(this)
      },
      configurable: true,
    })
  }
  delete Locale.prototype.toJSON
  Object.defineProperty(Locale, "length", { value: 1, configurable: true })
  Object.defineProperty(Locale.prototype, Symbol.toStringTag, { value: "Intl.Locale", configurable: true })

  const isLocaleObject = (v) => v !== null && typeof v === "object" && kLocale in v
  return { parse, canonicalize, Locale, isLocaleObject, localeTag: (v) => v[kLocale].tag, maximizeTag: (tag) => {
    const p = parse(tag)
    if (!p) return tag
    return serialize({ ...p, ...maximizeId(p), variants: p.variants })
  } }
})()
