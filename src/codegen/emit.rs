//! The assembly output layer: everything that writes text, names symbols, and hands out labels.
//!
//! Getting Mach-O's conventions right here means no later part of code generation has to think
//! about them. A caller says "define this global word" or "call this function" and the spelling —
//! the leading underscore, the section directive, the alignment, the escaping — happens once.
//!
//! Assembly is written into four buffers rather than one, because the sections interleave in the
//! source program and must not interleave in the output: a function that references a string
//! literal is emitted while the text section is open, and its literal belongs in `__TEXT,__cstring`
//! at the end. [`Emitter::finish`] concatenates them in a fixed order, skipping any that stayed
//! empty, so the output is a function of the program rather than of the order the program was
//! walked in.
//!
//! The conventions below were read off `clang -S` rather than recalled, and a file in this shape
//! assembles under `clang -c` with no warnings and no `.build_version`.

use std::fmt::Write as _;

/// The indent every directive and instruction inside a section carries.
const INDENT: &str = "\t";

/// The scratch register used to hold an address or a size that no immediate field can carry.
///
/// `x9` is caller-saved and is not an argument register, so nothing of value is ever in it across
/// the two instructions that use it. Reserving one register by name, in one place, is what keeps
/// the materialization paths from having to agree with each other.
pub const SCRATCH: &str = "x9";

/// The width of a memory access, which fixes both the instruction and the offsets it can reach.
///
/// The ranges are not from memory. Each was put to `clang -c`: a word load reaches 16380 and must
/// be a multiple of four, a byte load reaches 4095 with no such rule, and a doubleword reaches
/// 32760 in multiples of eight. Past those the assembler rejects the instruction outright, which is
/// the good case — it is [`Width::reaches`] that keeps a too-large offset from ever being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    /// One byte, for a `char`.
    Byte,
    /// Four bytes, for an `int`.
    Word,
    /// Eight bytes, for a pointer or a saved register.
    Double,
}

impl Width {
    /// The instruction that loads this width, sign-extending where the width is narrower than a
    /// register.
    ///
    /// `ldrsb` sign-extends on the way in, which is what makes C's rule that a `char` promotes to
    /// an `int` fall out with no extra instruction.
    pub fn load(self) -> &'static str {
        match self {
            Width::Byte => "ldrsb",
            Width::Word | Width::Double => "ldr",
        }
    }

    /// The instruction that stores this width.
    pub fn store(self) -> &'static str {
        match self {
            Width::Byte => "strb",
            Width::Word | Width::Double => "str",
        }
    }

    /// The largest offset this width can carry in an instruction, and the step it must land on.
    fn limits(self) -> (u64, u64) {
        match self {
            Width::Byte => (4095, 1),
            Width::Word => (16380, 4),
            Width::Double => (32760, 8),
        }
    }

    /// Whether `offset` fits this width's immediate field.
    pub fn reaches(self, offset: u64) -> bool {
        let (largest, step) = self.limits();

        offset <= largest && offset.is_multiple_of(step)
    }
}

/// One of the four sections this compiler emits into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Executable code.
    Text,
    /// Read-only, null-terminated string literals.
    CString,
    /// Globals with an initial value.
    Data,
    /// Globals without one, which the loader zeroes.
    Bss,
}

impl Section {
    /// The directive that opens this section, or `None` where the entries carry their own.
    ///
    /// `__bss` is the exception: `.zerofill` names its section on every line, so there is no
    /// section to open.
    fn opening_directive(self) -> Option<&'static str> {
        match self {
            Section::Text => Some(".section\t__TEXT,__text,regular,pure_instructions"),
            Section::CString => Some(".section\t__TEXT,__cstring,cstring_literals"),
            Section::Data => Some(".section\t__DATA,__data"),
            Section::Bss => None,
        }
    }
}

