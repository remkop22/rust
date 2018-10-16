// Copyright 2014-2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.



#![allow(clippy::default_hash_types)]

use itertools::Itertools;
use lazy_static::lazy_static;
use regex::Regex;
use walkdir::WalkDir;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::prelude::*;

lazy_static! {
    static ref DEC_CLIPPY_LINT_RE: Regex = Regex::new(r#"(?x)
        declare_clippy_lint!\s*[\{(]\s*
        pub\s+(?P<name>[A-Z_][A-Z_0-9]*)\s*,\s*
        (?P<cat>[a-z_]+)\s*,\s*
        "(?P<desc>(?:[^"\\]+|\\(?s).(?-s))*)"\s*[})]
    "#).unwrap();
    static ref DEC_DEPRECATED_LINT_RE: Regex = Regex::new(r#"(?x)
        declare_deprecated_lint!\s*[{(]\s*
        pub\s+(?P<name>[A-Z_][A-Z_0-9]*)\s*,\s*
        "(?P<desc>(?:[^"\\]+|\\(?s).(?-s))*)"\s*[})]
    "#).unwrap();
    static ref NL_ESCAPE_RE: Regex = Regex::new(r#"\\\n\s*"#).unwrap();
    pub static ref DOCS_LINK: String = "https://rust-lang-nursery.github.io/rust-clippy/master/index.html".to_string();
}

/// Lint data parsed from the Clippy source code.
#[derive(Clone, PartialEq, Debug)]
pub struct Lint {
    pub name: String,
    pub group: String,
    pub desc: String,
    pub deprecation: Option<String>,
    pub module: String,
}

impl Lint {
    pub fn new(name: &str, group: &str, desc: &str, deprecation: Option<&str>, module: &str) -> Self {
        Self {
            name: name.to_lowercase(),
            group: group.to_string(),
            desc: NL_ESCAPE_RE.replace(&desc.replace("\\\"", "\""), "").to_string(),
            deprecation: deprecation.map(|d| d.to_string()),
            module: module.to_string(),
        }
    }

    /// Returns all non-deprecated lints and non-internal lints
    pub fn usable_lints(lints: impl Iterator<Item=Self>) -> impl Iterator<Item=Self> {
        lints.filter(|l| l.deprecation.is_none() && !l.group.starts_with("internal"))
    }

    /// Returns the lints in a HashMap, grouped by the different lint groups
    pub fn by_lint_group(lints: &[Self]) -> HashMap<String, Vec<Self>> {
        lints.iter().map(|lint| (lint.group.to_string(), lint.clone())).into_group_map()
    }
}

/// Gathers all files in `src/clippy_lints` and gathers all lints inside
pub fn gather_all() -> impl Iterator<Item=Lint> {
    lint_files().flat_map(|f| gather_from_file(&f))
}

fn gather_from_file(dir_entry: &walkdir::DirEntry) -> impl Iterator<Item=Lint> {
    let mut file = fs::File::open(dir_entry.path()).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    parse_contents(&content, dir_entry.path().file_stem().unwrap().to_str().unwrap())
}

fn parse_contents(content: &str, filename: &str) -> impl Iterator<Item=Lint> {
    let lints = DEC_CLIPPY_LINT_RE
        .captures_iter(content)
        .map(|m| Lint::new(&m["name"], &m["cat"], &m["desc"], None, filename));
    let deprecated = DEC_DEPRECATED_LINT_RE
        .captures_iter(content)
        .map(|m| Lint::new( &m["name"], "Deprecated", &m["desc"], Some(&m["desc"]), filename));
    // Removing the `.collect::<Vec<Lint>>().into_iter()` causes some lifetime issues due to the map
    lints.chain(deprecated).collect::<Vec<Lint>>().into_iter()
}

/// Collects all .rs files in the `clippy_lints/src` directory
fn lint_files() -> impl Iterator<Item=walkdir::DirEntry> {
    // We use `WalkDir` instead of `fs::read_dir` here in order to recurse into subdirectories.
    // Otherwise we would not collect all the lints, for example in `clippy_lints/src/methods/`.
    WalkDir::new("../clippy_lints/src")
        .into_iter()
        .filter_map(|f| f.ok())
        .filter(|f| f.path().extension() == Some(OsStr::new("rs")))
}

#[test]
fn test_parse_contents() {
    let result: Vec<Lint> = parse_contents(
        r#"
declare_clippy_lint! {
    pub PTR_ARG,
    style,
    "really long \
     text"
}

declare_clippy_lint!{
    pub DOC_MARKDOWN,
    pedantic,
    "single line"
}

/// some doc comment
declare_deprecated_lint! {
    pub SHOULD_ASSERT_EQ,
    "`assert!()` will be more flexible with RFC 2011"
}
    "#,
    "module_name").collect();

    let expected = vec![
        Lint::new("ptr_arg", "style", "really long text", None, "module_name"),
        Lint::new("doc_markdown", "pedantic", "single line", None, "module_name"),
        Lint::new(
            "should_assert_eq",
            "Deprecated",
            "`assert!()` will be more flexible with RFC 2011",
            Some("`assert!()` will be more flexible with RFC 2011"),
            "module_name"
        ),
    ];
    assert_eq!(expected, result);
}

#[test]
fn test_usable_lints() {
    let lints = vec![
        Lint::new("should_assert_eq", "Deprecated", "abc", Some("Reason"), "module_name"),
        Lint::new("should_assert_eq2", "Not Deprecated", "abc", None, "module_name"),
        Lint::new("should_assert_eq2", "internal", "abc", None, "module_name"),
        Lint::new("should_assert_eq2", "internal_style", "abc", None, "module_name")
    ];
    let expected = vec![
        Lint::new("should_assert_eq2", "Not Deprecated", "abc", None, "module_name")
    ];
    assert_eq!(expected, Lint::usable_lints(lints.into_iter()).collect::<Vec<Lint>>());
}

#[test]
fn test_by_lint_group() {
    let lints = vec![
        Lint::new("should_assert_eq", "group1", "abc", None, "module_name"),
        Lint::new("should_assert_eq2", "group2", "abc", None, "module_name"),
        Lint::new("incorrect_match", "group1", "abc", None, "module_name"),
    ];
    let mut expected: HashMap<String, Vec<Lint>> = HashMap::new();
    expected.insert("group1".to_string(), vec![
        Lint::new("should_assert_eq", "group1", "abc", None, "module_name"),
        Lint::new("incorrect_match", "group1", "abc", None, "module_name"),
    ]);
    expected.insert("group2".to_string(), vec![
        Lint::new("should_assert_eq2", "group2", "abc", None, "module_name")
    ]);
    assert_eq!(expected, Lint::by_lint_group(&lints));
}
