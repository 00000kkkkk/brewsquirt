const GC_VALUES: &[&str] = &[
    "C",
    "Other",
    "Cc",
    "Control",
    "cntrl",
    "Cf",
    "Format",
    "Cn",
    "Unassigned",
    "Co",
    "Private_Use",
    "Cs",
    "Surrogate",
    "L",
    "Letter",
    "LC",
    "Cased_Letter",
    "Ll",
    "Lowercase_Letter",
    "Lm",
    "Modifier_Letter",
    "Lo",
    "Other_Letter",
    "Lt",
    "Titlecase_Letter",
    "Lu",
    "Uppercase_Letter",
    "M",
    "Mark",
    "Combining_Mark",
    "Mc",
    "Spacing_Mark",
    "Me",
    "Enclosing_Mark",
    "Mn",
    "Nonspacing_Mark",
    "N",
    "Number",
    "Nd",
    "Decimal_Number",
    "digit",
    "Nl",
    "Letter_Number",
    "No",
    "Other_Number",
    "P",
    "Punctuation",
    "punct",
    "Pc",
    "Connector_Punctuation",
    "Pd",
    "Dash_Punctuation",
    "Pe",
    "Close_Punctuation",
    "Pf",
    "Final_Punctuation",
    "Pi",
    "Initial_Punctuation",
    "Po",
    "Other_Punctuation",
    "Ps",
    "Open_Punctuation",
    "S",
    "Symbol",
    "Sc",
    "Currency_Symbol",
    "Sk",
    "Modifier_Symbol",
    "Sm",
    "Math_Symbol",
    "So",
    "Other_Symbol",
    "Z",
    "Separator",
    "Zl",
    "Line_Separator",
    "Zp",
    "Paragraph_Separator",
    "Zs",
    "Space_Separator",
];

const BINARY_PROPS: &[&str] = &[
    "ASCII",
    "ASCII_Hex_Digit",
    "AHex",
    "Alphabetic",
    "Alpha",
    "Any",
    "Assigned",
    "Bidi_Control",
    "Bidi_C",
    "Bidi_Mirrored",
    "Bidi_M",
    "Case_Ignorable",
    "CI",
    "Cased",
    "Changes_When_Casefolded",
    "CWCF",
    "Changes_When_Casemapped",
    "CWCM",
    "Changes_When_Lowercased",
    "CWL",
    "Changes_When_NFKC_Casefolded",
    "CWKCF",
    "Changes_When_Titlecased",
    "CWT",
    "Changes_When_Uppercased",
    "CWU",
    "Dash",
    "Default_Ignorable_Code_Point",
    "DI",
    "Deprecated",
    "Dep",
    "Diacritic",
    "Dia",
    "Emoji",
    "Emoji_Component",
    "EComp",
    "Emoji_Modifier",
    "EMod",
    "Emoji_Modifier_Base",
    "EBase",
    "Emoji_Presentation",
    "EPres",
    "Extended_Pictographic",
    "ExtPict",
    "Extender",
    "Ext",
    "Grapheme_Base",
    "Gr_Base",
    "Grapheme_Extend",
    "Gr_Ext",
    "Hex_Digit",
    "Hex",
    "IDS_Binary_Operator",
    "IDSB",
    "IDS_Trinary_Operator",
    "IDST",
    "ID_Continue",
    "IDC",
    "ID_Start",
    "IDS",
    "Ideographic",
    "Ideo",
    "Join_Control",
    "Join_C",
    "Logical_Order_Exception",
    "LOE",
    "Lowercase",
    "Lower",
    "Math",
    "Noncharacter_Code_Point",
    "NChar",
    "Pattern_Syntax",
    "Pat_Syn",
    "Pattern_White_Space",
    "Pat_WS",
    "Quotation_Mark",
    "QMark",
    "Radical",
    "Regional_Indicator",
    "RI",
    "Sentence_Terminal",
    "STerm",
    "Soft_Dotted",
    "SD",
    "Terminal_Punctuation",
    "Term",
    "Unified_Ideograph",
    "UIdeo",
    "Uppercase",
    "Upper",
    "Variation_Selector",
    "VS",
    "White_Space",
    "space",
    "XID_Continue",
    "XIDC",
    "XID_Start",
    "XIDS",
];

const STRING_PROPS: &[&str] = &[
    "Basic_Emoji",
    "Emoji_Keycap_Sequence",
    "RGI_Emoji",
    "RGI_Emoji_Flag_Sequence",
    "RGI_Emoji_Modifier_Sequence",
    "RGI_Emoji_Tag_Sequence",
    "RGI_Emoji_ZWJ_Sequence",
];