/// The assembly file being built.
#[derive(Debug, Clone, Default)]
pub struct Emitter {
    /// Executable code.
    text: String,
    /// String literals.
    cstring: String,
    /// Initialized globals.
    data: String,
    /// Uninitialized globals.
    bss: String,
    /// The number of labels handed out so far, which is what makes the next one unique.
    labels: u32,
}

impl Emitter {
    /// An empty assembly file.
    pub fn new() -> Self {
        Self::default()
    }

    /// The Mach-O symbol for the C name `name`.
    ///
    /// Mach-O prefixes every C symbol with an underscore, so the C function `main` is the symbol
    /// `_main`. Doing it in one place means no caller can forget.
    pub fn symbol(name: &str) -> String {
        format!("_{name}")
    }

    /// A label named after what it is for, unique within the file.
    ///
    /// The counter runs across the whole file rather than restarting at each function. A restarting
    /// counter would be just as readable and wrong: `Lif_else_1` in two functions is one name for
    /// two places, and the assembler would resolve every reference to the first.
    ///
    /// The `L` prefix is what makes the label local to the file in Mach-O, so it does not appear in
    /// the symbol table.
    pub fn new_label(&mut self, purpose: &str) -> String {
        let label = format!("L{purpose}_{}", self.labels);
        self.labels = self.labels.saturating_add(1);

        label
    }

    /// Opens a function: external linkage, alignment, and its label.
    ///
    /// ARM64 instructions are four bytes and must be four-byte aligned, which is what `.p2align 2`
    /// asks for — an alignment of two to the power of two.
    pub fn begin_function(&mut self, name: &str) {
        let symbol = Self::symbol(name);

        self.line(Section::Text, &format!(".globl\t{symbol}"));
        self.line(Section::Text, ".p2align\t2");
        self.place_label(&symbol);
    }

    /// One instruction in the text section.
    pub fn instruction(&mut self, instruction: &str) {
        self.line(Section::Text, instruction);
    }

    /// A label definition in the text section, at the left margin.
    pub fn place_label(&mut self, label: &str) {
        let _ = writeln!(self.buffer(Section::Text), "{label}:");
    }

    /// Calls the C function `name`.
    pub fn call(&mut self, name: &str) {
        let symbol = Self::symbol(name);
        self.instruction(&format!("bl {symbol}"));
    }

    /// Jumps to `label` unconditionally.
    pub fn branch(&mut self, label: &str) {
        self.instruction(&format!("b {label}"));
    }

    /// Jumps to `label` when `register` holds zero.
    pub fn branch_if_zero(&mut self, register: &str, label: &str) {
        self.instruction(&format!("cbz {register}, {label}"));
    }

    /// Jumps to `label` when `register` holds anything but zero.
    pub fn branch_if_nonzero(&mut self, register: &str, label: &str) {
        self.instruction(&format!("cbnz {register}, {label}"));
    }

    /// Puts the address of `symbol` into `register`.
    ///
    /// ARM64 has no instruction wide enough to hold a 64-bit address, so an address is built in two
    /// steps: `adrp` loads the 4 KB page the symbol is on, and `add` applies its offset within that
    /// page. The pair is written here so no caller emits half of it.
    pub fn address_of(&mut self, register: &str, symbol: &str) {
        self.instruction(&format!("adrp {register}, {symbol}@PAGE"));
        self.instruction(&format!("add {register}, {register}, {symbol}@PAGEOFF"));
    }

    /// Puts the literal `value` into `register`.
    ///
    /// `mov` with a wide immediate only accepts what fits sixteen bits, so anything larger is built
    /// in sixteen-bit pieces: `movz` writes the lowest and zeroes the rest, and each `movk` writes
    /// one more piece without disturbing what is already there.
    pub fn load_immediate(&mut self, register: &str, value: u64) {
        self.instruction(&format!("movz {register}, #{}", value & 0xffff));

        for shift in [16, 32, 48] {
            let piece = (value >> shift) & 0xffff;
            if piece != 0 {
                self.instruction(&format!("movk {register}, #{piece}, lsl #{shift}"));
            }
        }
    }

