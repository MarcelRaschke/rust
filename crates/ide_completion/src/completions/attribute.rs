//! Completion for attributes
//!
//! This module uses a bit of static metadata to provide completions
//! for built-in attributes.

use once_cell::sync::Lazy;
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::{ast, AstNode, SyntaxKind, T};

use crate::{
    context::CompletionContext,
    generated_lint_completions::{CLIPPY_LINTS, FEATURES},
    item::{CompletionItem, CompletionItemKind, CompletionKind},
    Completions,
};

mod derive;
mod lint;
pub(crate) use self::lint::LintCompletion;

pub(crate) fn complete_attribute(acc: &mut Completions, ctx: &CompletionContext) -> Option<()> {
    let attribute = ctx.attribute_under_caret.as_ref()?;
    match (attribute.path().and_then(|p| p.as_single_name_ref()), attribute.token_tree()) {
        (Some(path), Some(token_tree)) => match path.text().as_str() {
            "derive" => derive::complete_derive(acc, ctx, token_tree),
            "feature" => lint::complete_lint(acc, ctx, token_tree, FEATURES),
            "allow" | "warn" | "deny" | "forbid" => {
                lint::complete_lint(acc, ctx, token_tree.clone(), lint::DEFAULT_LINT_COMPLETIONS);
                lint::complete_lint(acc, ctx, token_tree, CLIPPY_LINTS);
            }
            _ => (),
        },
        (None, Some(_)) => (),
        _ => complete_new_attribute(acc, ctx, attribute),
    }
    Some(())
}

fn complete_new_attribute(acc: &mut Completions, ctx: &CompletionContext, attribute: &ast::Attr) {
    let attribute_annotated_item_kind = attribute.syntax().parent().map(|it| it.kind());
    let attributes = attribute_annotated_item_kind.and_then(|kind| {
        if ast::Expr::can_cast(kind) {
            Some(EXPR_ATTRIBUTES)
        } else {
            KIND_TO_ATTRIBUTES.get(&kind).copied()
        }
    });
    let is_inner = attribute.kind() == ast::AttrKind::Inner;

    let add_completion = |attr_completion: &AttrCompletion| {
        let mut item = CompletionItem::new(
            CompletionKind::Attribute,
            ctx.source_range(),
            attr_completion.label,
        );
        item.kind(CompletionItemKind::Attribute);

        if let Some(lookup) = attr_completion.lookup {
            item.lookup_by(lookup);
        }

        if let Some((snippet, cap)) = attr_completion.snippet.zip(ctx.config.snippet_cap) {
            item.insert_snippet(cap, snippet);
        }

        if is_inner || !attr_completion.prefer_inner {
            acc.add(item.build());
        }
    };

    match attributes {
        Some(applicable) => applicable
            .iter()
            .flat_map(|name| ATTRIBUTES.binary_search_by(|attr| attr.key().cmp(name)).ok())
            .flat_map(|idx| ATTRIBUTES.get(idx))
            .for_each(add_completion),
        None if is_inner => ATTRIBUTES.iter().for_each(add_completion),
        None => ATTRIBUTES.iter().filter(|compl| !compl.prefer_inner).for_each(add_completion),
    }
}

struct AttrCompletion {
    label: &'static str,
    lookup: Option<&'static str>,
    snippet: Option<&'static str>,
    prefer_inner: bool,
}

impl AttrCompletion {
    fn key(&self) -> &'static str {
        self.lookup.unwrap_or(self.label)
    }

    const fn prefer_inner(self) -> AttrCompletion {
        AttrCompletion { prefer_inner: true, ..self }
    }
}

const fn attr(
    label: &'static str,
    lookup: Option<&'static str>,
    snippet: Option<&'static str>,
) -> AttrCompletion {
    AttrCompletion { label, lookup, snippet, prefer_inner: false }
}