const SCRIPTS: &[&str] = &[
    "Adlam",
    "Adlm",
    "Ahom",
    "Anatolian_Hieroglyphs",
    "Hluw",
    "Arabic",
    "Arab",
    "Armenian",
    "Armn",
    "Avestan",
    "Avst",
    "Balinese",
    "Bali",
    "Bamum",
    "Bamu",
    "Bassa_Vah",
    "Bass",
    "Batak",
    "Batk",
    "Bengali",
    "Beng",
    "Bhaiksuki",
    "Bhks",
    "Bopomofo",
    "Bopo",
    "Brahmi",
    "Brah",
    "Braille",
    "Brai",
    "Buginese",
    "Bugi",
    "Buhid",
    "Buhd",
    "Canadian_Aboriginal",
    "Cans",
    "Carian",
    "Cari",
    "Caucasian_Albanian",
    "Aghb",
    "Chakma",
    "Cakm",
    "Cham",
    "Cherokee",
    "Cher",
    "Chorasmian",
    "Chrs",
    "Common",
    "Zyyy",
    "Coptic",
    "Copt",
    "Qaac",
    "Cuneiform",
    "Xsux",
    "Cypriot",
    "Cprt",
    "Cypro_Minoan",
    "Cpmn",
    "Cyrillic",
    "Cyrl",
    "Deseret",
    "Dsrt",
    "Devanagari",
    "Deva",
    "Dives_Akuru",
    "Diak",
    "Dogra",
    "Dogr",
    "Duployan",
    "Dupl",
    "Egyptian_Hieroglyphs",
    "Egyp",
    "Elbasan",
    "Elba",
    "Elymaic",
    "Elym",
    "Ethiopic",
    "Ethi",
    "Garay",
    "Gara",
    "Georgian",
    "Geor",
    "Glagolitic",
    "Glag",
    "Gothic",
    "Goth",
    "Grantha",
    "Gran",
    "Greek",
    "Grek",
    "Gujarati",
    "Gujr",
    "Gunjala_Gondi",
    "Gong",
    "Gurmukhi",
    "Guru",
    "Gurung_Khema",
    "Gukh",
    "Han",
    "Hani",
    "Hangul",
    "Hang",
    "Hanifi_Rohingya",
    "Rohg",
    "Hanunoo",
    "Hano",
    "Hatran",
    "Hatr",
    "Hebrew",
    "Hebr",
    "Hiragana",
    "Hira",
    "Imperial_Aramaic",
    "Armi",
    "Inherited",
    "Zinh",
    "Qaai",
    "Inscriptional_Pahlavi",
    "Phli",
    "Inscriptional_Parthian",
    "Prti",
    "Javanese",
    "Java",
    "Kaithi",
    "Kthi",
    "Kannada",
    "Knda",
    "Katakana",
    "Kana",
    "Kawi",
    "Kayah_Li",
    "Kali",
    "Kharoshthi",
    "Khar",
    "Khitan_Small_Script",
    "Kits",
    "Khmer",
    "Khmr",
    "Khojki",
    "Khoj",
    "Khudawadi",
    "Sind",
    "Kirat_Rai",
    "Krai",
    "Lao",
    "Laoo",
    "Latin",
    "Latn",
    "Lepcha",
    "Lepc",
    "Limbu",
    "Limb",
    "Linear_A",
    "Lina",
    "Linear_B",
    "Linb",
    "Lisu",
    "Lycian",
    "Lyci",
    "Lydian",
    "Lydi",
    "Mahajani",
    "Mahj",
    "Makasar",
    "Maka",
    "Malayalam",
    "Mlym",
    "Mandaic",
    "Mand",
    "Manichaean",
    "Mani",
    "Marchen",
    "Marc",
    "Masaram_Gondi",
    "Gonm",
    "Medefaidrin",
    "Medf",
    "Meetei_Mayek",
    "Mtei",
    "Mende_Kikakui",
    "Mend",
    "Meroitic_Cursive",
    "Merc",
    "Meroitic_Hieroglyphs",
    "Mero",
    "Miao",
    "Plrd",
    "Modi",
    "Mongolian",
    "Mong",
    "Mro",
    "Mroo",
    "Multani",
    "Mult",
    "Myanmar",
    "Mymr",
    "Nabataean",
    "Nbat",
    "Nag_Mundari",
    "Nagm",
    "Nandinagari",
    "Nand",
    "New_Tai_Lue",
    "Talu",
    "Newa",
    "Nko",
    "Nkoo",
    "Nushu",
    "Nshu",
    "Nyiakeng_Puachue_Hmong",
    "Hmnp",
    "Ogham",
    "Ogam",
    "Ol_Chiki",
    "Olck",
    "Ol_Onal",
    "Onao",
    "Old_Hungarian",
    "Hung",
    "Old_Italic",
    "Ital",
    "Old_North_Arabian",
    "Narb",
    "Old_Permic",
    "Perm",
    "Old_Persian",
    "Xpeo",
    "Old_Sogdian",
    "Sogo",
    "Old_South_Arabian",
    "Sarb",
    "Old_Turkic",
    "Orkh",
    "Old_Uyghur",
    "Ougr",
    "Oriya",
    "Orya",
    "Osage",
    "Osge",
    "Osmanya",
    "Osma",
    "Pahawh_Hmong",
    "Hmng",
    "Palmyrene",
    "Palm",
    "Pau_Cin_Hau",
    "Pauc",
    "Phags_Pa",
    "Phag",
    "Phoenician",
    "Phnx",
    "Psalter_Pahlavi",
    "Phlp",
    "Rejang",
    "Rjng",
    "Runic",
    "Runr",
    "Samaritan",
    "Samr",
    "Saurashtra",
    "Saur",
    "Sharada",
    "Shrd",
    "Shavian",
    "Shaw",
    "Siddham",
    "Sidd",
    "SignWriting",
    "Sgnw",
    "Sinhala",
    "Sinh",
    "Sogdian",
    "Sogd",
    "Sora_Sompeng",
    "Sora",
    "Soyombo",
    "Soyo",
    "Sundanese",
    "Sund",
    "Sunuwar",
    "Sunu",
    "Syloti_Nagri",
    "Sylo",
    "Syriac",
    "Syrc",
    "Tagalog",
    "Tglg",
    "Tagbanwa",
    "Tagb",
    "Tai_Le",
    "Tale",
    "Tai_Tham",
    "Lana",
    "Tai_Viet",
    "Tavt",
    "Takri",
    "Takr",
    "Tamil",
    "Taml",
    "Tangsa",
    "Tnsa",
    "Tangut",
    "Tang",
    "Telugu",
    "Telu",
    "Thaana",
    "Thaa",
    "Thai",
    "Tibetan",
    "Tibt",
    "Tifinagh",
    "Tfng",
    "Tirhuta",
    "Tirh",
    "Todhri",
    "Todr",
    "Toto",
    "Tulu_Tigalari",
    "Tutg",
    "Ugaritic",
    "Ugar",
    "Vai",
    "Vaii",
    "Vithkuqi",
    "Vith",
    "Wancho",
    "Wcho",
    "Warang_Citi",
    "Wara",
    "Yezidi",
    "Yezi",
    "Yi",
    "Yiii",
    "Zanabazar_Square",
    "Zanb",
];

