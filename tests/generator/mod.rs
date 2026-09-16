//! A seeded generator of well-typed subset-C programs that stay inside defined behavior.
//!
//! A hand-written corpus covers the combinations someone thought to write down. This covers the
//! ones nobody did — an expression tree whose shape no person would choose, a call whose eighth
//! argument is itself a call, an array subscript computed three operators deep. Those are where the
//! code generation bugs that survive a feature checklist live.
//!
//! The hard part is not producing C, it is producing C with a right answer. `clang` is only an
//! oracle for programs whose behavior the standard defines, so the generator may never emit a
//! division by zero, an arithmetic overflow, a subscript off the end of an array, a read of
//! something never written, or two modifications of one object with no sequence point between them.
//! It rules those out by construction rather than by filtering afterwards:
//!
//! - Every expression is built together with an interval of the values it can take, computed in
//!   64-bit arithmetic. An operator is emitted only if its result interval fits in a 32-bit `int`,
//!   so overflow cannot happen rather than being unlikely.
//! - A divisor is only ever an expression whose interval excludes zero — in practice a positive
//!   literal, which also keeps `INT_MIN / -1` out of reach.
//! - Every variable carries the invariant that it holds a value within [`VALUE_BOUND`]. An
//!   assignment whose interval does not fit is wrapped in a remainder by a positive literal, which
//!   brings any `int` into range without a branch.
//! - Subscripts are literals inside the array, or a `for` counter whose bound is the array's length.
//! - Every variable and every array element is initialized where it is declared.
//! - No generated expression assigns to anything, so there is nothing to sequence.

// This module is compiled into every test binary that declares it, and the binary that runs the
// generated programs does not call every builder here directly.
#![allow(dead_code)]
// Generator code is test code: it builds strings and never touches user input. Indexing included —
// every index here is either a constant into a fixed-size table of operator spellings or a value
// the random number generator was asked to keep below the length, and writing each one as a `get`
// and a fallback would put an unreachable branch beside every choice the generator makes.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use std::fmt::Write as _;

/// The range every generated variable, parameter, and function result is held to.
///
/// Small enough that a product of two of them is nowhere near the edge of an `int`, which is what
/// keeps the interval arithmetic below from having to refuse most of what it is asked to build.
const VALUE_BOUND: i64 = 1000;

/// The largest magnitude any generated expression is allowed to reach.
///
/// Below `i32::MAX` on purpose: an expression at the very edge leaves no room for the operator
/// above it, so the generator would build a tree and then throw it away.
const EXPRESSION_BOUND: i64 = 1 << 29;

/// How many elements every generated array has.
const ARRAY_LENGTH: usize = 8;

/// Roughly how many statement executions one generated program may cost.
///
/// The value intervals above keep a program's *answer* defined; this keeps its *runtime* finite.
/// Nothing rules out a loop inside a loop inside a function called from a loop, and the first
/// version of this generator wrote exactly that: three of two and a half thousand programs ran past
/// the harness's ten-second limit, one of them finishing under `clang` and not under `rustycc` —
/// which is an honest difference between the two compilers and no use at all as a test, since a
/// program that does not finish has no output to compare.
///
/// So the generator carries a running estimate of how much work it has asked for, computed the same
/// way the value intervals are: each statement costs the product of the loop bounds around it, and
/// a call costs whatever the callee was estimated at. Past the budget it stops offering loops and
/// calls and writes plain statements instead.
const WORK_BUDGET: u64 = 200_000;

/// How deeply a call's arguments may themselves contain calls.
///
/// Nested calls are worth generating — placing an argument register and then evaluating the next
/// argument is where a call-lowering bug lives — but a call's arguments are generated from the same
/// expression grammar the call was a leaf of, so without a limit the generator recurses forever.
const MAX_CALL_DEPTH: usize = 2;

/// How deeply statements may nest inside one another.
///
/// A statement that contains a block — an `if`, either loop — generates that block from the same
/// list of statements it came from, so without a limit the generator recurses until the stack runs
/// out. Three levels is deeper than most hand-written code and shallow enough to read.
const MAX_BLOCK_DEPTH: usize = 3;

