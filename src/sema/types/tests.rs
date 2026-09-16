//! Unit tests for the type model: layout, promotion, decay, and compatibility.
//!
//! The rules under test are matrices rather than single cases, so most tests here enumerate every
//! cell of a matrix rather than sampling it. [`sample_types`] is the row and column index for all
//! of them, and [`variant_name`] makes it a build failure to add a [`Ty`] variant without adding a
//! sample for it — an omitted row would otherwise pass every matrix test silently.

use super::*;

/// One value of every [`Ty`] variant, used as the axis of the matrix tests below.
///
/// `Ptr` and `Array` appear twice with different element types, because the rules that treat them
/// as compatible or not turn on the element type rather than on the variant.
fn sample_types() -> Vec<Ty> {
    vec![
        Ty::Int,
        Ty::Char,
        Ty::Void,
        Ty::Error,
        Ty::array(Ty::Int, 10),
        Ty::array(Ty::Char, 3),
        Ty::ptr(Ty::Int),
        Ty::ptr(Ty::Char),
        Ty::func(Ty::Int, vec![Ty::Int]),
    ]
}

/// The variant `ty` is, named so a matrix test can report which cell failed.
///
/// The `match` is exhaustive on purpose: adding a variant to [`Ty`] fails to compile here, which
/// is the reminder to extend [`sample_types`] so the matrices below keep covering every variant.
fn variant_name(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int => "Int",
        Ty::Char => "Char",
        Ty::Void => "Void",
        Ty::Error => "Error",
        Ty::Array(_, _) => "Array",
        Ty::Ptr(_) => "Ptr",
        Ty::Func { .. } => "Func",
    }
}

/// Every `Ty` variant is represented in the sample list the matrix tests run over.
#[test]
fn sample_types_cover_every_variant() {
    let mut covered: Vec<&'static str> = sample_types().iter().map(variant_name).collect();
    covered.sort_unstable();
    covered.dedup();

    assert_eq!(
        covered,
        vec!["Array", "Char", "Error", "Func", "Int", "Ptr", "Void"]
    );
}

/// `char` occupies one byte and `int` four, each aligned to its own size.
#[test]
fn scalar_layouts_match_the_arm64_c_abi() {
    assert_eq!(Ty::Char.layout(), Some(Layout { size: 1, align: 1 }));
    assert_eq!(Ty::Int.layout(), Some(Layout { size: 4, align: 4 }));
}

/// A pointer is eight bytes on ARM64 whatever it points at.
#[test]
fn pointer_layout_is_eight_bytes_regardless_of_pointee() {
    let eight = Some(Layout { size: 8, align: 8 });

    assert_eq!(Ty::ptr(Ty::Int).layout(), eight);
    assert_eq!(Ty::ptr(Ty::Char).layout(), eight);
}

/// An array is its length times its element size, aligned like its element.
#[test]
fn array_layout_is_element_size_times_length() {
    assert_eq!(
        Ty::array(Ty::Int, 10).layout(),
        Some(Layout { size: 40, align: 4 })
    );
    assert_eq!(
        Ty::array(Ty::Char, 3).layout(),
        Some(Layout { size: 3, align: 1 })
    );
}

/// A nested array multiplies its dimensions and keeps the innermost element's alignment.
///
/// The parser rejects `int a[2][3]`, so no source program reaches this; the type model still has
/// to answer correctly, because nothing in it depends on the parser's restriction holding.
#[test]
fn nested_array_layout_multiplies_the_dimensions() {
    let nested = Ty::array(Ty::array(Ty::Int, 3), 2);

    assert_eq!(nested.layout(), Some(Layout { size: 24, align: 4 }));
}

/// An array long enough to overflow 32-bit arithmetic still reports a size rather than wrapping.
#[test]
fn huge_array_layout_does_not_wrap() {
    let huge = Ty::array(Ty::Int, u32::MAX);

    assert_eq!(
        huge.layout(),
        Some(Layout {
            size: u64::from(u32::MAX) * 4,
            align: 4,
        })
    );
}

/// An array too large to measure reports no layout rather than overflowing.
///
/// The parser cannot build this type, since it rejects multi-dimensional arrays. `layout` is a
/// public entry point all the same, so it answers rather than panicking.
#[test]
fn unmeasurable_array_layout_reports_none_rather_than_overflowing() {
    let unmeasurable = Ty::array(Ty::array(Ty::array(Ty::Int, u32::MAX), u32::MAX), u32::MAX);

    assert_eq!(unmeasurable.layout(), None);
}