const SYNTAX_CHARS: &str = "^$\\.*+?()[]{}|";

pub fn validate(source: &str, flags: &str) -> Result<(), String> {
    let mut seen = Vec::new();
    for f in flags.chars() {
        if !"dgimsuyv".contains(f) || seen.contains(&f) {
            return Err(format!(
                "Invalid flags supplied to RegExp constructor '{flags}'"
            ));
        }
        seen.push(f);
    }
    if flags.contains('u') && flags.contains('v') {
        return Err(format!(
            "Invalid flags supplied to RegExp constructor '{flags}'"
        ));
    }
    let chars: Vec<char> = source.chars().collect();
    let v = flags.contains('v');
    let (groups, has_names) = prescan(&chars, v);
    let mut p = Parser {
        chars,
        pos: 0,
        unicode: flags.contains('u') || v,
        v,
        groups,
        has_names,
        names: Vec::new(),
        backrefs: Vec::new(),
        path: Vec::new(),
        disjunctions: 0,
    };
    p.disjunction()?;
    if p.pos < p.chars.len() {
        return Err(if p.chars[p.pos] == ')' {
            "Unmatched ')'".to_string()
        } else {
            "Unexpected character".to_string()
        });
    }
    for name in &p.backrefs {
        if !p.names.iter().any(|(n, _)| n == name) {
            return Err("Invalid named capture referenced".to_string());
        }
    }
    Ok(())
}