/// The global every computation inside a loop or a function folds itself into.
///
/// Printing from inside a loop that is inside a loop that is inside a function called from a loop
/// produces megabytes, and a mismatch report nobody can read is most of the way to no report. So
/// only the top level of `main` prints; everywhere else the value is folded into this one variable,
/// which `main` prints at the end. The fold is order-sensitive, so a wrong value anywhere still
/// changes it — it is a digest of the whole run rather than a sample of it.
const SINK: &str = "sink";

/// The modulus that brings an out-of-range value back inside [`VALUE_BOUND`].
///
/// Prime and positive: positive so the sign of the result follows the left operand rather than
/// the divisor, and below the bound so the result is always in range.
const CLAMP_MODULUS: i64 = 997;

/// A small deterministic random number generator.
///
/// SplitMix64, which is short enough to read and good enough to shuffle a generator's choices. It
/// is here rather than pulled in as a dependency because the only property needed is that a seed
/// reproduces a run exactly, and that is easier to guarantee for eight lines than for a crate whose
/// version could change underneath a checked-in failing seed.
pub struct Rng {
    /// The state, advanced by a fixed odd increment on every draw.
    state: u64,
}

impl Rng {
    /// A generator started from `seed`.
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    /// The next value in the sequence.
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);

        z ^ (z >> 31)
    }

    /// A value in `0..limit`.
    pub fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            return 0;
        }

        usize::try_from(self.next() % limit as u64).unwrap_or(0)
    }

    /// A value in `low..=high`.
    pub fn between(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let span = (high - low + 1) as u64;

        low + i64::try_from(self.next() % span).unwrap_or(0)
    }

    /// True with probability one in `odds`.
    pub fn one_in(&mut self, odds: usize) -> bool {
        self.below(odds.max(1)) == 0
    }

    /// One of `choices`, by index.
    fn choose(&mut self, choices: usize) -> usize {
        self.below(choices)
    }
}

/// A generated expression, together with the values it can take.
///
/// The interval is the whole point: it is carried alongside the text so the generator can refuse to
/// build an operator whose result would not fit, rather than building one and hoping.
#[derive(Clone, Debug)]
pub struct Value {
    /// The C source of the expression, parenthesized where it needs to be.
    pub text: String,
    /// The smallest value it can evaluate to.
    pub low: i64,
    /// The largest value it can evaluate to.
    pub high: i64,
}

impl Value {
    /// An expression whose value is exactly `n`.
    fn literal(n: i64) -> Self {
        Value {
            text: n.to_string(),
            low: n,
            high: n,
        }
    }

    /// Whether every value this expression can take lies within `bound` of zero.
    fn fits(&self, bound: i64) -> bool {
        self.low >= -bound && self.high <= bound
    }

    /// Whether zero is a value this expression can take, which is what disqualifies a divisor.
    fn can_be_zero(&self) -> bool {
        self.low <= 0 && self.high >= 0
    }
}

/// A name in scope, and what is known about it.
#[derive(Clone, Debug)]
struct Variable {
    /// How the variable is written in an expression.
    name: String,
    /// The smallest value it can hold.
    low: i64,
    /// The largest value it can hold.
    high: i64,
    /// Whether the generator may assign to it.
    ///
    /// False for a `for` counter. Assigning to one is legal C, but a generated `i--` in the body of
    /// a loop counting up is a program that never finishes, which a differential harness reports as
    /// a timeout — indistinguishable from the compiler having generated a loop that cannot exit.
    assignable: bool,
}

/// A function the generator has already emitted, and so may call.
#[derive(Clone, Debug)]
struct Function {
    /// Its name.
    name: String,
    /// How many `int` parameters it takes.
    arity: usize,
    /// The estimated cost of one call to it, in statement executions.
    cost: u64,
}

