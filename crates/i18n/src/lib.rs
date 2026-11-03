//! YouCraft's interface in every language.
//!
//! Text is looked up by its English source, as craft-i18n's catalogs key it: `t(lang, "Home")`.
//! The server embeds every catalog of `locales/` (feature `embed`) and picks each request's
//! language from the person's choice or their browser; the browser installs the one catalog its
//! page carries, so it hydrates with the same words. A missing translation shows the English.

mod catalogs {
    include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));
}

mod compact;
mod compact_data;
pub mod when;

use std::sync::OnceLock;

pub use craft_i18n::{I18n, Lang};

static I18N: OnceLock<I18n> = OnceLock::new();

/// The catalogs: the embedded ones, unless [`install`] came first.
pub fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        let offered: Vec<&str> = catalogs::EMBEDDED
            .iter()
            .map(|(code, _, _)| *code)
            .collect();
        I18n::new(catalogs::EMBEDDED, &offered).0
    })
}

/// Use `catalog`, the text of `code`'s `youcraft.tsv`, as the only translation, as the browser
/// does with the one its page carries. Only a call before the first lookup counts; it returns
/// whether this one did.
pub fn install(code: &str, catalog: &str) -> bool {
    let entry: &'static [(&'static str, &'static str, &'static str)] =
        Box::leak(Box::new([(leak(code), "", leak(catalog))]));
    let code: &'static str = leak(code);
    I18N.set(I18n::new(entry, &[code]).0).is_ok()
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

/// The catalog text of `lang`, for the page to carry to the browser; empty for English.
pub fn catalog_text(lang: Lang) -> &'static str {
    let code = i18n().code(lang);
    catalogs::EMBEDDED
        .iter()
        .find(|(c, _, _)| *c == code)
        .map_or("", |(_, _, text)| text)
}

/// `source` in `lang`.
pub fn t(lang: Lang, source: &'static str) -> &'static str {
    i18n().t(lang, source)
}

/// The wording of a count in `lang`, the number in full: `1,234 comments`. `one` and `other`
/// are the English forms with `{n}`.
pub fn count(lang: Lang, n: u64, one: &str, other: &str) -> String {
    worded(lang, n, &craft_i18n::ltr(&number(lang, n)), one, other)
}

/// The wording of a count in `lang`, the number compact as YouTube shows views and subscribers:
/// `1.2K views`, `1,2 k vues`. The words take the form of the number shown, 1200, as Russian
/// `1,2 тыс. просмотров` does where 1234 would be `просмотра`.
pub fn compact_count(lang: Lang, n: u64, one: &str, other: &str) -> String {
    let (text, shown) = compact::compact(lang, n);
    worded(lang, shown, &text, one, other)
}

/// `n` in full, grouped as `lang` groups numbers: `1,234,567`, `1 234 567`, `۱٬۲۳۴٬۵۶۷`.
pub fn number(lang: Lang, n: u64) -> String {
    compact::full(lang, n)
}

/// `n` compact, as a like button shows it: `999`, `1.2K`, `12K`, `1.2M`, `1,2 Mio.`, `1.2万`.
pub fn compact(lang: Lang, n: u64) -> String {
    compact::compact(lang, n).0
}

/// The form craft-i18n's catalogs give `n` in `lang`, with `shown` where it has `{n}`. craft-i18n
/// writes the number in one format for the whole process, so the form is found as the one that
/// reads as its plain wording does.
fn worded(lang: Lang, n: u64, shown: &str, one: &str, other: &str) -> String {
    let i18n = i18n();
    let plain = i18n.trn_plain(lang, n, one, other);
    let digits = n.to_string();
    let key = format!("{one}|{other}");
    let form = i18n
        .get(lang, "@plural", &key)
        .into_iter()
        .flat_map(|forms| forms.split('|'))
        .chain([one, other])
        .find(|form| craft_i18n::fmt_plain(form, &[("n", &digits)]) == plain);
    match form {
        Some(form) => craft_i18n::fmt(form, &[("n", shown)]),
        None => plain,
    }
}

/// `template` with each `{name}` replaced by its value, each value isolated from the text around
/// it so that a name in another script keeps its direction: `fill(t(lang, "Language: {language}"),
/// &[("language", "Français")])`.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    craft_i18n::fmt(template, values)
}