pub fn prescan(chars: &[char], v: bool) -> (usize, bool) {
    let mut groups = 0;
    let mut named = false;
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                i += 2;
                continue;
            }
            '[' => {
                if depth == 0 || v {
                    depth += 1;
                }
            }
            ']' => depth = depth.saturating_sub(1),
            '(' if depth == 0 => {
                if chars.get(i + 1) == Some(&'?') {
                    if chars.get(i + 2) == Some(&'<')
                        && !matches!(chars.get(i + 3), Some('=') | Some('!'))
                    {
                        groups += 1;
                        named = true;
                    }
                } else {
                    groups += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    (groups, named)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    unicode: bool,
    v: bool,
    groups: usize,
    has_names: bool,
    names: Vec<(String, Vec<(usize, usize)>)>,
    backrefs: Vec<String>,
    path: Vec<(usize, usize)>,
    disjunctions: usize,
}

type R<T> = Result<T, String>;

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: char) -> R<()> {
        if self.eat(c) {
            Ok(())
        } else if c == ')' {
            Err("Unterminated group".to_string())
        } else {
            Err(format!("Expected '{c}'"))
        }
    }

    fn disjunction(&mut self) -> R<()> {
        let id = self.disjunctions;
        self.disjunctions += 1;
        let mut alt = 0;
        loop {
            self.path.push((id, alt));
            let r = self.alternative();
            self.path.pop();
            r?;
            if self.eat('|') {
                alt += 1;
                continue;
            }
            return Ok(());
        }
    }

    fn alternative(&mut self) -> R<()> {
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                return Ok(());
            }
            self.term()?;
        }
        Ok(())
    }

    fn term(&mut self) -> R<()> {
        let c = self.peek().ok_or("Unexpected end")?;
        match c {
            '^' | '$' => {
                self.pos += 1;
                return self.no_quantifier();
            }
            '\\' if matches!(self.at(1), Some('b') | Some('B')) => {
                self.pos += 2;
                return self.no_quantifier();
            }
            '(' if self.at(1) == Some('?') => match (self.at(2), self.at(3)) {
                (Some('='), _) | (Some('!'), _) => {
                    self.pos += 3;
                    self.disjunction()?;
                    self.expect(')')?;
                    if self.unicode {
                        return self.no_quantifier();
                    }
                    return self.quantifier_opt();
                }
                (Some('<'), Some('=')) | (Some('<'), Some('!')) => {
                    self.pos += 4;
                    self.disjunction()?;
                    self.expect(')')?;
                    return self.no_quantifier();
                }
                (Some(':'), _) => {
                    self.pos += 3;
                    self.disjunction()?;
                    self.expect(')')?;
                }
                (Some('<'), _) => {
                    self.pos += 2;
                    let name = self.group_name()?;
                    self.define_name(name)?;
                    self.disjunction()?;
                    self.expect(')')?;
                }
                _ => {
                    self.pos += 2;
                    self.modifiers()?;
                    self.disjunction()?;
                    self.expect(')')?;
                }
            },
            '(' => {
                self.pos += 1;
                self.disjunction()?;
                self.expect(')')?;
            }
            ')' => return Err("Unmatched ')'".to_string()),
            '*' | '+' | '?' => return Err("Nothing to repeat".to_string()),
            '{' => {
                if self.unicode {
                    return Err("Lone quantifier brackets".to_string());
                }
                if self.braced_quantifier(self.pos).is_some() {
                    return Err("Nothing to repeat".to_string());
                }
                self.pos += 1;
            }
            '}' => {
                if self.unicode {
                    return Err("Lone quantifier brackets".to_string());
                }
                self.pos += 1;
            }
            ']' => {
                if self.unicode {
                    return Err("Lone ']'".to_string());
                }
                self.pos += 1;
            }
            '[' => {
                self.pos += 1;
                if self.v {
                    self.class_set()?;
                } else {
                    self.class()?;
                }
            }
            '\\' => self.atom_escape()?,
            _ => self.pos += 1,
        }
        self.quantifier_opt()
    }

    fn modifiers(&mut self) -> R<()> {
        let mut add = String::new();
        while let Some(c) = self.peek() {
            if "ims".contains(c) {
                if add.contains(c) {
                    return Err("Repeated flag in modifiers".to_string());
                }
                add.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        let mut remove = String::new();
        if self.eat('-') {
            while let Some(c) = self.peek() {
                if "ims".contains(c) {
                    if add.contains(c) || remove.contains(c) {
                        return Err("Repeated flag in modifiers".to_string());
                    }
                    remove.push(c);
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if add.is_empty() && remove.is_empty() {
                return Err("Invalid group".to_string());
            }
        }
        if !self.eat(':') {
            return Err("Invalid group".to_string());
        }
        Ok(())
    }

    fn braced_quantifier(&self, i: usize) -> Option<(usize, u128, Option<u128>)> {
        let mut j = i + 1;
        let start = j;
        let mut lo: u128 = 0;
        while j < self.chars.len() && self.chars[j].is_ascii_digit() {
            lo = lo
                .saturating_mul(10)
                .saturating_add(self.chars[j] as u128 - 48);
            j += 1;
        }
        if j == start {
            return None;
        }
        let mut hi = Some(lo);
        if j < self.chars.len() && self.chars[j] == ',' {
            j += 1;
            let hs = j;
            let mut h: u128 = 0;
            while j < self.chars.len() && self.chars[j].is_ascii_digit() {
                h = h
                    .saturating_mul(10)
                    .saturating_add(self.chars[j] as u128 - 48);
                j += 1;
            }
            hi = if j == hs { None } else { Some(h) };
        }
        if j < self.chars.len() && self.chars[j] == '}' {
            Some((j, lo, hi))
        } else {
            None
        }
    }

    fn quantifier_opt(&mut self) -> R<()> {
        match self.peek() {
            Some('*') | Some('+') | Some('?') => self.pos += 1,
            Some('{') => match self.braced_quantifier(self.pos) {
                Some((_, lo, Some(hi))) if lo > hi => {
                    return Err("numbers out of order in {} quantifier".to_string());
                }
                Some((end, _, _)) => self.pos = end + 1,
                None => return Ok(()),
            },
            _ => return Ok(()),
        }
        self.eat('?');
        Ok(())
    }

    fn no_quantifier(&mut self) -> R<()> {
        match self.peek() {
            Some('*') | Some('+') | Some('?') => Err("Nothing to repeat".to_string()),
            Some('{') if self.braced_quantifier(self.pos).is_some() => {
                Err("Nothing to repeat".to_string())
            }
            _ => Ok(()),
        }
    }

    fn define_name(&mut self, name: String) -> R<()> {
        for (n, path) in &self.names {
            if *n != name {
                continue;
            }
            let mut separate = false;
            for (a, b) in self.path.iter().zip(path.iter()) {
                if a.0 != b.0 {
                    break;
                }
                if a.1 != b.1 {
                    separate = true;
                    break;
                }
            }
            if !separate {
                return Err("Duplicate capture group name".to_string());
            }
        }
        self.names.push((name, self.path.clone()));
        Ok(())
    }

    fn group_name(&mut self) -> R<String> {
        if !self.eat('<') {
            return Err("Invalid capture group name".to_string());
        }
        let mut name = String::new();
        loop {
            let c = self.peek().ok_or("Invalid capture group name")?;
            if c == '>' {
                self.pos += 1;
                break;
            }
            let cp = if c == '\\' {
                self.pos += 1;
                if !self.eat('u') {
                    return Err("Invalid capture group name".to_string());
                }
                self.unicode_escape_value(true)?
                    .ok_or("Invalid capture group name")?
            } else {
                self.pos += 1;
                c as u32
            };
            let ch = char::from_u32(cp).ok_or("Invalid capture group name")?;
            let ok = if name.is_empty() {
                ch == '$' || ch == '_' || unicode_ident::is_xid_start(ch)
            } else {
                ch == '$'
                    || ch == '_'
                    || ch == '\u{200c}'
                    || ch == '\u{200d}'
                    || unicode_ident::is_xid_continue(ch)
            };
            if !ok {
                return Err("Invalid capture group name".to_string());
            }
            name.push(ch);
        }
        if name.is_empty() {
            return Err("Invalid capture group name".to_string());
        }
        Ok(name)
    }

    fn hex(&self, start: usize, n: usize) -> Option<u32> {
        if start + n > self.chars.len() {
            return None;
        }
        let mut v = 0u32;
        for c in &self.chars[start..start + n] {
            v = v * 16 + c.to_digit(16)?;
        }
        Some(v)
    }

    fn unicode_escape_value(&mut self, strict: bool) -> R<Option<u32>> {
        if self.peek() == Some('{') && (self.unicode || strict) {
            let mut j = self.pos + 1;
            let mut v: u32 = 0;
            let mut digits = 0;
            while j < self.chars.len() && self.chars[j] != '}' {
                let d = self.chars[j].to_digit(16).ok_or("Invalid Unicode escape")?;
                v = v.saturating_mul(16).saturating_add(d);
                if v > 0x10FFFF {
                    return Err("Invalid Unicode escape".to_string());
                }
                digits += 1;
                j += 1;
            }
            if j >= self.chars.len() || digits == 0 {
                return Err("Invalid Unicode escape".to_string());
            }
            self.pos = j + 1;
            return Ok(Some(v));
        }
        let Some(v) = self.hex(self.pos, 4) else {
            if self.unicode || strict {
                return Err("Invalid Unicode escape".to_string());
            }
            return Ok(None);
        };
        self.pos += 4;
        if (0xD800..0xDC00).contains(&v) && self.peek() == Some('\\') && self.at(1) == Some('u') {
            if let Some(low) = self.hex(self.pos + 2, 4) {
                if (0xDC00..0xE000).contains(&low) {
                    self.pos += 6;
                    return Ok(Some(0x10000 + ((v - 0xD800) << 10) + (low - 0xDC00)));
                }
            }
        }
        if strict && (0xD800..0xE000).contains(&v) && self.unicode {
            return Ok(Some(v));
        }
        Ok(Some(v))
    }

    fn legacy_octal(&mut self) -> u32 {
        let mut v = 0u32;
        let mut n = 0;
        while let Some(c) = self.peek() {
            if !('0'..='7').contains(&c) || n == 3 {
                break;
            }
            let next = v * 8 + (c as u32 - 48);
            if next > 255 {
                break;
            }
            v = next;
            self.pos += 1;
            n += 1;
        }
        v
    }

    fn atom_escape(&mut self) -> R<()> {
        self.pos += 1;
        let c = self.peek().ok_or("\\ at end of pattern")?;
        match c {
            '1'..='9' => {
                let start = self.pos;
                let mut n: usize = 0;
                while let Some(d) = self.peek().filter(char::is_ascii_digit) {
                    n = n.saturating_mul(10).saturating_add(d as usize - 48);
                    self.pos += 1;
                }
                if n > self.groups {
                    if self.unicode || self.has_names {
                        return Err("Invalid escape".to_string());
                    }
                    self.pos = start;
                    if matches!(self.peek(), Some('8') | Some('9')) {
                        self.pos += 1;
                    } else {
                        self.legacy_octal();
                    }
                }
            }
            '0' => {
                self.pos += 1;
                if self.peek().is_some_and(|d| d.is_ascii_digit()) {
                    if self.unicode {
                        return Err("Invalid decimal escape".to_string());
                    }
                    self.pos -= 1;
                    self.legacy_octal();
                }
            }
            'd' | 'D' | 's' | 'S' | 'w' | 'W' => self.pos += 1,
            'p' | 'P' => {
                self.pos += 1;
                if self.unicode {
                    let strings = self.property()?;
                    if strings && (c == 'P' || !self.v) {
                        return Err("Invalid property name".to_string());
                    }
                }
            }
            'k' => {
                self.pos += 1;
                if self.unicode || self.has_names {
                    let name = self.group_name()?;
                    self.backrefs.push(name);
                }
            }
            'b' | 'B' | 'f' | 'n' | 'r' | 't' | 'v' => self.pos += 1,
            'c' => match self.at(1) {
                Some(l) if l.is_ascii_alphabetic() => self.pos += 2,
                _ => {
                    if self.unicode {
                        return Err("Invalid unicode escape".to_string());
                    }
                }
            },
            'x' => {
                self.pos += 1;
                if self.hex(self.pos, 2).is_some() {
                    self.pos += 2;
                } else if self.unicode {
                    return Err("Invalid escape".to_string());
                }
            }
            'u' => {
                self.pos += 1;
                self.unicode_escape_value(false)?;
            }
            '-' => {
                if self.unicode {
                    return Err("Invalid escape".to_string());
                }
                self.pos += 1;
            }
            c if SYNTAX_CHARS.contains(c) || c == '/' => self.pos += 1,
            _ => {
                if self.unicode {
                    return Err("Invalid escape".to_string());
                }
                self.pos += 1;
            }
        }
        Ok(())
    }

    fn property(&mut self) -> R<bool> {
        if !self.eat('{') {
            return Err("Invalid property name".to_string());
        }
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '}' {
                break;
            }
            self.pos += 1;
        }
        if self.peek() != Some('}') {
            return Err("Invalid property name".to_string());
        }
        let body: String = self.chars[start..self.pos].iter().collect();
        self.pos += 1;
        if let Some((name, value)) = body.split_once('=') {
            let ok = match name {
                "General_Category" | "gc" => GC_VALUES.contains(&value),
                "Script" | "sc" | "Script_Extensions" | "scx" => SCRIPTS.contains(&value),
                _ => false,
            };
            return if ok {
                Ok(false)
            } else {
                Err("Invalid property name".to_string())
            };
        }
        if GC_VALUES.contains(&body.as_str()) || BINARY_PROPS.contains(&body.as_str()) {
            return Ok(false);
        }
        if self.v && STRING_PROPS.contains(&body.as_str()) {
            return Ok(true);
        }
        Err("Invalid property name".to_string())
    }

    fn class(&mut self) -> R<()> {
        self.eat('^');
        loop {
            let c = self.peek().ok_or("Unterminated character class")?;
            if c == ']' {
                self.pos += 1;
                return Ok(());
            }
            let a = self.class_atom()?;
            if self.peek() == Some('-') && self.at(1).is_some_and(|n| n != ']') {
                self.pos += 1;
                let b = self.class_atom()?;
                match (a, b) {
                    (Some(x), Some(y)) => {
                        if x > y {
                            return Err("Range out of order in character class".to_string());
                        }
                    }
                    _ => {
                        if self.unicode {
                            return Err("Invalid character class".to_string());
                        }
                    }
                }
            }
        }
    }

    fn class_atom(&mut self) -> R<Option<u32>> {
        let c = self.peek().ok_or("Unterminated character class")?;
        if c != '\\' {
            self.pos += 1;
            return Ok(Some(c as u32));
        }
        self.pos += 1;
        let e = self.peek().ok_or("\\ at end of pattern")?;
        match e {
            'd' | 'D' | 's' | 'S' | 'w' | 'W' => {
                self.pos += 1;
                Ok(None)
            }
            'p' | 'P' if self.unicode => {
                self.pos += 1;
                if self.property()? {
                    return Err("Invalid property name in character class".to_string());
                }
                Ok(None)
            }
            'b' => {
                self.pos += 1;
                Ok(Some(8))
            }
            '-' if self.unicode => {
                self.pos += 1;
                Ok(Some('-' as u32))
            }
            'c' => match self.at(1) {
                Some(l) if l.is_ascii_alphabetic() => {
                    self.pos += 2;
                    Ok(Some(l as u32 % 32))
                }
                Some(l) if !self.unicode && (l.is_ascii_digit() || l == '_') => {
                    self.pos += 2;
                    Ok(Some(l as u32 % 32))
                }
                _ => {
                    if self.unicode {
                        return Err("Invalid class escape".to_string());
                    }
                    Ok(Some('\\' as u32))
                }
            },
            '0' => {
                self.pos += 1;
                if self.peek().is_some_and(|d| d.is_ascii_digit()) {
                    if self.unicode {
                        return Err("Invalid class escape".to_string());
                    }
                    self.pos -= 1;
                    return Ok(Some(self.legacy_octal()));
                }
                Ok(Some(0))
            }
            '1'..='9' => {
                if self.unicode {
                    return Err("Invalid class escape".to_string());
                }
                if matches!(e, '8' | '9') {
                    self.pos += 1;
                    return Ok(Some(e as u32));
                }
                Ok(Some(self.legacy_octal()))
            }
            'x' => {
                self.pos += 1;
                if let Some(v) = self.hex(self.pos, 2) {
                    self.pos += 2;
                    Ok(Some(v))
                } else if self.unicode {
                    Err("Invalid escape".to_string())
                } else {
                    Ok(Some('x' as u32))
                }
            }
            'u' => {
                self.pos += 1;
                Ok(Some(
                    self.unicode_escape_value(false)?.unwrap_or('u' as u32),
                ))
            }
            'f' => {
                self.pos += 1;
                Ok(Some(12))
            }
            'n' => {
                self.pos += 1;
                Ok(Some(10))
            }
            'r' => {
                self.pos += 1;
                Ok(Some(13))
            }
            't' => {
                self.pos += 1;
                Ok(Some(9))
            }
            'v' => {
                self.pos += 1;
                Ok(Some(11))
            }
            'k' if self.unicode => Err("Invalid class escape".to_string()),
            c if SYNTAX_CHARS.contains(c) || c == '/' => {
                self.pos += 1;
                Ok(Some(c as u32))
            }
            c => {
                if self.unicode {
                    return Err("Invalid escape".to_string());
                }
                self.pos += 1;
                Ok(Some(c as u32))
            }
        }
    }

    fn class_set(&mut self) -> R<bool> {
        let negated = self.eat('^');
        let mut strings = false;
        let mut op: Option<char> = None;
        let mut operands = 0;
        loop {
            match self.peek() {
                None => return Err("Unterminated character class".to_string()),
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                _ => {}
            }
            let pair = (self.peek(), self.at(1));
            if pair == (Some('&'), Some('&')) || pair == (Some('-'), Some('-')) {
                let this = pair.0.unwrap_or('&');
                if operands == 0 || op.is_some_and(|o| o != this) {
                    return Err("Invalid set operation in character class".to_string());
                }
                if operands > 1 && op.is_none() {
                    return Err("Invalid set operation in character class".to_string());
                }
                op = Some(this);
                self.pos += 2;
                if self.peek() == Some(this) || self.peek() == Some(']') {
                    return Err("Invalid set operation in character class".to_string());
                }
                continue;
            }
            if op.is_some() && operands % 2 == 0 && operands > 0 {
                return Err("Invalid set operation in character class".to_string());
            }
            let s = self.class_set_operand(op.is_none())?;
            strings |= s;
            operands += 1;
            if op.is_some() {
                operands = 1;
            }
        }
        if negated && strings {
            return Err("Negated character class may contain strings".to_string());
        }
        Ok(strings)
    }

    fn class_set_operand(&mut self, allow_range: bool) -> R<bool> {
        match (self.peek(), self.at(1)) {
            (Some('['), _) => {
                self.pos += 1;
                self.class_set()
            }
            (Some('\\'), Some('q')) => {
                self.pos += 2;
                if !self.eat('{') {
                    return Err("Invalid escape".to_string());
                }
                let mut strings = false;
                loop {
                    let mut len = 0;
                    while let Some(c) = self.peek() {
                        if c == '|' || c == '}' {
                            break;
                        }
                        self.class_set_char()?;
                        len += 1;
                    }
                    if len != 1 {
                        strings = true;
                    }
                    if self.eat('|') {
                        continue;
                    }
                    if !self.eat('}') {
                        return Err("Unterminated class string disjunction".to_string());
                    }
                    break;
                }
                Ok(strings)
            }
            (Some('\\'), Some('p')) | (Some('\\'), Some('P')) => {
                let negated = self.at(1) == Some('P');
                self.pos += 2;
                let strings = self.property()?;
                if strings && negated {
                    return Err("Invalid property name in character class".to_string());
                }
                Ok(strings)
            }
            (Some('\\'), Some('d' | 'D' | 's' | 'S' | 'w' | 'W')) => {
                self.pos += 2;
                Ok(false)
            }
            _ => {
                let lo = self.class_set_char()?;
                if allow_range && self.peek() == Some('-') && self.at(1) != Some('-') {
                    self.pos += 1;
                    let hi = self.class_set_char()?;
                    if lo > hi {
                        return Err("Range out of order in character class".to_string());
                    }
                }
                Ok(false)
            }
        }
    }

    fn class_set_char(&mut self) -> R<u32> {
        let c = self.peek().ok_or("Unterminated character class")?;
        if c == '\\' {
            let e = self.at(1).ok_or("\\ at end of pattern")?;
            if "&-!#%,:;<=>@`~".contains(e) {
                self.pos += 2;
                return Ok(e as u32);
            }
            if e == 'b' {
                self.pos += 2;
                return Ok(8);
            }
            self.pos += 1;
            return self.character_escape_value();
        }
        if "()[]{}/-\\|".contains(c) {
            return Err("Invalid character in character class".to_string());
        }
        if self.at(1) == Some(c) && "&!#$%*+,.:;<=>?@^`~".contains(c) {
            return Err("Invalid set operation in character class".to_string());
        }
        self.pos += 1;
        Ok(c as u32)
    }

    fn character_escape_value(&mut self) -> R<u32> {
        let e = self.peek().ok_or("\\ at end of pattern")?;
        match e {
            'f' => {
                self.pos += 1;
                Ok(12)
            }
            'n' => {
                self.pos += 1;
                Ok(10)
            }
            'r' => {
                self.pos += 1;
                Ok(13)
            }
            't' => {
                self.pos += 1;
                Ok(9)
            }
            'v' => {
                self.pos += 1;
                Ok(11)
            }
            'c' => match self.at(1) {
                Some(l) if l.is_ascii_alphabetic() => {
                    self.pos += 2;
                    Ok(l as u32 % 32)
                }
                _ => Err("Invalid unicode escape".to_string()),
            },
            '0' => {
                self.pos += 1;
                if self.peek().is_some_and(|d| d.is_ascii_digit()) {
                    return Err("Invalid decimal escape".to_string());
                }
                Ok(0)
            }
            'x' => {
                self.pos += 1;
                let v = self.hex(self.pos, 2).ok_or("Invalid escape")?;
                self.pos += 2;
                Ok(v)
            }
            'u' => {
                self.pos += 1;
                self.unicode_escape_value(true)?
                    .ok_or_else(|| "Invalid Unicode escape".to_string())
            }
            c if SYNTAX_CHARS.contains(c) || c == '/' => {
                self.pos += 1;
                Ok(c as u32)
            }
            _ => Err("Invalid escape".to_string()),
        }
    }
}
