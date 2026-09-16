//! The type model and the rules that govern it.
//!
//! Every type question the rest of the compiler asks is answered here: how much storage a type
//! needs, what a `char` becomes in arithmetic, what an array becomes at a call site, and what may
//! be assigned to what. Semantic analysis records the answers; code generation reads them back and
//! reasons about types no further.
//!
//! Two rules are worth stating up front because they shape everything below:
//!
//! - Storage is not computation. A `char` occupies one byte, but no arithmetic is ever performed
//!   on one byte: it is widened to `int` first, exactly as C requires.
//! - An array becomes a pointer at one place only — the function-argument position. There is no
//!   other pointer-producing expression in this subset, which is
//!   [ADR 0007](../../docs/decisions/0007-array-decay-only-at-parameter-boundary.md).

use std::fmt;

/// The size in bytes of a `char`, and its alignment.
const CHAR_BYTES: u64 = 1;

/// The size in bytes of an `int`, and its alignment.
const INT_BYTES: u64 = 4;

/// The size in bytes of a pointer on ARM64, and its alignment.
const POINTER_BYTES: u64 = 8;

/// A type in the accepted subset of C.
///
/// `Ptr` has no declarator that produces it — the subset has no `int *p` — but parameters written
/// `int a[]` and string literals genuinely have pointer type, so the model carries it. The
/// restriction is on which expressions can produce a pointer, not on what can be represented.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    /// A 32-bit signed integer.
    Int,
    /// An 8-bit signed character. Stored in one byte; computed on as an `int`.
    Char,
    /// The absence of a value. Legal as a function return type and nowhere else.
    Void,
    /// A fixed-length array of `element`, holding `length` of them.
    Array(Box<Ty>, u32),
    /// A pointer to `pointee`.
    Ptr(Box<Ty>),
    /// A type analysis could not work out, because it already reported why.
    ///
    /// Recovery, not a type any program can write. Every rule below treats it as acceptable, so a
    /// value whose type was already reported wrong does not collect a second complaint from each
    /// operator it then flows through. One mistake, one message.
    Error,
    /// A function taking `params` and returning `ret`.
    Func {
        /// What the function returns.
        ret: Box<Ty>,
        /// What the function takes, in declaration order.
        params: Vec<Ty>,
    },
}

/// How much storage a type occupies and how it must be aligned, both in bytes.
///
/// Sizes are `u64` so that an array long enough to overflow 32-bit arithmetic still reports its
/// real size. Whether a frame that large is acceptable is a question for the analyzer, which can
/// only ask it if the number it is handed is honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Total bytes occupied.
    pub size: u64,
    /// Byte boundary the value must start on.
    pub align: u64,
}

/// An implicit conversion the source program did not write but C requires.
///
/// Analysis records one of these against the expression it applies to rather than rewriting the
/// tree, so the AST stays exactly as the parser built it (ADR 0004) while code generation still
/// learns that a widening or an address-of has to be emitted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Conversion {
    /// A `char` widened to `int`, sign-extending.
    PromoteCharToInt,
    /// An `int` narrowed to `char`, keeping the low byte.
    TruncateIntToChar,
    /// An array replaced by the address of its first element.
    DecayArrayToPtr,
}

/// Whether a value of one type may be stored into a location of another, and at what cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assignability {
    /// The types are the same; the value is stored as it stands.
    Exact,
    /// Legal once the named conversion is applied to the value first.
    Converted(Conversion),
    /// Not legal at all.
    Incompatible,
}

impl Ty {
    /// An array of `length` elements of type `element`.
    pub fn array(element: Ty, length: u32) -> Self {
        Ty::Array(Box::new(element), length)
    }

    /// A pointer to `pointee`.
    pub fn ptr(pointee: Ty) -> Self {
        Ty::Ptr(Box::new(pointee))
    }

    /// A function taking `params` and returning `ret`.
    pub fn func(ret: Ty, params: Vec<Ty>) -> Self {
        Ty::Func {
            ret: Box::new(ret),
            params,
        }
    }

    /// The storage this type needs, or `None` if it has none.
    ///
    /// `void` and function types are incomplete: they name something that is not a value, so
    /// there is no size to report. An array of an incomplete type is incomplete for the same
    /// reason, which is how `void a[4]` is caught without a rule of its own.
    ///
    /// An array too large to measure reports `None` as well. Nothing the parser accepts can build
    /// one — it rejects the nesting that would be needed — but this is a public entry point, and a
    /// caller that hands it such a type must get an answer rather than an overflow panic.
    pub fn layout(&self) -> Option<Layout> {
        match self {
            Ty::Char => Some(Layout {
                size: CHAR_BYTES,
                align: CHAR_BYTES,
            }),
            Ty::Int => Some(Layout {
                size: INT_BYTES,
                align: INT_BYTES,
            }),
            Ty::Ptr(_) => Some(Layout {
                size: POINTER_BYTES,
                align: POINTER_BYTES,
            }),
            Ty::Array(element, length) => {
                let element = element.layout()?;

                Some(Layout {
                    size: element.size.checked_mul(u64::from(*length))?,
                    align: element.align,
                })
            }
            Ty::Void | Ty::Error | Ty::Func { .. } => None,
        }
    }