/// `value` with `decimals` places, in the digits and separators of `lang`.
pub fn decimal(lang: Lang, value: f64, decimals: usize) -> String {
    craft_i18n::Format::from_tag(code(lang)).number(value, decimals)
}

/// `ratio` as a whole percentage, as `lang` writes one: 0.5 is "50%" in English.
pub fn percent(lang: Lang, ratio: f64) -> String {
    craft_i18n::Format::from_tag(code(lang)).percent(ratio, 0)
}

/// The language a request should be answered in: `chosen` if the person chose an offered one,
/// else the best match of the browser's `Accept-Language`, else English.
pub fn negotiate(chosen: Option<&str>, accept_language: &str) -> Lang {
    if let Some(lang) = chosen.and_then(|code| i18n().from_code(code)) {
        return lang;
    }
    let mut tags: Vec<(u32, usize, &str)> = accept_language
        .split(',')
        .enumerate()
        .filter_map(|(index, item)| {
            let mut parts = item.split(';');
            let tag = parts.next()?.trim();
            let weight = parts
                .find_map(|p| p.trim().strip_prefix("q="))
                .map_or(1000, quality);
            (!tag.is_empty() && tag != "*" && weight > 0).then_some((weight, index, tag))
        })
        .collect();
    tags.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    i18n().match_tags(tags.into_iter().map(|(_, _, tag)| tag))
}

/// A `q=` value in thousandths: `0.8` is 800; anything malformed is 0.
fn quality(text: &str) -> u32 {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let whole: u32 = match whole {
        "0" => 0,
        "1" => 1000,
        _ => return 0,
    };
    let digits: String = fraction.chars().take(3).collect();
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return 0;
    }
    let thousandths = format!("{digits:0<3}").parse::<u32>().unwrap_or(0);
    (whole + thousandths).min(1000)
}

/// The BCP 47 code of `lang`, for `<html lang>`.
pub fn code(lang: Lang) -> &'static str {
    i18n().code(lang)
}

/// Whether `lang` is written right to left, for `<html dir>`.
pub fn rtl(lang: Lang) -> bool {
    i18n().rtl(lang)
}

