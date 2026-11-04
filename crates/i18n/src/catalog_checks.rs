//! The catalogs of `locales/` against each other and against the code: each parses cleanly, and
//! each translates exactly the strings the interface writes, no fewer and no more.
//!
//! A string the interface writes is the one string literal of a `tr(…)` call, or the last two
//! arguments of a call when both are string literals with `{n}`, the English forms of a count:
//! `count(n, "{n} view", "{n} views")`. The code is read as Rust tokens, so comments, raw strings,
//! escapes and calls split across lines read as the compiler reads them; a `tr` called with
//! anything but one string literal fails the check, since no catalog could be checked against it.
//! A catalog is read by craft-i18n, alone, so that no fallback to another locale hides a row it
//! lacks; its rows are listed by a reader that craft-i18n confirms row by row.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use craft_i18n::{I18n, Lang};
use proc_macro2::{Delimiter, TokenStream, TokenTree};

/// A row of a catalog, or a string the code writes: (context, source).
type Row = (String, String);

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every catalog: (locale, text), by locale.
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

/// craft-i18n over `catalogs`, offering each, and the problems it finds while indexing them.
fn indexed(catalogs: &[(String, String)]) -> (I18n, Vec<String>) {
    let embedded: Vec<(&'static str, &'static str, &'static str)> = catalogs
        .iter()
        .map(|(code, text)| (crate::leak(code), "", crate::leak(text)))
        .collect();
    let embedded: &'static [(&'static str, &'static str, &'static str)] =
        Box::leak(embedded.into_boxed_slice());
    let offered: Vec<&str> = embedded.iter().map(|(code, _, _)| *code).collect();
    I18n::new(embedded, &offered)
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

/// The value of `argument` if it is exactly one string literal, escapes and raw strings read.
fn string(argument: &[TokenTree]) -> Option<String> {
    match argument {
        [TokenTree::Literal(literal)] => match syn::Lit::new(literal.clone()) {
            syn::Lit::Str(text) => Some(text.value()),
            _ => None,
        },
        _ => None,
    }
}

/// The arguments of a parenthesized list, split at its commas; a trailing comma adds none.
fn arguments(list: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut all = vec![Vec::new()];
    for token in list {
        match &token {
            TokenTree::Punct(punct) if punct.as_char() == ',' => all.push(Vec::new()),
            _ => all.last_mut().unwrap().push(token),
        }
    }
    if all.last().is_some_and(Vec::is_empty) {
        all.pop();
    }
    all
}

fn is_ident(token: Option<&TokenTree>, name: &str) -> bool {
    matches!(token, Some(TokenTree::Ident(ident)) if ident == name)
}

/// Add what `tokens`, from `file`, write to `into`, looking inside every group, macro bodies
/// (`view!`) included.
fn scan(tokens: TokenStream, file: &Path, into: &mut BTreeSet<Row>) {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    for (at, token) in tokens.iter().enumerate() {
        let TokenTree::Group(group) = token else {
            continue;
        };
        if group.delimiter() == Delimiter::Parenthesis {
            let before = |back: usize| at.checked_sub(back).and_then(|i| tokens.get(i));
            let arguments = arguments(group.stream());
            if is_ident(before(1), "tr") && !is_ident(before(2), "fn") {
                let source = match arguments.as_slice() {
                    [only] => string(only),
                    _ => None,
                };
                let Some(source) = source else {
                    panic!(
                        "{}: tr({}) takes one string literal",
                        file.display(),
                        group.stream()
                    );
                };
                into.insert((String::new(), source));
            }
            if let [.., one, other] = arguments.as_slice()
                && let (Some(one), Some(other)) = (string(one), string(other))
                && one.contains("{n}")
                && other.contains("{n}")
            {
                into.insert(("@plural".to_owned(), format!("{one}|{other}")));
            }
        }
        scan(group.stream(), file, into);
    }
}