    /// This type after integer promotion: `char` becomes `int`, everything else is unchanged.
    ///
    /// Promotion applies in every arithmetic, comparison, and logical context. It is the reason
    /// `char` needs no arithmetic of its own anywhere in the compiler.
    pub fn promoted(&self) -> Ty {
        match self {
            Ty::Char => Ty::Int,
            other => other.clone(),
        }
    }

    /// Whether [`promoted`](Ty::promoted) would change this type.
    pub fn promotes(&self) -> bool {
        matches!(self, Ty::Char)
    }

    /// This type after array-to-pointer decay, which the caller applies only at an argument.
    ///
    /// Nothing about this function decides where decay is legal; it only says what decay produces.
    /// The single place it may be called from is stated in ADR 0007.
    pub fn decayed(&self) -> Ty {
        match self {
            Ty::Array(element, _) => Ty::Ptr(element.clone()),
            other => other.clone(),
        }
    }

    /// Whether [`decayed`](Ty::decayed) would change this type.
    pub fn decays(&self) -> bool {
        matches!(self, Ty::Array(_, _))
    }

    /// Whether arithmetic may be performed on this type.
    ///
    /// [`Ty::Error`] qualifies, as it does everywhere: suppressing the follow-on complaint is the
    /// whole reason it exists.
    pub fn is_arithmetic(&self) -> bool {
        matches!(self, Ty::Int | Ty::Char | Ty::Error)
    }

    /// Whether this is the recovery type.
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Whether this type can be tested for truth.
    ///
    /// This is exactly the set a condition context accepts — an `if`, a `while`, a `for`
    /// condition, and the operands of `&&`, `||`, and `!` — so there is no second predicate for
    /// those to drift away from.
    pub fn is_scalar(&self) -> bool {
        matches!(self, Ty::Int | Ty::Char | Ty::Ptr(_) | Ty::Error)
    }

    /// The type both operands of an arithmetic or comparison operator are converted to.
    ///
    /// `None` means the pairing has no common type and the operator does not apply. An array is
    /// one of those pairings: `a + 1` is rejected rather than treated as pointer arithmetic.
    pub fn common_arithmetic(left: &Ty, right: &Ty) -> Option<Ty> {
        if left.is_error() || right.is_error() {
            return Some(Ty::Error);
        }

        if left.is_arithmetic() && right.is_arithmetic() {
            Some(Ty::Int)
        } else {
            None
        }
    }

    /// Whether a value of type `source` may be stored into a location of type `target`.
    ///
    /// The array case is the parameter-passing rule and only that: it reports the decay so the
    /// caller can record it, and says nothing about whether the caller was entitled to ask.
    pub fn assignability(target: &Ty, source: &Ty) -> Assignability {
        if target.is_error() || source.is_error() {
            return Assignability::Exact;
        }

        match (target, source) {
            (Ty::Int, Ty::Int) | (Ty::Char, Ty::Char) => Assignability::Exact,
            (Ty::Int, Ty::Char) => Assignability::Converted(Conversion::PromoteCharToInt),
            (Ty::Char, Ty::Int) => Assignability::Converted(Conversion::TruncateIntToChar),
            (Ty::Ptr(pointee), Ty::Ptr(source_pointee)) if pointee == source_pointee => {
                Assignability::Exact
            }
            (Ty::Ptr(pointee), Ty::Array(element, _)) if pointee == element => {
                Assignability::Converted(Conversion::DecayArrayToPtr)
            }
            _ => Assignability::Incompatible,
        }
    }
}

impl fmt::Display for Ty {
    /// Spells the type as C does, so a diagnostic can quote one without rephrasing it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Array dimensions are peeled before anything is written, because C spells them after the
        // element type and outermost first: `int[2][3]` is two arrays of three ints.
        let mut dimensions = Vec::new();
        let mut innermost = self;
        while let Ty::Array(element, length) = innermost {
            dimensions.push(*length);
            innermost = element;
        }

        match innermost {
            Ty::Int => write!(formatter, "int")?,
            Ty::Char => write!(formatter, "char")?,
            Ty::Void => write!(formatter, "void")?,
            Ty::Error => write!(formatter, "<error>")?,
            Ty::Ptr(pointee) => write!(formatter, "{pointee} *")?,
            Ty::Func { ret, params } => {
                write!(formatter, "{ret}(")?;
                if params.is_empty() {
                    write!(formatter, "void")?;
                } else {
                    for (position, param) in params.iter().enumerate() {
                        if position > 0 {
                            write!(formatter, ", ")?;
                        }
                        write!(formatter, "{param}")?;
                    }
                }
                write!(formatter, ")")?;
            }
            // Peeled above, so an array can no longer be the innermost type here.
            Ty::Array(_, _) => {}
        }

        for length in dimensions {
            write!(formatter, "[{length}]")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