/// The languages a person may choose, by their own names.
pub fn languages() -> Vec<(Lang, &'static str)> {
    let i18n = i18n();
    i18n.languages()
        .into_iter()
        .map(|lang| (lang, i18n.name(lang)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualities_read_in_thousandths() {
        assert_eq!(quality("1"), 1000);
        assert_eq!(quality("0.8"), 800);
        assert_eq!(quality("0.123"), 123);
        assert_eq!(quality("0.1234"), 123);
        assert_eq!(quality("2"), 0);
        assert_eq!(quality("0.x"), 0);
    }

    #[test]
    fn numbers_are_written_as_each_language_writes_them() {
        assert_eq!(decimal(Lang::EN, 1.25, 2), "1.25");
        assert_eq!(decimal(Lang::EN, 2.0, 0), "2");
        assert_eq!(percent(Lang::EN, 0.5), "50%");
    }

    #[test]
    fn counts_word_the_number_they_show() {
        let lang = |code| i18n().from_code(code).unwrap();
        let views = |lang, n| {
            (
                count(lang, n, "{n} view", "{n} views"),
                compact_count(lang, n, "{n} view", "{n} views"),
            )
        };
        assert_eq!(
            views(Lang::EN, 1),
            ("1 view".to_owned(), "1 view".to_owned())
        );
        assert_eq!(
            views(Lang::EN, 1234),
            ("1,234 views".to_owned(), "1.2K views".to_owned())
        );
        let (full, compact) = views(lang("fr"), 1234);
        assert_eq!(
            (full.as_str(), compact.as_str()),
            ("1\u{202f}234 vues", "1,2\u{a0}k vues")
        );
        let (full, compact) = views(lang("ru"), 1234);
        assert!(
            full.ends_with(" просмотра") && compact.ends_with("\u{a0}тыс. просмотров"),
            "{full} / {compact}"
        );
        let (_, compact) = views(lang("ar"), 1500);
        assert!(
            compact.contains("\u{2068}1.5\u{a0}ألف\u{2069}"),
            "{compact:?}"
        );
    }

    #[test]
    fn english_answers_when_nothing_matches() {
        assert_eq!(code(negotiate(None, "")), "en");
        assert_eq!(code(negotiate(Some("tlh"), "x-klingon;q=0.9")), "en");
    }
}

#[cfg(test)]
mod catalogs_tests {
    //! The catalogs of `locales/` against each other and against the code: each parses cleanly,
    //! and each translates every string the interface writes. A string the interface writes is
    //! `tr("…")`, or the `"{n} …"` pair of a count.

    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use craft_i18n::I18n;

    fn workspace() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Every catalog: (locale, text).
    fn catalogs() -> Vec<(String, String)> {
        let mut found: Vec<(String, String)> = std::fs::read_dir(workspace().join("locales"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter_map(|dir| {
                let text = std::fs::read_to_string(dir.join("youcraft.tsv")).ok()?;
                Some((dir.file_name()?.to_string_lossy().into_owned(), text))
            })
            .collect();
        found.sort();
        found
    }

    fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, into);
            } else if path.extension().is_some_and(|e| e == "rs") {
                into.push(path);
            }
        }
    }

    /// The string literal that opens `text`, unescaped, and what follows it.
    fn literal(text: &str) -> Option<(String, &str)> {
        let mut chars = text.strip_prefix('"')?.char_indices();
        let mut value = String::new();
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => return Some((value, &text[at + 2..])),
                '\\' => value.push(match chars.next()?.1 {
                    'n' => '\n',
                    other => other,
                }),
                c => value.push(c),
            }
        }
        None
    }

    /// Every (context, source) the code outside comments writes: `tr("…")` as plain rows, and
    /// `"{n} …", "{n} …"` pairs as `@plural` rows.
    fn written() -> BTreeSet<(String, String)> {
        let mut files = Vec::new();
        rust_files(&workspace().join("crates"), &mut files);
        let mut strings = BTreeSet::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let code: String = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .flat_map(|line| [line, "\n"])
                .collect();
            for (at, _) in code.match_indices("tr(\"") {
                let before = code[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                if let Some((source, _)) = literal(&code[at + 3..]) {
                    strings.insert((String::new(), source));
                }
            }
            for (at, _) in code.match_indices("\"{n}") {
                let Some((one, rest)) = literal(&code[at..]) else {
                    continue;
                };
                let rest = rest.trim_start().strip_prefix(',').map(str::trim_start);
                if let Some((other, _)) =
                    rest.and_then(|rest| rest.starts_with("\"{n}").then(|| literal(rest)).flatten())
                {
                    strings.insert(("@plural".to_owned(), format!("{one}|{other}")));
                }
            }
        }
        strings
    }

    #[test]
    fn every_catalog_parses_cleanly() {
        let all = catalogs();
        assert!(all.len() >= 41, "the 41 languages of the Craft apps");
        let embedded: Vec<(&'static str, &'static str, &'static str)> = all
            .iter()
            .map(|(code, text)| (super::leak(code), "", super::leak(text)))
            .collect();
        let embedded: &'static [(&'static str, &'static str, &'static str)] =
            Box::leak(embedded.into_boxed_slice());
        let offered: Vec<&str> = embedded.iter().map(|(code, _, _)| *code).collect();
        let (i18n, problems) = I18n::new(embedded, &offered);
        assert!(problems.is_empty(), "{problems:#?}");
        let problems = i18n.check_all();
        assert!(problems.is_empty(), "{problems:#?}");
        for lang in i18n.languages() {
            assert_ne!(
                i18n.name(lang),
                i18n.code(lang),
                "{}: no language.name",
                i18n.code(lang)
            );
        }
    }

    #[test]
    fn every_catalog_translates_every_string_the_code_writes() {
        let written = written();
        assert!(written.len() > 20, "the scan finds the interface's strings");
        for (code, text) in catalogs() {
            let rows: BTreeSet<(String, String)> = text
                .lines()
                .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
                .filter_map(|line| {
                    let mut columns = line.split('\t');
                    Some((columns.next()?.to_owned(), columns.next()?.to_owned()))
                })
                .collect();
            let missing: Vec<_> = written.difference(&rows).collect();
            assert!(missing.is_empty(), "{code} lacks {missing:#?}");
            let unused: Vec<_> = rows
                .iter()
                .filter(|row| row.0 != "@id" && !written.contains(*row))
                .collect();
            assert!(
                unused.is_empty(),
                "{code} translates strings the code no longer writes: {unused:#?}"
            );
        }
    }
}
