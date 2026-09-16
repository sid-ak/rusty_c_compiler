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

    /// Defines a global four-byte word holding `value`.
    pub fn define_word(&mut self, name: &str, value: i32, global: bool) {
        self.define_data(name, global, 2, &format!(".long\t{value}"));
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