/// The state of one program being built.
pub struct Generator {
    /// The source assembled so far.
    source: String,
    /// The random choices.
    rng: Rng,
    /// Scalars in scope, innermost last.
    scalars: Vec<Variable>,
    /// Arrays in scope, by name. Every one has [`ARRAY_LENGTH`] elements.
    arrays: Vec<String>,
    /// Functions already emitted, which are the only ones that may be called.
    functions: Vec<Function>,
    /// How many names have been handed out, so every one is distinct.
    names: usize,
    /// The current indentation, in levels of four spaces.
    indent: usize,
    /// How many loops the statement being generated is inside.
    ///
    /// `break` and `continue` outside a loop are a semantic error, so they are only offered here.
    loops: usize,
    /// How many blocks deep the statement being generated is.
    blocks: usize,
    /// How many calls the expression being generated is already inside the argument list of.
    calls: usize,
    /// The estimated cost of the function being generated, in statement executions.
    cost: u64,
    /// The product of the bounds of the loops the statement being generated is inside.
    ///
    /// One statement written inside two loops of eight runs sixty-four times, so it is charged
    /// sixty-four rather than one.
    multiplier: u64,
    /// Whether the statement being generated is in `main` rather than in a helper function.
    ///
    /// Together with the loop depth, this is what decides whether a computation may print itself or
    /// has to fold into [`SINK`] instead.
    in_main: bool,
}

/// Generates the whole of one program from `seed`.
///
/// The same seed produces byte-identical source, which is what makes a failure reproducible from
/// the one number a failing test prints.
pub fn program(seed: u64) -> String {
    let mut generator = Generator::new(seed);
    generator.build();

    generator.source
}

impl Generator {
    /// An empty program, ready to be built.
    fn new(seed: u64) -> Self {
        Generator {
            source: String::new(),
            rng: Rng::new(seed),
            scalars: Vec::new(),
            arrays: Vec::new(),
            functions: Vec::new(),
            names: 0,
            indent: 0,
            loops: 0,
            blocks: 0,
            calls: 0,
            cost: 0,
            multiplier: 1,
            in_main: false,
        }
    }

    // -- Assembling the program ------------------------------------------------------------------

    /// Writes the whole program: the shim's prototypes, some globals, some functions, and `main`.
    fn build(&mut self) {
        self.line("// Generated by tests/generator. Do not edit: change the generator instead.");
        self.line("");
        self.line("void print_int(int n);");
        self.line("void print_char(char c);");
        self.line("");

        self.globals();

        let count = self.rng.between(2, 4) as usize;
        for _ in 0..count {
            self.function();
        }

        self.main();
    }

    /// Emits a few globals, so not every name a function reads is one it was handed.
    fn globals(&mut self) {
        self.line(&format!("int {SINK} = 0;"));

        let scalars = self.rng.between(1, 3) as usize;
        for _ in 0..scalars {
            let name = self.fresh("g");
            let value = self.rng.between(-VALUE_BOUND, VALUE_BOUND);
            self.line(&format!("int {name} = {value};"));
            self.scalars.push(Variable {
                name,
                low: -VALUE_BOUND,
                high: VALUE_BOUND,
                assignable: true,
            });
        }

        let name = self.fresh("ga");
        let elements = self.element_list();
        self.line(&format!("int {name}[{ARRAY_LENGTH}] = {{{elements}}};"));
        self.arrays.push(name);
        self.line("");
    }

