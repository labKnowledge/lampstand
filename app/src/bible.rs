use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::Read;

pub const BOOKS: [(&str, &[&str]); 66] = [
    ("Genesis", &["gen", "ge", "gn"]),
    ("Exodus", &["exo", "ex", "exod"]),
    ("Leviticus", &["lev", "le", "lv"]),
    ("Numbers", &["num", "nu", "nm", "nb"]),
    ("Deuteronomy", &["deut", "deu", "dt"]),
    ("Joshua", &["josh", "jos", "jsh"]),
    ("Judges", &["judg", "jdg", "jg", "jdgs"]),
    ("Ruth", &["ruth", "rth", "ru"]),
    ("1 Samuel", &["1sam", "1sa", "1sm", "isam", "1samuel"]),
    ("2 Samuel", &["2sam", "2sa", "2sm", "iisam", "2samuel"]),
    ("1 Kings", &["1kgs", "1ki", "1kin", "ikings", "1kings"]),
    ("2 Kings", &["2kgs", "2ki", "2kin", "iikings", "2kings"]),
    ("1 Chronicles", &["1chr", "1ch", "1chron", "1chronicles"]),
    ("2 Chronicles", &["2chr", "2ch", "2chron", "2chronicles"]),
    ("Ezra", &["ezra", "ezr"]),
    ("Nehemiah", &["neh", "ne"]),
    ("Esther", &["esth", "est", "es"]),
    ("Job", &["job", "jb"]),
    ("Psalms", &["ps", "psa", "psalm", "pslm", "psm", "pss"]),
    ("Proverbs", &["prov", "pro", "prv", "pr"]),
    ("Ecclesiastes", &["eccl", "ecc", "ec", "qoh"]),
    ("Song of Solomon", &["song", "sos", "so", "canticles", "songofsongs", "sng"]),
    ("Isaiah", &["isa", "is"]),
    ("Jeremiah", &["jer", "je", "jr"]),
    ("Lamentations", &["lam", "la"]),
    ("Ezekiel", &["ezek", "eze", "ezk"]),
    ("Daniel", &["dan", "da", "dn"]),
    ("Hosea", &["hos", "ho"]),
    ("Joel", &["joel", "jl"]),
    ("Amos", &["amos", "am"]),
    ("Obadiah", &["obad", "ob"]),
    ("Jonah", &["jonah", "jnh", "jon"]),
    ("Micah", &["mic", "mc"]),
    ("Nahum", &["nah", "na"]),
    ("Habakkuk", &["hab", "hb"]),
    ("Zephaniah", &["zeph", "zep", "zp"]),
    ("Haggai", &["hag", "hg"]),
    ("Zechariah", &["zech", "zec", "zc"]),
    ("Malachi", &["mal", "ml"]),
    ("Matthew", &["matt", "mt", "mat"]),
    ("Mark", &["mark", "mrk", "mar", "mk", "mr"]),
    ("Luke", &["luke", "luk", "lk"]),
    ("John", &["john", "joh", "jhn", "jn"]),
    ("Acts", &["acts", "act", "ac"]),
    ("Romans", &["rom", "ro", "rm"]),
    ("1 Corinthians", &["1cor", "1co", "icor", "1corinthians"]),
    ("2 Corinthians", &["2cor", "2co", "iicor", "2corinthians"]),
    ("Galatians", &["gal", "ga"]),
    ("Ephesians", &["eph", "ephes"]),
    ("Philippians", &["phil", "php", "pp"]),
    ("Colossians", &["col", "co"]),
    ("1 Thessalonians", &["1thess", "1th", "1thes", "1thessalonians"]),
    ("2 Thessalonians", &["2thess", "2th", "2thes", "2thessalonians"]),
    ("1 Timothy", &["1tim", "1ti", "1timothy"]),
    ("2 Timothy", &["2tim", "2ti", "2timothy"]),
    ("Titus", &["titus", "tit", "ti"]),
    ("Philemon", &["philem", "phm", "pm"]),
    ("Hebrews", &["heb"]),
    ("James", &["jas", "jm", "jam"]),
    ("1 Peter", &["1pet", "1pe", "1pt", "1peter"]),
    ("2 Peter", &["2pet", "2pe", "2pt", "2peter"]),
    ("1 John", &["1john", "1jn", "1jo", "1jhn"]),
    ("2 John", &["2john", "2jn", "2jo", "2jhn"]),
    ("3 John", &["3john", "3jn", "3jo", "3jhn"]),
    ("Jude", &["jude", "jud", "jd"]),
    ("Revelation", &["rev", "re", "revelations", "apocalypse"]),
];

const VPL_CODES: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH", "2CH", "EZR", "NEH",
    "EST", "JOB", "PSA", "PRO", "ECC", "SOL", "ISA", "JER", "LAM", "EZE", "DAN", "HOS", "JOE", "AMO", "OBA", "JON",
    "MIC", "NAH", "HAB", "ZEP", "HAG", "ZEC", "MAL", "MAT", "MAR", "LUK", "JOH", "ACT", "ROM", "1CO", "2CO", "GAL",
    "EPH", "PHI", "COL", "1TH", "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAM", "1PE", "2PE", "1JO", "2JO", "3JO",
    "JUD", "REV",
];

