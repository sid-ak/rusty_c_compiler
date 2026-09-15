//! Unit tests for the scope stack: nesting, shadowing, redeclaration, and slot numbering.
//!
//! These pin the structure rather than the diagnostics. That a `for`-init variable used after the
//! loop produces an undeclared-identifier message is a test on the analyzer; that the binding has
//! gone out of scope by then is a test on this stack, and it is the one here.

use super::*;

/// A span standing in for wherever a declaration would have come from.
///
/// Distinct values matter only where a test asserts which declaration was found, so those tests
/// build their own spans rather than using this.
const ANYWHERE: Span = Span { start: 0, end: 1 };

/// Declares `name` as a local `int` and returns the symbol id, failing the test on a conflict.
fn declare_local(scopes: &mut Scopes, name: &str) -> SymbolId {
    scopes
        .declare(name, Ty::Int, SymbolKind::Local, ANYWHERE)
        .expect("name was not already declared in this scope")
}

/// The type of the symbol `name` resolves to, or `None` if it resolves to nothing.
fn lookup_type(scopes: &Scopes, name: &str) -> Option<Ty> {
    let id = scopes.lookup(name)?;

    scopes.symbol(id).map(|symbol| symbol.ty.clone())
}

/// A fresh stack is at file scope, which is depth zero.
#[test]
fn a_fresh_stack_is_at_file_scope() {
    let scopes = Scopes::new();

    assert_eq!(scopes.depth(), 0);
}

/// Entering and leaving blocks moves the depth up and back down.
#[test]
fn entering_and_leaving_blocks_tracks_depth() {
    let mut scopes = Scopes::new();

    scopes.enter_function();
    assert_eq!(scopes.depth(), 1);

    scopes.enter_block();
    assert_eq!(scopes.depth(), 2);

    scopes.leave_block();
    assert_eq!(scopes.depth(), 1);

    scopes.leave_function();
    assert_eq!(scopes.depth(), 0);
}

/// Leaving file scope is refused rather than underflowing the stack.
///
/// Nothing in a well-formed walk does this. The stack is a public entry point all the same, and
/// the no-panic invariant does not admit an exception for a caller that should have known better.
#[test]
fn leaving_file_scope_is_refused_rather_than_panicking() {
    let mut scopes = Scopes::new();

    scopes.leave_block();
    scopes.leave_function();

    assert_eq!(scopes.depth(), 0);

    declare_local(&mut scopes, "still_usable");
    assert_eq!(lookup_type(&scopes, "still_usable"), Some(Ty::Int));
}

/// A name declared in an inner block hides the outer one, which returns when the block ends.
#[test]
fn an_inner_declaration_shadows_an_outer_one_until_the_block_ends() {
    let mut scopes = Scopes::new();
    scopes
        .declare("value", Ty::Int, SymbolKind::Global, ANYWHERE)
        .expect("the global is the first declaration of this name");

    scopes.enter_function();
    scopes
        .declare("value", Ty::Char, SymbolKind::Local, ANYWHERE)
        .expect("a local may shadow a global");

    assert_eq!(lookup_type(&scopes, "value"), Some(Ty::Char));

    scopes.leave_function();

    assert_eq!(lookup_type(&scopes, "value"), Some(Ty::Int));
}

/// A global is shadowable by a local, and the global is what resolves outside the function.
#[test]
fn a_global_is_shadowable_by_a_local() {
    let mut scopes = Scopes::new();
    scopes
        .declare("count", Ty::Int, SymbolKind::Global, ANYWHERE)
        .expect("the global is the first declaration of this name");

    scopes.enter_function();
    scopes
        .declare("count", Ty::array(Ty::Char, 4), SymbolKind::Local, ANYWHERE)
        .expect("a local may shadow a global");

    assert_eq!(lookup_type(&scopes, "count"), Some(Ty::array(Ty::Char, 4)));

    scopes.leave_function();

    assert_eq!(lookup_type(&scopes, "count"), Some(Ty::Int));
}