/// `void` and function types have no storage, so they have no layout.
#[test]
fn incomplete_types_have_no_layout() {
    assert_eq!(Ty::Void.layout(), None);
    assert_eq!(Ty::Error.layout(), None);
    assert_eq!(Ty::func(Ty::Int, vec![]).layout(), None);
    assert_eq!(Ty::array(Ty::Void, 4).layout(), None);
}

/// Promotion widens `char` to `int` and leaves every other type alone.
#[test]
fn promotion_widens_char_and_nothing_else() {
    for ty in sample_types() {
        let expected = if ty == Ty::Char { Ty::Int } else { ty.clone() };

        assert_eq!(
            ty.promoted(),
            expected,
            "promotion of {} ({})",
            ty,
            variant_name(&ty)
        );
    }
}

/// `promotes` agrees with `promoted` on every type, so the predicate cannot drift from the rule.
#[test]
fn promotes_predicate_agrees_with_the_promotion_rule() {
    for ty in sample_types() {
        assert_eq!(
            ty.promotes(),
            ty.promoted() != ty,
            "promotes() for {} ({})",
            ty,
            variant_name(&ty)
        );
    }
}

/// Decay turns an array into a pointer to its element and leaves every other type alone.
#[test]
fn decay_rewrites_arrays_to_pointers_and_nothing_else() {
    for ty in sample_types() {
        let expected = match &ty {
            Ty::Array(element, _) => Ty::Ptr(element.clone()),
            other => other.clone(),
        };

        assert_eq!(
            ty.decayed(),
            expected,
            "decay of {} ({})",
            ty,
            variant_name(&ty)
        );
    }
}

/// `decays` agrees with `decayed` on every type, so the predicate cannot drift from the rule.
#[test]
fn decays_predicate_agrees_with_the_decay_rule() {
    for ty in sample_types() {
        assert_eq!(
            ty.decays(),
            ty.decayed() != ty,
            "decays() for {} ({})",
            ty,
            variant_name(&ty)
        );
    }
}

/// Only `int` and `char` are arithmetic; only they plus pointers are scalar.
///
/// Condition contexts accept exactly the scalar types, which is why there is no separate
/// predicate for them to disagree with this one.
#[test]
fn arithmetic_and_scalar_classify_every_type() {
    for ty in sample_types() {
        // The recovery type satisfies both, so a value already reported wrong collects no
        // second complaint from the operator it flows into.
        let arithmetic = matches!(ty, Ty::Int | Ty::Char | Ty::Error);
        let scalar = matches!(ty, Ty::Int | Ty::Char | Ty::Ptr(_) | Ty::Error);

        assert_eq!(
            ty.is_arithmetic(),
            arithmetic,
            "is_arithmetic for {} ({})",
            ty,
            variant_name(&ty)
        );
        assert_eq!(
            ty.is_scalar(),
            scalar,
            "is_scalar for {} ({})",
            ty,
            variant_name(&ty)
        );
    }
}

/// Two arithmetic operands share `int`; any other pairing has no common type.
///
/// The array cells are the ones that matter: `a + 1` must not quietly decay into pointer
/// arithmetic, which is the restriction ADR 0007 exists to state.
#[test]
fn common_arithmetic_type_is_int_for_arithmetic_pairs_only() {
    for left in sample_types() {
        for right in sample_types() {
            let expected = if left.is_error() || right.is_error() {
                Some(Ty::Error)
            } else if left.is_arithmetic() && right.is_arithmetic() {
                Some(Ty::Int)
            } else {
                None
            };

            assert_eq!(
                Ty::common_arithmetic(&left, &right),
                expected,
                "common type of {left} and {right}"
            );
        }
    }
}

/// The expected assignability of `source` to `target`, stated independently of the implementation.
///
/// Written as a table over the pair rather than delegating to the code under test, so a rule that
/// changes in the implementation shows up here as a failure rather than agreeing with itself.
fn expected_assignability(target: &Ty, source: &Ty) -> Assignability {
    if target.is_error() || source.is_error() {
        return Assignability::Exact;
    }

    match (target, source) {
        (Ty::Int, Ty::Int) | (Ty::Char, Ty::Char) => Assignability::Exact,
        (Ty::Int, Ty::Char) => Assignability::Converted(Conversion::PromoteCharToInt),
        (Ty::Char, Ty::Int) => Assignability::Converted(Conversion::TruncateIntToChar),
        (Ty::Ptr(target_element), Ty::Ptr(source_element)) if target_element == source_element => {
            Assignability::Exact
        }
        (Ty::Ptr(target_element), Ty::Array(source_element, _))
            if target_element == source_element =>
        {
            Assignability::Converted(Conversion::DecayArrayToPtr)
        }
        _ => Assignability::Incompatible,
    }
}