const USFM_CODES: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH", "2CH", "EZR", "NEH",
    "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK", "DAN", "HOS", "JOL", "AMO", "OBA", "JON",
    "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL", "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL",
    "EPH", "PHP", "COL", "1TH", "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN",
    "JUD", "REV",
];

#[derive(Clone, Debug)]
pub struct Verse {
    pub book: u8,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
}

pub struct Translation {
    pub abbrev: String,
    pub name: String,
    pub verses: Vec<Verse>,
    lower: Vec<String>,
    chapters: HashMap<(u8, u16), (usize, usize)>,
    pub chapter_counts: [u16; 66],
}

impl Translation {
    fn build(abbrev: &str, name: &str, verses: Vec<Verse>) -> Self {
        let mut chapters: HashMap<(u8, u16), (usize, usize)> = HashMap::new();
        let mut chapter_counts = [0u16; 66];
        for (i, v) in verses.iter().enumerate() {
            let e = chapters.entry((v.book, v.chapter)).or_insert((i, i + 1));
            e.1 = i + 1;
            let c = &mut chapter_counts[v.book as usize];
            *c = (*c).max(v.chapter);
        }
        let lower = verses.iter().map(|v| v.text.to_lowercase()).collect();
        Translation { abbrev: abbrev.into(), name: name.into(), verses, lower, chapters, chapter_counts }
    }

    pub fn from_tsv_gz(abbrev: &str, name: &str, bytes: &[u8]) -> Self {
        let mut s = String::new();
        let _ = GzDecoder::new(bytes).read_to_string(&mut s);
        let mut verses = Vec::with_capacity(32000);
        for line in s.lines() {
            let mut it = line.splitn(4, '\t');
            let (Some(b), Some(c), Some(v), Some(t)) = (it.next(), it.next(), it.next(), it.next()) else { continue };
            let (Ok(b), Ok(c), Ok(v)) = (b.parse::<u8>(), c.parse::<u16>(), v.parse::<u16>()) else { continue };
            verses.push(Verse { book: b, chapter: c, verse: v, text: t.to_string() });
        }
        Self::build(abbrev, name, verses)
    }

    pub fn from_vpl(abbrev: &str, name: &str, text: &str) -> Option<Self> {
        let mut verses = Vec::new();
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}');
            let Some((code, rest)) = line.split_once(' ') else { continue };
            let code = code.to_uppercase();
            let book = VPL_CODES.iter().position(|c| *c == code).or_else(|| USFM_CODES.iter().position(|c| *c == code));
            let Some(book) = book else { continue };
            let Some((cv, body)) = rest.split_once(' ') else { continue };
            let Some((c, v)) = cv.split_once(':') else { continue };
            let (Ok(c), Ok(v)) = (c.parse::<u16>(), v.parse::<u16>()) else { continue };
            let body = body.replace(['¶', '[', ']'], "");
            let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
            if body.is_empty() {
                continue;
            }
            verses.push(Verse { book: book as u8, chapter: c, verse: v, text: body });
        }
        if verses.len() < 100 {
            return None;
        }
        Some(Self::build(abbrev, name, verses))
    }

    pub fn chapter(&self, book: u8, chapter: u16) -> &[Verse] {
        match self.chapters.get(&(book, chapter)) {
            Some(&(a, b)) => &self.verses[a..b],
            None => &[],
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<usize> {
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(|s| s.to_string()).collect();
        if words.is_empty() {
            return vec![];
        }
        let phrase = query.trim().to_lowercase();
        let mut exact = Vec::new();
        let mut loose = Vec::new();
        for (i, l) in self.lower.iter().enumerate() {
            if l.contains(&phrase) {
                exact.push(i);
            } else if words.iter().all(|w| l.contains(w.as_str())) {
                loose.push(i);
            }
            if exact.len() >= limit {
                break;
            }
        }
        exact.extend(loose);
        exact.truncate(limit);
        exact
    }
}

pub fn book_name(b: u8) -> &'static str {
    BOOKS.get(b as usize).map(|x| x.0).unwrap_or("?")
}

fn norm(s: &str) -> String {
    let s = s.to_lowercase().replace(['.', ' '], "");
    if let Some(r) = s.strip_prefix("iii") {
        return format!("3{r}");
    }
    if let Some(r) = s.strip_prefix("ii") {
        return format!("2{r}");
    }
    if let Some(r) = s.strip_prefix("first") {
        return format!("1{r}");
    }
    if let Some(r) = s.strip_prefix("second") {
        return format!("2{r}");
    }
    if let Some(r) = s.strip_prefix("third") {
        return format!("3{r}");
    }
    s
}