/// A parameter shares the function body's scope, so a body local cannot redeclare it.
///
/// This is C's rule, confirmed against the oracle: `int f(int a) { int a; }` is a redefinition
/// under `clang -std=c99`. Accepting it would make this compiler more permissive than clang,
/// which is the one direction the differential suite cannot catch.
#[test]
fn a_body_local_cannot_redeclare_a_parameter() {
    let mut scopes = Scopes::new();
    scopes.enter_function();
    scopes
        .declare("a", Ty::Int, SymbolKind::Parameter(0), ANYWHERE)
        .expect("the parameter is the first declaration of this name");

    let conflict = scopes.declare("a", Ty::Int, SymbolKind::Local, ANYWHERE);

    assert!(conflict.is_err(), "a body local may not redeclare `a`");
}

/// A local in a nested block may shadow a parameter, and the parameter returns afterwards.
#[test]
fn a_nested_block_local_may_shadow_a_parameter() {
    let mut scopes = Scopes::new();
    scopes.enter_function();
    scopes
        .declare("a", Ty::Int, SymbolKind::Parameter(0), ANYWHERE)
        .expect("the parameter is the first declaration of this name");

    scopes.enter_block();
    scopes
        .declare("a", Ty::Char, SymbolKind::Local, ANYWHERE)
        .expect("a nested block may shadow a parameter");

    assert_eq!(lookup_type(&scopes, "a"), Some(Ty::Char));

    scopes.leave_block();

    assert_eq!(lookup_type(&scopes, "a"), Some(Ty::Int));
}

/// Redeclaring a name in one scope fails and hands back the declaration already there.
///
/// The returned symbol is what lets the analyzer point at both the new and the original site; the
/// rendered message is the analyzer's test to write.
#[test]
fn same_scope_redeclaration_reports_the_original_declaration() {
    let original = Span { start: 4, end: 9 };
    let duplicate = Span { start: 20, end: 25 };

    let mut scopes = Scopes::new();
    scopes.enter_function();
    scopes
        .declare("total", Ty::Int, SymbolKind::Local, original)
        .expect("the first declaration succeeds");

    let Err(conflict) = scopes.declare("total", Ty::Char, SymbolKind::Local, duplicate) else {
        panic!("a second declaration of `total` in one scope must fail");
    };

    let previous = scopes
        .symbol(conflict)
        .expect("the conflict names a symbol");
    assert_eq!(previous.span, original);
    assert_eq!(previous.ty, Ty::Int);
}

/// A failed redeclaration leaves the original binding in place rather than half-replacing it.
#[test]
fn a_failed_redeclaration_leaves_the_original_binding_intact() {
    let mut scopes = Scopes::new();
    scopes.enter_function();
    declare_local(&mut scopes, "total");

    let _ = scopes.declare("total", Ty::Char, SymbolKind::Local, ANYWHERE);

    assert_eq!(lookup_type(&scopes, "total"), Some(Ty::Int));
}

/// The same name in two sibling blocks is two separate declarations, not a redeclaration.
#[test]
fn sibling_blocks_may_each_declare_the_same_name() {
    let mut scopes = Scopes::new();
    scopes.enter_function();

    scopes.enter_block();
    declare_local(&mut scopes, "i");
    scopes.leave_block();

    scopes.enter_block();
    scopes
        .declare("i", Ty::Char, SymbolKind::Local, ANYWHERE)
        .expect("a sibling block is a different scope");
    scopes.leave_block();
}

/// A `for`-init variable is visible in the loop body and gone once the loop ends.
///
/// The init clause gets its own scope that encloses the body, which is the whole reason
/// `for (int i = 0; ...)` leaves no `i` behind.
#[test]
fn a_for_init_variable_is_visible_in_the_body_and_not_after_the_loop() {
    let mut scopes = Scopes::new();
    scopes.enter_function();

    // The init clause's scope.
    scopes.enter_block();
    declare_local(&mut scopes, "i");

    // The body nests inside it.
    scopes.enter_block();
    assert_eq!(lookup_type(&scopes, "i"), Some(Ty::Int));
    scopes.leave_block();

    scopes.leave_block();

    assert_eq!(scopes.lookup("i"), None);
}

/// A local shadows a function of the same name, so the name stops being callable in that scope.
///
/// Functions sit in file scope beside globals and are shadowed by the ordinary rule rather than a
/// rule of their own. Whether calling the shadowed name is then an error is the analyzer's
/// question; that it no longer resolves to the function is this one.
#[test]
fn a_local_shadows_a_function_of_the_same_name() {
    let mut scopes = Scopes::new();
    scopes
        .declare(
            "helper",
            Ty::func(Ty::Int, vec![]),
            SymbolKind::Function,
            ANYWHERE,
        )
        .expect("the function is the first declaration of this name");

    scopes.enter_function();
    declare_local(&mut scopes, "helper");

    let id = scopes.lookup("helper").expect("the name still resolves");
    let symbol = scopes.symbol(id).expect("the id names a symbol");
    assert_eq!(symbol.kind, SymbolKind::Local);

    scopes.leave_function();

    let id = scopes.lookup("helper").expect("the function is back");
    let symbol = scopes.symbol(id).expect("the id names a symbol");
    assert_eq!(symbol.kind, SymbolKind::Function);
}