/// Every (context, source) the code of `crates/` writes: `tr` sources as plain rows, the forms
/// of counts as `@plural` rows.
fn written() -> BTreeSet<Row> {
    let mut files = Vec::new();
    rust_files(&workspace().join("crates"), &mut files);
    let mut strings = BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let tokens: TokenStream = text
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        scan(tokens, &file, &mut strings);
    }
    strings
}

/// A column of a catalog row with its escapes read, as craft-i18n's catalog format writes them:
/// `\n`, `\t` and `\\`.
fn unescaped(column: &str) -> String {
    let mut out = String::new();
    let mut chars = column.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The (context, source) of every row of `text` but its `@id` rows.
fn rows(text: &str) -> BTreeSet<Row> {
    text.lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut columns = line.split('\t');
            Some((unescaped(columns.next()?), unescaped(columns.next()?)))
        })
        .filter(|(context, _)| context != "@id")
        .collect()
}

#[test]
fn every_catalog_parses_cleanly() {
    let all = catalogs();
    assert!(all.len() >= 41, "the 41 languages of the Craft apps");
    let (i18n, problems) = indexed(&all);
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
fn every_catalog_translates_every_string_the_code_writes_and_no_other() {
    let written = written();
    assert!(written.len() > 20, "the scan finds the interface's strings");
    for (code, text) in catalogs() {
        let (alone, _) = indexed(&[(code.clone(), text.clone())]);
        let lang: Lang = alone.from_code(&code).unwrap();
        let translates = |(context, source): &Row| alone.get(lang, context, source).is_some();
        let missing: Vec<&Row> = written.iter().filter(|row| !translates(row)).collect();
        assert!(missing.is_empty(), "{code} lacks {missing:#?}");
        let rows = rows(&text);
        let misread: Vec<&Row> = rows.iter().filter(|row| !translates(row)).collect();
        assert!(
            misread.is_empty(),
            "{code}: rows craft-i18n does not read so: {misread:#?}"
        );
        let unused: Vec<&Row> = rows.difference(&written).collect();
        assert!(
            unused.is_empty(),
            "{code} translates strings the code no longer writes: {unused:#?}"
        );
    }
}

#[test]
fn the_scan_reads_code_as_the_compiler_does() {
    let code = r##"
        // tr("in a comment")
        /// tr("in a doc comment")
        fn tr(source: &'static str) -> &'static str { source }
        fn page() {
            tr(
                "split across lines",
            );
            lang::tr(r#"raw "quoted""#);
            view! { <p>{tr("in a \"view\"\n")}</p> };
            count(n, "{n} view",
                "{n} views");
            compact_count(n, "{n} subscriber", "{n} subscribers");
            fill("{n} of {total}", &[("n", "1"), ("total", "2")]);
            xtr("not a lookup");
        }
    "##;
    let mut found = BTreeSet::new();
    scan(code.parse().unwrap(), Path::new("sample.rs"), &mut found);
    let plain = |s: &str| (String::new(), s.to_owned());
    let plural = |s: &str| ("@plural".to_owned(), s.to_owned());
    let expected: BTreeSet<Row> = [
        plain("split across lines"),
        plain("raw \"quoted\""),
        plain("in a \"view\"\n"),
        plural("{n} view|{n} views"),
        plural("{n} subscriber|{n} subscribers"),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, expected);
}

#[test]
#[should_panic(expected = "takes one string literal")]
fn a_lookup_of_anything_but_a_literal_fails_the_scan() {
    scan(
        "fn page() { tr(label); }".parse().unwrap(),
        Path::new("sample.rs"),
        &mut BTreeSet::new(),
    );
}

#[test]
fn rows_read_their_escapes() {
    let rows = rows("# a comment\n\n\tHome\tAccueil\n\tLine\\none\tLigne\\nune\r\n@id\tx\ty\n");
    let expected: BTreeSet<Row> = [
        (String::new(), "Home".to_owned()),
        (String::new(), "Line\none".to_owned()),
    ]
    .into_iter()
    .collect();
    assert_eq!(rows, expected);
}