/// Every cell of the assignment-compatibility matrix matches the documented rules, both directions.
#[test]
fn assignability_matrix_matches_the_documented_rules() {
    for target in sample_types() {
        for source in sample_types() {
            assert_eq!(
                Ty::assignability(&target, &source),
                expected_assignability(&target, &source),
                "assigning {source} to {target}"
            );
        }
    }
}

/// `int` and `char` convert to each other, widening one way and truncating the other.
#[test]
fn int_and_char_interconvert_with_truncation_on_the_narrowing_side() {
    assert_eq!(
        Ty::assignability(&Ty::Int, &Ty::Char),
        Assignability::Converted(Conversion::PromoteCharToInt)
    );
    assert_eq!(
        Ty::assignability(&Ty::Char, &Ty::Int),
        Assignability::Converted(Conversion::TruncateIntToChar)
    );
}

/// A pointer accepts only a pointer to the same element type.
#[test]
fn pointers_accept_only_a_matching_pointee() {
    assert_eq!(
        Ty::assignability(&Ty::ptr(Ty::Int), &Ty::ptr(Ty::Int)),
        Assignability::Exact
    );
    assert_eq!(
        Ty::assignability(&Ty::ptr(Ty::Int), &Ty::ptr(Ty::Char)),
        Assignability::Incompatible
    );
    assert_eq!(
        Ty::assignability(&Ty::ptr(Ty::Int), &Ty::Int),
        Assignability::Incompatible
    );
}

/// Arrays, functions, and `void` are never assignable to, whatever the source type is.
#[test]
fn arrays_functions_and_void_are_never_assignable_targets() {
    let targets = [
        Ty::Void,
        Ty::array(Ty::Int, 10),
        Ty::func(Ty::Int, vec![Ty::Int]),
    ];

    for target in targets {
        for source in sample_types().into_iter().filter(|ty| !ty.is_error()) {
            assert_eq!(
                Ty::assignability(&target, &source),
                Assignability::Incompatible,
                "assigning {source} to {target}"
            );
        }
    }
}

/// An array is assignable to a matching pointer, which is the parameter-passing rule and only that.
///
/// The conversion is reported so the caller records it; nothing about this makes `a` a pointer
/// anywhere other than where the caller chose to ask.
#[test]
fn an_array_is_assignable_to_a_pointer_to_its_element() {
    assert_eq!(
        Ty::assignability(&Ty::ptr(Ty::Int), &Ty::array(Ty::Int, 10)),
        Assignability::Converted(Conversion::DecayArrayToPtr)
    );
    assert_eq!(
        Ty::assignability(&Ty::ptr(Ty::Char), &Ty::array(Ty::Int, 10)),
        Assignability::Incompatible
    );
}

/// Types print the way C spells them, so a diagnostic can quote one directly.
#[test]
fn types_print_as_c_spells_them() {
    assert_eq!(Ty::Int.to_string(), "int");
    assert_eq!(Ty::Char.to_string(), "char");
    assert_eq!(Ty::Void.to_string(), "void");
    assert_eq!(Ty::Error.to_string(), "<error>");
    assert_eq!(Ty::ptr(Ty::Int).to_string(), "int *");
    assert_eq!(Ty::array(Ty::Char, 3).to_string(), "char[3]");
    assert_eq!(
        Ty::func(Ty::Void, vec![Ty::Int, Ty::ptr(Ty::Char)]).to_string(),
        "void(int, char *)"
    );
    assert_eq!(Ty::func(Ty::Int, vec![]).to_string(), "int(void)");
}

/// A nested array prints its dimensions outermost first, the order C declares them in.
#[test]
fn nested_array_prints_its_dimensions_in_declaration_order() {
    let nested = Ty::array(Ty::array(Ty::Int, 3), 2);

    assert_eq!(nested.to_string(), "int[2][3]");
}