    /// Puts a 32-bit literal into a `w` register.
    ///
    /// A `w` register's `movk` only shifts by sixteen, so a word is two pieces rather than four.
    /// The value is taken as its bit pattern, which is what makes a negative constant work without
    /// a negation: `-1` is `0xffffffff`, built from two pieces like any other number.
    pub fn load_word_immediate(&mut self, register: &str, value: i32) {
        #[expect(
            clippy::cast_sign_loss,
            reason = "the bit pattern is the point; a negative constant is built from its two halves"
        )]
        let bits = value as u32;

        self.instruction(&format!("movz {register}, #{}", bits & 0xffff));

        let high = bits >> 16;
        if high != 0 {
            self.instruction(&format!("movk {register}, #{high}, lsl #16"));
        }
    }

    /// Puts the address of the frame slot at `offset` into `register`.
    pub fn frame_address(&mut self, register: &str, offset: u64) {
        // `add` carries a twelve-bit immediate, so anything larger is built first.
        if offset <= 4095 {
            self.instruction(&format!("add {register}, x29, #{offset}"));

            return;
        }

        self.load_immediate(SCRATCH, offset);
        self.instruction(&format!("add {register}, x29, {SCRATCH}"));
    }

    /// Loads `register` from `offset` bytes into the current frame.
    pub fn load_from_frame(&mut self, register: &str, width: Width, offset: u64) {
        let instruction = width.load();
        self.frame_access(instruction, register, width, offset);
    }

    /// Stores `register` at `offset` bytes into the current frame.
    pub fn store_to_frame(&mut self, register: &str, width: Width, offset: u64) {
        let instruction = width.store();
        self.frame_access(instruction, register, width, offset);
    }

    /// One frame load or store, reaching `offset` however far away it is.
    ///
    /// An offset the instruction's immediate field cannot hold is computed into [`SCRATCH`] and the
    /// access goes through that instead. The alternative — letting the assembler see an offset it
    /// cannot encode — is at least an error rather than a wrong answer, but a function with enough
    /// locals is not a program this compiler should refuse.
    fn frame_access(&mut self, instruction: &str, register: &str, width: Width, offset: u64) {
        if width.reaches(offset) {
            self.instruction(&format!("{instruction} {register}, [x29, #{offset}]"));

            return;
        }

        self.load_immediate(SCRATCH, offset);
        self.instruction(&format!("add {SCRATCH}, x29, {SCRATCH}"));
        self.instruction(&format!("{instruction} {register}, [{SCRATCH}]"));
    }

    /// Defines a global four-byte word holding `value`.
    pub fn define_word(&mut self, name: &str, value: i32, global: bool) {
        self.define_data(name, global, 2, &format!(".long\t{value}"));
    }

    /// Opens a global datum: its linkage, its alignment, and its label.
    ///
    /// The value follows as one or more of [`data_word`](Emitter::data_word),
    /// [`data_byte`](Emitter::data_byte) and [`data_zero`](Emitter::data_zero), because an array is
    /// written element by element and may be shorter than the storage it was declared with.
    pub fn begin_data(&mut self, name: &str, align: u32) {
        let symbol = Self::symbol(name);

        self.line(Section::Data, &format!(".globl\t{symbol}"));
        self.line(Section::Data, &format!(".p2align\t{align}"));
        let _ = writeln!(self.buffer(Section::Data), "{symbol}:");
    }

    /// Four bytes of initialized data.
    pub fn data_word(&mut self, value: i32) {
        self.line(Section::Data, &format!(".long\t{value}"));
    }

    /// One byte of initialized data.
    pub fn data_byte(&mut self, value: i32) {
        self.line(Section::Data, &format!(".byte\t{value}"));
    }

    /// `bytes` zeroed bytes, for the part of an array an initializer did not reach.
    pub fn data_zero(&mut self, bytes: u64) {
        if bytes == 0 {
            return;
        }

        self.line(Section::Data, &format!(".space\t{bytes}"));
    }

    /// Defines a global run of bytes, for a `char` array or an array's initializer image.
    pub fn define_bytes(&mut self, name: &str, bytes: &[u8], align: u32, global: bool) {
        let body = format!(".ascii\t\"{}\"", escape(bytes));
        self.define_data(name, global, align, &body);
    }

    /// Defines a null-terminated string literal in `__TEXT,__cstring`.
    ///
    /// String literals carry no `.globl`: the label is file-local, and the program refers to it by
    /// address rather than by name.
    pub fn define_string(&mut self, label: &str, bytes: &[u8]) {
        let _ = writeln!(self.buffer(Section::CString), "{label}:");
        self.line(Section::CString, &format!(".asciz\t\"{}\"", escape(bytes)));
    }

    /// Reserves `size` zeroed bytes for a global that was declared without an initializer.
    ///
    /// Reserved rather than written: `.zerofill` records the size in the object file and the loader
    /// provides the zeroes, so a large uninitialized array costs nothing on disk.
    pub fn reserve_zeroed(&mut self, name: &str, size: u64, align: u32) {
        let symbol = Self::symbol(name);

        self.line(Section::Bss, &format!(".globl\t{symbol}"));
        let _ = writeln!(
            self.buffer(Section::Bss),
            ".zerofill __DATA,__bss,{symbol},{size},{align}"
        );
    }

    /// The finished assembly file.
    ///
    /// Sections are concatenated in a fixed order and an empty one is left out entirely. An empty
    /// `__DATA,__data` assembles perfectly well and says something untrue about the program, which
    /// makes a snapshot harder to read than it needs to be.
    pub fn finish(self) -> String {
        let mut assembly = String::new();

        for (section, body) in [
            (Section::Text, &self.text),
            (Section::CString, &self.cstring),
            (Section::Data, &self.data),
            (Section::Bss, &self.bss),
        ] {
            if body.is_empty() {
                continue;
            }

            if let Some(directive) = section.opening_directive() {
                let _ = writeln!(assembly, "{INDENT}{directive}");
            }
            assembly.push_str(body);
        }

        // Lets the linker discard any function or datum nothing referenced.
        assembly.push_str(".subsections_via_symbols\n");

        assembly
    }

    /// Defines a datum in `__DATA,__data`: its linkage, its alignment, its label, then its body.
    fn define_data(&mut self, name: &str, global: bool, align: u32, body: &str) {
        let symbol = Self::symbol(name);

        if global {
            self.line(Section::Data, &format!(".globl\t{symbol}"));
        }
        self.line(Section::Data, &format!(".p2align\t{align}"));
        let _ = writeln!(self.buffer(Section::Data), "{symbol}:");
        self.line(Section::Data, body);
    }

    /// Writes one indented line into `section`.
    fn line(&mut self, section: Section, text: &str) {
        let _ = writeln!(self.buffer(section), "{INDENT}{text}");
    }

    /// The buffer `section` accumulates into.
    fn buffer(&mut self, section: Section) -> &mut String {
        match section {
            Section::Text => &mut self.text,
            Section::CString => &mut self.cstring,
            Section::Data => &mut self.data,
            Section::Bss => &mut self.bss,
        }
    }
}

/// `bytes` written as the assembler's string syntax.
///
/// Anything that is not plainly printable becomes an escape, and a byte with no named escape
/// becomes a three-digit octal one — always three digits, so a following digit cannot be read as
/// part of it.
fn escape(bytes: &[u8]) -> String {
    let mut escaped = String::new();

    for &byte in bytes {
        match byte {
            b'"' => escaped.push_str("\\\""),
            b'\\' => escaped.push_str("\\\\"),
            b'\n' => escaped.push_str("\\n"),
            b'\t' => escaped.push_str("\\t"),
            b'\r' => escaped.push_str("\\r"),
            0x20..=0x7e => escaped.push(char::from(byte)),
            other => {
                let _ = write!(escaped, "\\{other:03o}");
            }
        }
    }

    escaped
}

#[cfg(test)]
mod tests;