/// Lookup finds nothing for a name that was never declared.
#[test]
fn an_undeclared_name_resolves_to_nothing() {
    let scopes = Scopes::new();

    assert_eq!(scopes.lookup("nothing"), None);
}

/// Locals and parameters get consecutive slot ids; globals and functions get none.
#[test]
fn slots_are_handed_to_locals_and_parameters_only() {
    let mut scopes = Scopes::new();
    let global = scopes
        .declare("g", Ty::Int, SymbolKind::Global, ANYWHERE)
        .expect("the global is the first declaration of this name");
    let function = scopes
        .declare(
            "f",
            Ty::func(Ty::Int, vec![]),
            SymbolKind::Function,
            ANYWHERE,
        )
        .expect("the function is the first declaration of this name");

    scopes.enter_function();
    let parameter = scopes
        .declare("p", Ty::Int, SymbolKind::Parameter(0), ANYWHERE)
        .expect("the parameter is the first declaration of this name");
    let local = declare_local(&mut scopes, "l");

    let slot_of = |id: SymbolId| scopes.symbol(id).and_then(|symbol| symbol.slot);

    assert_eq!(slot_of(global), None);
    assert_eq!(slot_of(function), None);
    assert_eq!(slot_of(parameter), Some(SlotId(0)));
    assert_eq!(slot_of(local), Some(SlotId(1)));
}

/// Slot numbering restarts at each function, since slots index that function's frame.
#[test]
fn slot_numbering_restarts_for_each_function() {
    let mut scopes = Scopes::new();

    scopes.enter_function();
    let first = declare_local(&mut scopes, "a");
    scopes.leave_function();

    scopes.enter_function();
    let second = declare_local(&mut scopes, "a");
    scopes.leave_function();

    let slot_of = |id: SymbolId| scopes.symbol(id).and_then(|symbol| symbol.slot);

    assert_eq!(slot_of(first), Some(SlotId(0)));
    assert_eq!(slot_of(second), Some(SlotId(0)));
}

/// Slots keep counting up across the blocks of one function, since one frame holds them all.
#[test]
fn slots_keep_counting_across_blocks_within_a_function() {
    let mut scopes = Scopes::new();
    scopes.enter_function();

    let outer = declare_local(&mut scopes, "a");

    scopes.enter_block();
    let inner = declare_local(&mut scopes, "b");
    scopes.leave_block();

    let after = declare_local(&mut scopes, "c");

    let slot_of = |id: SymbolId| scopes.symbol(id).and_then(|symbol| symbol.slot);

    assert_eq!(slot_of(outer), Some(SlotId(0)));
    assert_eq!(slot_of(inner), Some(SlotId(1)));
    assert_eq!(slot_of(after), Some(SlotId(2)));
}

/// A symbol that went out of scope is still readable by id, which is what the annotations rely on.
///
/// Bindings recorded during the walk are read back long after their scope closed, so the symbol
/// table outlives the stack that shaped it.
#[test]
fn a_symbol_outlives_the_scope_it_was_declared_in() {
    let mut scopes = Scopes::new();
    scopes.enter_function();
    let local = declare_local(&mut scopes, "gone");
    scopes.leave_function();

    assert_eq!(scopes.lookup("gone"), None);

    let symbol = scopes.symbol(local).expect("the id still names a symbol");
    assert_eq!(symbol.name, "gone");
    assert_eq!(symbol.ty, Ty::Int);
}

/// An id from one stack does not name a symbol in another.
#[test]
fn an_unknown_symbol_id_resolves_to_nothing() {
    let mut other = Scopes::new();
    let id = other
        .declare("elsewhere", Ty::Int, SymbolKind::Global, ANYWHERE)
        .expect("the global is the first declaration of this name");

    let scopes = Scopes::new();

    assert_eq!(scopes.symbol(id), None);
}