    /// Emits one function, whose parameters and body are then out of scope again.
    ///
    /// Arities run from none to nine so the boundary at eight — where arguments stop arriving in
    /// registers and start arriving on the stack — is crossed by some of them.
    fn function(&mut self) {
        let name = self.fresh("f");
        let arity = self.rng.below(10);

        let outer_scalars = self.scalars.len();
        let outer_arrays = self.arrays.len();
        self.cost = 0;
        self.multiplier = 1;

        let mut parameters = Vec::new();
        for _ in 0..arity {
            let parameter = self.fresh("p");
            parameters.push(parameter.clone());
            self.scalars.push(Variable {
                name: parameter,
                low: -VALUE_BOUND,
                high: VALUE_BOUND,
                assignable: true,
            });
        }

        let signature = if parameters.is_empty() {
            "void".to_owned()
        } else {
            parameters
                .iter()
                .map(|parameter| format!("int {parameter}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        self.line(&format!("int {name}({signature}) {{"));
        self.indent += 1;

        self.local_array();
        let statements = self.rng.between(2, 5) as usize;
        for _ in 0..statements {
            self.statement();
        }

        let result = self.bounded_expression(VALUE_BOUND);
        self.line(&format!("return {};", result.text));

        self.indent -= 1;
        self.line("}");
        self.line("");

        self.scalars.truncate(outer_scalars);
        self.arrays.truncate(outer_arrays);
        self.functions.push(Function {
            name,
            arity,
            // At least one: a function that does nothing still costs a call, and a cost of zero
            // would let it be called from inside a loop without limit.
            cost: self.cost.max(1),
        });
    }

    /// Emits `main`, which does the same work as any other function and then prints what it found.
    ///
    /// Everything reachable is printed at the end. A generated program that computed the wrong
    /// answer in a variable nobody looked at would pass every comparison there is.
    fn main(&mut self) {
        self.line("int main(void) {");
        self.indent += 1;
        self.in_main = true;
        self.cost = 0;
        self.multiplier = 1;

        let outer_scalars = self.scalars.len();
        let outer_arrays = self.arrays.len();

        self.local_array();
        let statements = self.rng.between(4, 9) as usize;
        for _ in 0..statements {
            self.statement();
        }

        self.line("");
        self.line(&format!("print_int({SINK});"));
        self.line("print_char(' ');");
        for index in 0..self.scalars.len() {
            let name = self.scalars[index].name.clone();
            self.line(&format!("print_int({name});"));
            self.line("print_char(' ');");
        }
        for index in 0..self.arrays.len() {
            let name = self.arrays[index].clone();
            self.line(&format!(
                "for (int i = 0; i < {ARRAY_LENGTH}; i = i + 1) {{ print_int({name}[i]); print_char(','); }}"
            ));
        }
        self.line("print_char('\\n');");
        self.line("");
        self.line("return 0;");

        self.scalars.truncate(outer_scalars);
        self.arrays.truncate(outer_arrays);

        self.in_main = false;
        self.indent -= 1;
        self.line("}");
    }

    /// Declares a local array, fully initialized, and brings it into scope.
    fn local_array(&mut self) {
        let name = self.fresh("a");
        let elements = self.element_list();
        self.line(&format!("int {name}[{ARRAY_LENGTH}] = {{{elements}}};"));
        self.arrays.push(name);
    }

    /// Literal initializers for a whole array, so no element is ever read before it is written.
    fn element_list(&mut self) -> String {
        (0..ARRAY_LENGTH)
            .map(|_| self.rng.between(-VALUE_BOUND, VALUE_BOUND).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    // -- Statements ------------------------------------------------------------------------------

    /// Emits one statement, chosen at random from the forms the subset has.
    ///
    /// The three forms that open a block are only offered while there is depth left for them, since
    /// each one generates its body from this same list.
    fn statement(&mut self) {
        self.cost = self.cost.saturating_add(self.multiplier);

        let nesting_left = self.blocks < MAX_BLOCK_DEPTH && self.cost < WORK_BUDGET;
        let forms = if nesting_left { 9 } else { 6 };

        match self.rng.choose(forms) {
            0 => self.declaration(),
            1 => self.assignment(),
            2 => self.print_statement(),
            3 => self.increment(),
            4 => self.call_statement(),
            5 => self.array_assignment(),
            6 => self.branch(),
            7 => self.counted_loop(),
            _ => self.while_loop(),
        }
    }

    /// A new scalar, initialized in range and in scope from here on.
    fn declaration(&mut self) {
        let name = self.fresh("v");
        let value = self.bounded_expression(VALUE_BOUND);
        self.line(&format!("int {name} = {};", value.text));
        self.scalars.push(Variable {
            name,
            low: -VALUE_BOUND,
            high: VALUE_BOUND,
            assignable: true,
        });
    }

    /// An assignment to a scalar already in scope.
    fn assignment(&mut self) {
        let Some(target) = self.pick_writable_scalar() else {
            self.declaration();

            return;
        };
        let value = self.bounded_expression(VALUE_BOUND);
        self.line(&format!("{} = {};", target.name, value.text));
    }

    /// A write through an array subscript.
    fn array_assignment(&mut self) {
        let Some(array) = self.pick_array() else {
            self.assignment();

            return;
        };
        let index = self.rng.below(ARRAY_LENGTH);
        let value = self.bounded_expression(VALUE_BOUND);
        self.line(&format!("{array}[{index}] = {};", value.text));
    }

    /// An `if`, sometimes with an `else`.
    fn branch(&mut self) {
        let condition = self.condition();
        self.line(&format!("if ({}) {{", condition.text));
        self.indent += 1;
        self.body();
        self.indent -= 1;

        if self.rng.one_in(2) {
            self.line("} else {");
            self.indent += 1;
            self.body();
            self.indent -= 1;
        }

        self.line("}");
    }

    /// A `for` loop over a counter, whose bound is the array length so the counter can subscript.
    fn counted_loop(&mut self) {
        let counter = self.fresh("i");
        let limit = self.rng.between(1, ARRAY_LENGTH as i64);

        self.line(&format!(
            "for (int {counter} = 0; {counter} < {limit}; {counter} = {counter} + 1) {{"
        ));

        let outer = self.scalars.len();
        let outer_multiplier = self.multiplier;
        self.scalars.push(Variable {
            name: counter,
            low: 0,
            high: limit - 1,
            assignable: false,
        });
        self.indent += 1;
        self.loops += 1;
        self.multiplier = self.multiplier.saturating_mul(limit.max(1) as u64);
        self.body();
        self.multiplier = outer_multiplier;
        self.loops -= 1;
        self.indent -= 1;
        self.scalars.truncate(outer);

        self.line("}");
    }

    /// A `while` loop with a counter the body cannot reach, so it always terminates.
    ///
    /// The counter is declared before the loop and advanced as the last statement of the body,
    /// outside anything the generator fills in — a random body could otherwise `continue` past the
    /// advance and hang, which is indistinguishable from a compiler bug in a harness that times out.
    fn while_loop(&mut self) {
        let counter = self.fresh("w");
        let limit = self.rng.between(1, 4);
        self.line(&format!("int {counter} = 0;"));
        self.line(&format!("while ({counter} < {limit}) {{"));
        self.indent += 1;
        self.line(&format!("{counter} = {counter} + 1;"));

        let outer_multiplier = self.multiplier;
        self.loops += 1;
        self.multiplier = self.multiplier.saturating_mul(limit.max(1) as u64);
        self.body();
        self.multiplier = outer_multiplier;
        self.loops -= 1;

        self.indent -= 1;
        self.line("}");
    }

    /// Makes one expression observable, by printing it or by folding it into [`SINK`].
    fn print_statement(&mut self) {
        let value = self.expression(3);
        self.observe(&value.text, ';');
    }

    /// Whether a statement here may print, or has to fold into [`SINK`] instead.
    ///
    /// Only the top level of `main` prints. A loop body runs many times, and a function body runs
    /// once per call from a loop body, so either one printing turns the output into something no
    /// failure report can carry.
    fn may_print(&self) -> bool {
        self.in_main && self.loops == 0
    }

    /// Emits `text` so its value reaches the program's output, one way or the other.
    fn observe(&mut self, text: &str, marker: char) {
        if self.may_print() {
            self.line(&format!("print_int({text});"));
            self.line(&format!("print_char('{marker}');"));

            return;
        }

        self.line(&format!("{SINK} = ({SINK} + ({text})) % {CLAMP_MODULUS};"));
    }

    /// An increment or decrement of a scalar, as a statement so only its effect matters.
    fn increment(&mut self) {
        let Some(target) = self.pick_writable_scalar() else {
            self.declaration();

            return;
        };

        // The variable's invariant is a range, and a bare increment could walk it out. It is
        // brought back afterwards rather than being skipped, so the operator still appears.
        let form = match self.rng.choose(4) {
            0 => format!("{}++;", target.name),
            1 => format!("++{};", target.name),
            2 => format!("{}--;", target.name),
            _ => format!("--{};", target.name),
        };
        self.line(&form);
        self.line(&format!(
            "{} = {} % {CLAMP_MODULUS};",
            target.name, target.name
        ));
    }

    /// Calls a function and keeps the result, or prints it.
    fn call_statement(&mut self) {
        let Some(call) = self.call() else {
            self.print_statement();

            return;
        };
        self.observe(&call.text, '.');
    }

    /// The inside of a block: a few statements, and sometimes a way out of the loop around it.
    ///
    /// Names declared here go out of scope again at the closing brace, so the generator's idea of
    /// what is in scope has to shrink back too. Without that it goes on offering a variable that no
    /// longer exists, and the program it writes does not compile — which is a bug in the generator
    /// reported as one compiler refusing the program.
    fn body(&mut self) {
        let outer_scalars = self.scalars.len();
        let outer_arrays = self.arrays.len();

        self.blocks += 1;
        let statements = self.rng.between(1, 3) as usize;
        for _ in 0..statements {
            self.statement();
        }
        self.blocks -= 1;

        self.scalars.truncate(outer_scalars);
        self.arrays.truncate(outer_arrays);

        if self.loops > 0 && self.rng.one_in(4) {
            let condition = self.condition();
            let escape = if self.rng.one_in(2) {
                "break"
            } else {
                "continue"
            };
            self.line(&format!("if ({}) {{ {escape}; }}", condition.text));
        }
    }

    // -- Expressions -----------------------------------------------------------------------------

    /// An expression whose every possible value lies within `bound` of zero.
    fn bounded_expression(&mut self, bound: i64) -> Value {
        let value = self.expression(3);

        clamp(value, bound)
    }

    /// An expression that is `0` or `1`, for use where a condition goes.
    fn condition(&mut self) -> Value {
        let left = self.expression(2);
        let right = self.expression(2);
        let operator = ["<", ">", "<=", ">=", "==", "!="][self.rng.choose(6)];
        let comparison = Value {
            text: format!("{} {operator} {}", left.text, right.text),
            low: 0,
            high: 1,
        };

        match self.rng.choose(4) {
            0 => {
                let other = self.condition_leaf();

                Value {
                    text: format!("({}) && ({})", comparison.text, other.text),
                    low: 0,
                    high: 1,
                }
            }
            1 => {
                let other = self.condition_leaf();

                Value {
                    text: format!("({}) || ({})", comparison.text, other.text),
                    low: 0,
                    high: 1,
                }
            }
            2 => Value {
                text: format!("!({})", comparison.text),
                low: 0,
                high: 1,
            },
            _ => comparison,
        }
    }

    /// One comparison, with no logical operator on top, so `condition` cannot recurse forever.
    fn condition_leaf(&mut self) -> Value {
        let left = self.expression(1);
        let right = self.expression(1);
        let operator = ["<", ">", "<=", ">=", "==", "!="][self.rng.choose(6)];

        Value {
            text: format!("{} {operator} {}", left.text, right.text),
            low: 0,
            high: 1,
        }
    }

    /// An expression at most `depth` operators deep.
    ///
    /// Every operator is offered its operands and then checked: if the interval of the result would
    /// leave the range an `int` can hold, the operator is dropped and the left operand is used on
    /// its own. Refusing rather than retrying keeps this from ever looping, and the operands are
    /// small enough that the refusal is rare.
    fn expression(&mut self, depth: u32) -> Value {
        if depth == 0 {
            return self.leaf();
        }

        match self.rng.choose(8) {
            0 => self.arithmetic(depth, '+'),
            1 => self.arithmetic(depth, '-'),
            2 => self.arithmetic(depth, '*'),
            3 => self.division(depth, '/'),
            4 => self.division(depth, '%'),
            5 => self.negation(depth),
            6 => self.comparison(depth),
            _ => self.leaf(),
        }
    }

    /// `left op right` for `+`, `-`, or `*`, if the result still fits in an `int`.
    fn arithmetic(&mut self, depth: u32, operator: char) -> Value {
        let left = self.expression(depth - 1);
        let right = self.expression(depth - 1);

        let (low, high) = match operator {
            '+' => (left.low + right.low, left.high + right.high),
            '-' => (left.low - right.high, left.high - right.low),
            _ => {
                let corners = [
                    left.low * right.low,
                    left.low * right.high,
                    left.high * right.low,
                    left.high * right.high,
                ];

                (
                    corners.iter().copied().min().unwrap_or(0),
                    corners.iter().copied().max().unwrap_or(0),
                )
            }
        };

        let combined = Value {
            text: format!("({} {operator} {})", left.text, right.text),
            low,
            high,
        };
        if combined.fits(EXPRESSION_BOUND) {
            return combined;
        }

        left
    }

    /// `left / right` or `left % right`, with a divisor that cannot be zero.
    ///
    /// The divisor is a positive literal rather than a generated expression. That is the one shape
    /// where zero is ruled out by reading the source, and it also puts `INT_MIN / -1` — the other
    /// way division is undefined — out of reach.
    fn division(&mut self, depth: u32, operator: char) -> Value {
        let left = self.expression(depth - 1);
        let divisor = self.rng.between(2, 97);

        let magnitude = left.low.abs().max(left.high.abs());
        let (low, high) = if operator == '/' {
            (-magnitude, magnitude)
        } else {
            (-(divisor - 1), divisor - 1)
        };

        Value {
            text: format!("({} {operator} {divisor})", left.text),
            low,
            high,
        }
    }

    /// `-operand` or `!operand`.
    fn negation(&mut self, depth: u32) -> Value {
        let operand = self.expression(depth - 1);

        if self.rng.one_in(3) {
            return Value {
                text: format!("!({})", operand.text),
                low: 0,
                high: 1,
            };
        }

        Value {
            text: format!("(-({}))", operand.text),
            low: -operand.high,
            high: -operand.low,
        }
    }

    /// A comparison, which is an expression of value `0` or `1` like any other.
    fn comparison(&mut self, depth: u32) -> Value {
        let left = self.expression(depth - 1);
        let right = self.expression(depth - 1);
        let operator = ["<", ">", "<=", ">=", "==", "!="][self.rng.choose(6)];

        Value {
            text: format!("({} {operator} {})", left.text, right.text),
            low: 0,
            high: 1,
        }
    }

    /// An expression with no operator in it: a literal, a variable, a subscript, or a call.
    fn leaf(&mut self) -> Value {
        match self.rng.choose(5) {
            0 | 1 => Value::literal(self.rng.between(-64, 64)),
            2 => self
                .pick_scalar()
                .map(|variable| Value {
                    text: variable.name,
                    low: variable.low,
                    high: variable.high,
                })
                .unwrap_or_else(|| Value::literal(self.rng.between(-64, 64))),
            3 => self
                .subscript()
                .unwrap_or_else(|| Value::literal(self.rng.between(-64, 64))),
            _ => self
                .call()
                .unwrap_or_else(|| Value::literal(self.rng.between(-64, 64))),
        }
    }

    /// A read of an array element, at an index that cannot leave the array.
    ///
    /// Either a literal, or a `for` counter whose whole range the generator already knows lies
    /// inside the array. A computed subscript is worth generating — it is a multiply and an add the
    /// compiler has to get right — and those are the two shapes where staying in bounds is a fact
    /// about the source rather than a hope about the data.
    fn subscript(&mut self) -> Option<Value> {
        let array = self.pick_array()?;
        let index = match self.pick_counter() {
            Some(counter) if self.rng.one_in(2) => counter,
            _ => self.rng.below(ARRAY_LENGTH).to_string(),
        };

        Some(Value {
            text: format!("{array}[{index}]"),
            low: -VALUE_BOUND,
            high: VALUE_BOUND,
        })
    }

    /// A counter in scope whose entire range is a valid subscript, if there is one.
    fn pick_counter(&mut self) -> Option<String> {
        let counters: Vec<String> = self
            .scalars
            .iter()
            .filter(|variable| {
                !variable.assignable && variable.low >= 0 && variable.high < ARRAY_LENGTH as i64
            })
            .map(|variable| variable.name.clone())
            .collect();
        if counters.is_empty() {
            return None;
        }

        Some(counters[self.rng.below(counters.len())].clone())
    }

    /// A call to a function already emitted, with every argument in range.
    ///
    /// Arguments are shallower than the expression the call appears in, and a call inside an
    /// argument list is allowed only [`MAX_CALL_DEPTH`] deep. Both bounds exist because the
    /// arguments come from the same grammar the call is a leaf of: without them the generator
    /// descends until the stack runs out, which it did.
    fn call(&mut self) -> Option<Value> {
        if self.functions.is_empty() || self.calls >= MAX_CALL_DEPTH {
            return None;
        }
        let function = self.functions[self.rng.below(self.functions.len())].clone();

        // A call inside a loop runs once per iteration, and the callee has loops of its own. This
        // is the product that ran away: a cheap-looking call in a triply nested loop is not cheap.
        let charge = self.multiplier.saturating_mul(function.cost);
        if self.cost.saturating_add(charge) > WORK_BUDGET {
            return None;
        }
        self.cost = self.cost.saturating_add(charge);

        self.calls += 1;
        let arguments = (0..function.arity)
            .map(|_| {
                let argument = self.expression(1);

                clamp(argument, VALUE_BOUND).text
            })
            .collect::<Vec<_>>()
            .join(", ");
        self.calls -= 1;

        Some(Value {
            text: format!("{}({arguments})", function.name),
            low: -VALUE_BOUND,
            high: VALUE_BOUND,
        })
    }

    // -- Scope and output --------------------------------------------------------------------------

    /// A scalar in scope, if there is one. Any scalar may be read.
    fn pick_scalar(&mut self) -> Option<Variable> {
        if self.scalars.is_empty() {
            return None;
        }

        Some(self.scalars[self.rng.below(self.scalars.len())].clone())
    }

    /// A scalar in scope the generator is allowed to assign to.
    fn pick_writable_scalar(&mut self) -> Option<Variable> {
        let writable: Vec<Variable> = self
            .scalars
            .iter()
            .filter(|variable| variable.assignable)
            .cloned()
            .collect();
        if writable.is_empty() {
            return None;
        }

        Some(writable[self.rng.below(writable.len())].clone())
    }

    /// An array in scope, if there is one.
    fn pick_array(&mut self) -> Option<String> {
        if self.arrays.is_empty() {
            return None;
        }

        Some(self.arrays[self.rng.below(self.arrays.len())].clone())
    }

    /// A name nothing else has used.
    fn fresh(&mut self, prefix: &str) -> String {
        self.names += 1;

        format!("{prefix}{}", self.names)
    }

    /// Appends `text` as its own line, at the current indentation.
    fn line(&mut self, text: &str) {
        if text.is_empty() {
            self.source.push('\n');

            return;
        }

        for _ in 0..self.indent {
            self.source.push_str("    ");
        }
        let _ = writeln!(self.source, "{text}");
    }
}

/// Brings `value` inside `bound` of zero, leaving it alone if it is already there.
///
/// A remainder by a positive literal takes any `int` into range without a branch and without a
/// second evaluation of the operand, which is what makes it usable on an expression that may have
/// side effects. It is a free function rather than a method because it makes no random choices:
/// wrapping an expression is not part of what a seed decides.
fn clamp(value: Value, bound: i64) -> Value {
    if value.fits(bound) {
        return value;
    }

    let modulus = CLAMP_MODULUS.min(bound);

    Value {
        text: format!("({}) % {modulus}", value.text),
        low: -(modulus - 1),
        high: modulus - 1,
    }
}