macro_rules! attrs {
    [@ { item $($tt:tt)* } {$($acc:tt)*}] => {
        attrs!(@ { $($tt)* } { $($acc)*, "deprecated", "doc", "dochidden", "docalias", "must_use", "no_mangle" })
    };
    [@ { adt $($tt:tt)* } {$($acc:tt)*}] => {
        attrs!(@ { $($tt)* } { $($acc)*, "derive", "repr" })
    };
    [@ { linkable $($tt:tt)* } {$($acc:tt)*}] => {
        attrs!(@ { $($tt)* } { $($acc)*, "export_name", "link_name", "link_section" }) };
    [@ { $ty:ident $($tt:tt)* } {$($acc:tt)*}] => { compile_error!(concat!("unknown attr subtype ", stringify!($ty)))
    };
    [@ { $lit:literal $($tt:tt)*} {$($acc:tt)*}] => {
        attrs!(@ { $($tt)* } { $($acc)*, $lit })
    };
    [@ {$($tt:tt)+} {$($tt2:tt)*}] => {
        compile_error!(concat!("Unexpected input ", stringify!($($tt)+)))
    };
    [@ {} {$($tt:tt)*}] => { &[$($tt)*] as _ };
    [$($tt:tt),*] => {
        attrs!(@ { $($tt)* } { "allow", "cfg", "cfg_attr", "deny", "forbid", "warn" })
    };
}

#[rustfmt::skip]
static KIND_TO_ATTRIBUTES: Lazy<FxHashMap<SyntaxKind, &[&str]>> = Lazy::new(|| {
    std::array::IntoIter::new([
        (
            SyntaxKind::SOURCE_FILE,
            attrs!(
                item,
                "crate_name", "feature", "no_implicit_prelude", "no_main", "no_std",
                "recursion_limit", "type_length_limit", "windows_subsystem"
            ),
        ),
        (SyntaxKind::MODULE, attrs!(item, "no_implicit_prelude", "path")),
        (SyntaxKind::ITEM_LIST, attrs!(item, "no_implicit_prelude")),
        (SyntaxKind::MACRO_RULES, attrs!(item, "macro_export", "macro_use")),
        (SyntaxKind::MACRO_DEF, attrs!(item)),
        (SyntaxKind::EXTERN_CRATE, attrs!(item, "macro_use", "no_link")),
        (SyntaxKind::USE, attrs!(item)),
        (SyntaxKind::TYPE_ALIAS, attrs!(item)),
        (SyntaxKind::STRUCT, attrs!(item, adt, "non_exhaustive")),
        (SyntaxKind::ENUM, attrs!(item, adt, "non_exhaustive")),
        (SyntaxKind::UNION, attrs!(item, adt)),
        (SyntaxKind::CONST, attrs!(item)),
        (
            SyntaxKind::FN,
            attrs!(
                item, linkable,
                "cold", "ignore", "inline", "must_use", "panic_handler", "proc_macro",
                "proc_macro_derive", "proc_macro_attribute", "should_panic", "target_feature",
                "test", "track_caller"
            ),
        ),
        (SyntaxKind::STATIC, attrs!(item, linkable, "global_allocator", "used")),
        (SyntaxKind::TRAIT, attrs!(item, "must_use")),
        (SyntaxKind::IMPL, attrs!(item, "automatically_derived")),
        (SyntaxKind::ASSOC_ITEM_LIST, attrs!(item)),
        (SyntaxKind::EXTERN_BLOCK, attrs!(item, "link")),
        (SyntaxKind::EXTERN_ITEM_LIST, attrs!(item, "link")),
        (SyntaxKind::MACRO_CALL, attrs!()),
        (SyntaxKind::SELF_PARAM, attrs!()),
        (SyntaxKind::PARAM, attrs!()),
        (SyntaxKind::RECORD_FIELD, attrs!()),
        (SyntaxKind::VARIANT, attrs!("non_exhaustive")),
        (SyntaxKind::TYPE_PARAM, attrs!()),
        (SyntaxKind::CONST_PARAM, attrs!()),
        (SyntaxKind::LIFETIME_PARAM, attrs!()),
        (SyntaxKind::LET_STMT, attrs!()),
        (SyntaxKind::EXPR_STMT, attrs!()),
        (SyntaxKind::LITERAL, attrs!()),
        (SyntaxKind::RECORD_EXPR_FIELD_LIST, attrs!()),
        (SyntaxKind::RECORD_EXPR_FIELD, attrs!()),
        (SyntaxKind::MATCH_ARM_LIST, attrs!()),
        (SyntaxKind::MATCH_ARM, attrs!()),
        (SyntaxKind::IDENT_PAT, attrs!()),
        (SyntaxKind::RECORD_PAT_FIELD, attrs!()),
    ])
    .collect()
});
const EXPR_ATTRIBUTES: &[&str] = attrs!();