pub fn find_book(q: &str) -> Option<u8> {
    let n = norm(q);
    if n.is_empty() {
        return None;
    }
    for (i, (name, abbrs)) in BOOKS.iter().enumerate() {
        if norm(name) == n || abbrs.iter().any(|a| *a == n) {
            return Some(i as u8);
        }
    }
    for (i, (name, _)) in BOOKS.iter().enumerate() {
        if norm(name).starts_with(&n) {
            return Some(i as u8);
        }
    }
    for (i, (_, abbrs)) in BOOKS.iter().enumerate() {
        if abbrs.iter().any(|a| a.starts_with(&n)) {
            return Some(i as u8);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reference {
    pub book: u8,
    pub chapter: u16,
    pub from: Option<u16>,
    pub to: Option<u16>,
}

pub fn parse_reference(input: &str) -> Option<Reference> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let chars: Vec<char> = s.chars().collect();
    let mut split = chars.len();
    for i in 1..chars.len() {
        if chars[i].is_ascii_digit() && (chars[i - 1] == ' ' || chars[i - 1].is_alphabetic() || chars[i - 1] == '.') {
            let before: String = chars[..i].iter().collect();
            if before.chars().any(|c| c.is_alphabetic()) {
                split = i;
                break;
            }
        }
    }
    let book_part: String = chars[..split].iter().collect();
    let rest: String = chars[split..].iter().collect();
    let book = find_book(book_part.trim())?;
    let nums: Vec<u16> = rest
        .split(|c: char| !c.is_ascii_digit())
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect();
    let chapter = *nums.first().unwrap_or(&1);
    let from = nums.get(1).copied();
    let to = nums.get(2).copied().filter(|t| Some(*t) >= from);
    Some(Reference { book, chapter, from, to })
}

pub fn format_ref(book: u8, chapter: u16, from: u16, to: u16) -> String {
    if from == to {
        format!("{} {}:{}", book_name(book), chapter, from)
    } else {
        format!("{} {}:{}-{}", book_name(book), chapter, from, to)
    }
}

pub struct BibleStore {
    pub translations: Vec<Translation>,
}

impl BibleStore {
    pub fn load(extra_dir: Option<std::path::PathBuf>) -> Self {
        let mut translations = vec![
            Translation::from_tsv_gz("WEB", "World English Bible", include_bytes!("../assets/bibles/web.tsv.gz")),
            Translation::from_tsv_gz("KJV", "King James Version", include_bytes!("../assets/bibles/kjv.tsv.gz")),
        ];
        if let Some(dir) = extra_dir {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
                files.sort();
                for p in files {
                    if p.extension().map(|e| e == "txt").unwrap_or(false) {
                        if let Ok(text) = std::fs::read_to_string(&p) {
                            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            let abbrev = abbrev_from_name(&stem);
                            if let Some(t) = Translation::from_vpl(&abbrev, &stem, &text) {
                                translations.push(t);
                            }
                        }
                    }
                }
            }
        }
        BibleStore { translations }
    }

    pub fn get(&self, abbrev: &str) -> &Translation {
        self.translations.iter().find(|t| t.abbrev == abbrev).unwrap_or(&self.translations[0])
    }
}

pub fn abbrev_from_name(stem: &str) -> String {
    let s = stem.trim_end_matches("_vpl").trim_start_matches("eng-");
    let s: String = s.chars().filter(|c| c.is_alphanumeric()).take(8).collect();
    if s.is_empty() { "BIBLE".into() } else { s.to_uppercase() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_references() {
        let r = parse_reference("John 3:16").unwrap();
        assert_eq!((book_name(r.book), r.chapter, r.from, r.to), ("John", 3, Some(16), None));
        let r = parse_reference("1 cor 13:4-7").unwrap();
        assert_eq!((book_name(r.book), r.chapter, r.from, r.to), ("1 Corinthians", 13, Some(4), Some(7)));
        let r = parse_reference("ps 23").unwrap();
        assert_eq!((book_name(r.book), r.chapter, r.from), ("Psalms", 23, None));
        let r = parse_reference("1jn 1 9").unwrap();
        assert_eq!((book_name(r.book), r.chapter, r.from), ("1 John", 1, Some(9)));
        let r = parse_reference("Rev 21").unwrap();
        assert_eq!(book_name(r.book), "Revelation");
        let r = parse_reference("phil 4:13").unwrap();
        assert_eq!(book_name(r.book), "Philippians");
    }

    #[test]
    fn loads_bundled_bibles() {
        let b = BibleStore::load(None);
        let web = b.get("WEB");
        assert!(web.verses.len() > 31000);
        let j = web.chapter(42, 3);
        assert!(j.iter().any(|v| v.verse == 16 && v.text.contains("loved the world")));
        let kjv = b.get("KJV");
        assert_eq!(kjv.chapter_counts[18], 150);
        assert!(!kjv.search("shepherd", 50).is_empty());
    }
}