/// https://doc.rust-lang.org/reference/attributes.html#built-in-attributes-index
// Keep these sorted for the binary search!
const ATTRIBUTES: &[AttrCompletion] = &[
    attr("allow(…)", Some("allow"), Some("allow(${0:lint})")),
    attr("automatically_derived", None, None),
    attr("cfg(…)", Some("cfg"), Some("cfg(${0:predicate})")),
    attr("cfg_attr(…)", Some("cfg_attr"), Some("cfg_attr(${1:predicate}, ${0:attr})")),
    attr("cold", None, None),
    attr(r#"crate_name = """#, Some("crate_name"), Some(r#"crate_name = "${0:crate_name}""#))
        .prefer_inner(),
    attr("deny(…)", Some("deny"), Some("deny(${0:lint})")),
    attr(r#"deprecated"#, Some("deprecated"), Some(r#"deprecated"#)),
    attr("derive(…)", Some("derive"), Some(r#"derive(${0:Debug})"#)),
    attr(r#"doc = "…""#, Some("doc"), Some(r#"doc = "${0:docs}""#)),
    attr(r#"doc(alias = "…")"#, Some("docalias"), Some(r#"doc(alias = "${0:docs}")"#)),
    attr(r#"doc(hidden)"#, Some("dochidden"), Some(r#"doc(hidden)"#)),
    attr(
        r#"export_name = "…""#,
        Some("export_name"),
        Some(r#"export_name = "${0:exported_symbol_name}""#),
    ),
    attr("feature(…)", Some("feature"), Some("feature(${0:flag})")).prefer_inner(),
    attr("forbid(…)", Some("forbid"), Some("forbid(${0:lint})")),
    // FIXME: resolve through macro resolution?
    attr("global_allocator", None, None).prefer_inner(),
    attr(r#"ignore = "…""#, Some("ignore"), Some(r#"ignore = "${0:reason}""#)),
    attr("inline", Some("inline"), Some("inline")),
    attr("link", None, None),
    attr(r#"link_name = "…""#, Some("link_name"), Some(r#"link_name = "${0:symbol_name}""#)),
    attr(
        r#"link_section = "…""#,
        Some("link_section"),
        Some(r#"link_section = "${0:section_name}""#),
    ),
    attr("macro_export", None, None),
    attr("macro_use", None, None),
    attr(r#"must_use"#, Some("must_use"), Some(r#"must_use"#)),
    attr("no_implicit_prelude", None, None).prefer_inner(),
    attr("no_link", None, None).prefer_inner(),
    attr("no_main", None, None).prefer_inner(),
    attr("no_mangle", None, None),
    attr("no_std", None, None).prefer_inner(),
    attr("non_exhaustive", None, None),
    attr("panic_handler", None, None).prefer_inner(),
    attr(r#"path = "…""#, Some("path"), Some(r#"path ="${0:path}""#)),
    attr("proc_macro", None, None),
    attr("proc_macro_attribute", None, None),
    attr("proc_macro_derive(…)", Some("proc_macro_derive"), Some("proc_macro_derive(${0:Trait})")),
    attr("recursion_limit = …", Some("recursion_limit"), Some("recursion_limit = ${0:128}"))
        .prefer_inner(),
    attr("repr(…)", Some("repr"), Some("repr(${0:C})")),
    attr("should_panic", Some("should_panic"), Some(r#"should_panic"#)),
    attr(
        r#"target_feature = "…""#,
        Some("target_feature"),
        Some(r#"target_feature = "${0:feature}""#),
    ),
    attr("test", None, None),
    attr("track_caller", None, None),
    attr("type_length_limit = …", Some("type_length_limit"), Some("type_length_limit = ${0:128}"))
        .prefer_inner(),
    attr("used", None, None),
    attr("warn(…)", Some("warn"), Some("warn(${0:lint})")),
    attr(
        r#"windows_subsystem = "…""#,
        Some("windows_subsystem"),
        Some(r#"windows_subsystem = "${0:subsystem}""#),
    )
    .prefer_inner(),
];

#[test]
fn attributes_are_sorted() {
    let mut attrs = ATTRIBUTES.iter().map(|attr| attr.key());
    let mut prev = attrs.next().unwrap();

    attrs.for_each(|next| {
        assert!(
            prev < next,
            r#"Attributes are not sorted, "{}" should come after "{}""#,
            prev,
            next
        );
        prev = next;
    });
}

fn parse_comma_sep_input(derive_input: ast::TokenTree) -> Result<FxHashSet<String>, ()> {
    match (derive_input.left_delimiter_token(), derive_input.right_delimiter_token()) {
        (Some(left_paren), Some(right_paren))
            if left_paren.kind() == T!['('] && right_paren.kind() == T![')'] =>
        {
            let mut input_derives = FxHashSet::default();
            let mut current_derive = String::new();
            for token in derive_input
                .syntax()
                .children_with_tokens()
                .filter_map(|token| token.into_token())
                .skip_while(|token| token != &left_paren)
                .skip(1)
                .take_while(|token| token != &right_paren)
            {
                if T![,] == token.kind() {
                    if !current_derive.is_empty() {
                        input_derives.insert(current_derive);
                        current_derive = String::new();
                    }
                } else {
                    current_derive.push_str(token.text().trim());
                }
            }

            if !current_derive.is_empty() {
                input_derives.insert(current_derive);
            }
            Ok(input_derives)
        }
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use expect_test::{expect, Expect};

    use crate::{test_utils::completion_list, CompletionKind};

    fn check(ra_fixture: &str, expect: Expect) {
        let actual = completion_list(ra_fixture, CompletionKind::Attribute);
        expect.assert_eq(&actual);
    }

    #[test]
    fn complete_attribute_on_source_file() {
        check(
            r#"#![$0]"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at crate_name = ""
                at feature(…)
                at no_implicit_prelude
                at no_main
                at no_std
                at recursion_limit = …
                at type_length_limit = …
                at windows_subsystem = "…"
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_module() {
        check(
            r#"#[$0] mod foo;"#,
            expect![[r#"
            at allow(…)
            at cfg(…)
            at cfg_attr(…)
            at deny(…)
            at forbid(…)
            at warn(…)
            at deprecated
            at doc = "…"
            at doc(hidden)
            at doc(alias = "…")
            at must_use
            at no_mangle
            at path = "…"
        "#]],
        );
        check(
            r#"mod foo {#![$0]}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at no_implicit_prelude
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_macro_rules() {
        check(
            r#"#[$0] macro_rules! foo {}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at macro_export
                at macro_use
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_macro_def() {
        check(
            r#"#[$0] macro foo {}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_extern_crate() {
        check(
            r#"#[$0] extern crate foo;"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at macro_use
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_use() {
        check(
            r#"#[$0] use foo;"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_type_alias() {
        check(
            r#"#[$0] type foo = ();"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_struct() {
        check(
            r#"#[$0] struct Foo;"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at derive(…)
                at repr(…)
                at non_exhaustive
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_enum() {
        check(
            r#"#[$0] enum Foo {}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at derive(…)
                at repr(…)
                at non_exhaustive
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_const() {
        check(
            r#"#[$0] const FOO: () = ();"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_static() {
        check(
            r#"#[$0] static FOO: () = ()"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at export_name = "…"
                at link_name = "…"
                at link_section = "…"
                at used
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_trait() {
        check(
            r#"#[$0] trait Foo {}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at must_use
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_impl() {
        check(
            r#"#[$0] impl () {}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at automatically_derived
            "#]],
        );
        check(
            r#"impl () {#![$0]}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_extern_block() {
        check(
            r#"#[$0] extern {}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at link
            "#]],
        );
        check(
            r#"extern {#![$0]}"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at deprecated
                at doc = "…"
                at doc(hidden)
                at doc(alias = "…")
                at must_use
                at no_mangle
                at link
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_variant() {
        check(
            r#"enum Foo { #[$0] Bar }"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
                at non_exhaustive
            "#]],
        );
    }

    #[test]
    fn complete_attribute_on_expr() {
        check(
            r#"fn main() { #[$0] foo() }"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
            "#]],
        );
        check(
            r#"fn main() { #[$0] foo(); }"#,
            expect![[r#"
                at allow(…)
                at cfg(…)
                at cfg_attr(…)
                at deny(…)
                at forbid(…)
                at warn(…)
            "#]],
        );
    }

    #[test]
    fn test_attribute_completion_inside_nested_attr() {
        check(r#"#[cfg($0)]"#, expect![[]])
    }
}
