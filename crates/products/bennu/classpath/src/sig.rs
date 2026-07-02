//! Homegrown parser for the JVM `Signature` attribute (JVM Spec §4.7.9.1).
//!
//! Ported verbatim from the proven `bennu-spike-bytecode` (docs §10, confidence
//! 9/10): no Rust crate decodes the generic `Signature` attribute, so this decoder
//! is a mandatory work item, not a library choice (docs §4). Kept unchanged so its
//! proven test cases (`Optional.map`, `List.iterator`, `Map.entrySet`, class
//! signatures) keep passing.
//!
//! Grammar implemented (subset covering ClassSignature / MethodSignature /
//! FieldSignature and everything they transitively reference):
//!
//! ```text
//! JavaTypeSignature      := ReferenceTypeSignature | BaseType
//! BaseType               := B C D F I J S Z
//! ReferenceTypeSignature := ClassTypeSignature | TypeVariableSignature | ArrayTypeSignature
//! ClassTypeSignature     := 'L' [PackageSpecifier] SimpleClassTypeSignature
//!                              {'.' SimpleClassTypeSignature} ';'
//! SimpleClassTypeSig     := Identifier [TypeArguments]
//! TypeArguments          := '<' TypeArgument {TypeArgument} '>'
//! TypeArgument           := ['+'|'-'] ReferenceTypeSignature | '*'
//! TypeVariableSignature  := 'T' Identifier ';'
//! ArrayTypeSignature     := '[' JavaTypeSignature
//! ClassSignature         := [TypeParameters] SuperclassSig {SuperinterfaceSig}
//! MethodSignature        := [TypeParameters] '(' {JavaTypeSignature} ')' Result {ThrowsSig}
//! Result                 := JavaTypeSignature | VoidDescriptor ('V')
//! TypeParameters         := '<' TypeParameter {TypeParameter} '>'
//! TypeParameter          := Identifier ClassBound {InterfaceBound}
//! ClassBound             := ':' [ReferenceTypeSignature]
//! InterfaceBound         := ':' ReferenceTypeSignature
//! ```

use std::fmt;

/// A resolved reference/base type in a signature.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeSig {
    /// Primitive base type, e.g. `I` -> "int".
    Base(char),
    /// Void result (`V`), only valid as a method result.
    Void,
    /// A class type, possibly generic and possibly nested (inner classes).
    Class(ClassType),
    /// A reference to a declared type parameter, e.g. `TE;` -> `E`.
    TypeVar(String),
    /// An array of the inner type, e.g. `[I` -> `int[]`.
    Array(Box<TypeSig>),
}

/// A class type signature, including any nested inner-class suffixes.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassType {
    /// Fully qualified binary name of the outermost class, dots for package,
    /// slash-free (we normalise `/` -> `.`). e.g. `java.util.Map`.
    pub name: String,
    /// Type arguments applied to the outermost class.
    pub args: Vec<TypeArg>,
    /// Inner-class suffixes: each is `(SimpleName, args)`.
    pub inners: Vec<(String, Vec<TypeArg>)>,
}

/// A single type argument inside `<...>`.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeArg {
    /// `*` — unbounded wildcard `?`.
    Unbounded,
    /// `+X` — `? extends X`.
    Extends(TypeSig),
    /// `-X` — `? super X`.
    Super(TypeSig),
    /// `X` — invariant, the type itself.
    Exact(TypeSig),
}

/// A declared type parameter, e.g. `T:Ljava/lang/Object;` -> `T extends Object`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: String,
    /// Class bound (may be absent when only interface bounds are given).
    pub class_bound: Option<TypeSig>,
    pub interface_bounds: Vec<TypeSig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodSig {
    pub type_params: Vec<TypeParam>,
    pub params: Vec<TypeSig>,
    pub result: TypeSig,
    pub throws: Vec<TypeSig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassSig {
    pub type_params: Vec<TypeParam>,
    pub superclass: TypeSig,
    pub interfaces: Vec<TypeSig>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

pub struct SigParser<'a> {
    b: &'a [u8],
    i: usize,
}

pub type PResult<T> = Result<T, String>;

impl<'a> SigParser<'a> {
    pub fn new(s: &'a str) -> Self {
        SigParser { b: s.as_bytes(), i: 0 }
    }

    fn peek(&self) -> PResult<u8> {
        self.b.get(self.i).copied().ok_or_else(|| "unexpected end of signature".to_string())
    }

    fn bump(&mut self) -> PResult<u8> {
        let c = self.peek()?;
        self.i += 1;
        Ok(c)
    }

    fn eat(&mut self, c: u8) -> PResult<()> {
        let g = self.bump()?;
        if g != c {
            return Err(format!("expected '{}' but found '{}' at pos {}", c as char, g as char, self.i - 1));
        }
        Ok(())
    }

    fn at_end(&self) -> bool {
        self.i >= self.b.len()
    }

    /// Identifier: any run of chars up to one of the delimiters `. ; [ / < : >`.
    fn identifier(&mut self) -> PResult<String> {
        let start = self.i;
        while let Some(&c) = self.b.get(self.i) {
            match c {
                b'.' | b';' | b'[' | b'/' | b'<' | b'>' | b':' => break,
                _ => self.i += 1,
            }
        }
        if self.i == start {
            return Err(format!("empty identifier at pos {}", start));
        }
        Ok(String::from_utf8_lossy(&self.b[start..self.i]).into_owned())
    }

    // --- entry points -----------------------------------------------------

    pub fn parse_class_signature(&mut self) -> PResult<ClassSig> {
        let type_params = self.type_params_opt()?;
        let superclass = self.class_type_signature_as_typesig()?;
        let mut interfaces = Vec::new();
        while !self.at_end() {
            interfaces.push(self.class_type_signature_as_typesig()?);
        }
        Ok(ClassSig { type_params, superclass, interfaces })
    }

    pub fn parse_method_signature(&mut self) -> PResult<MethodSig> {
        let type_params = self.type_params_opt()?;
        self.eat(b'(')?;
        let mut params = Vec::new();
        while self.peek()? != b')' {
            params.push(self.java_type_signature()?);
        }
        self.eat(b')')?;
        let result = if self.peek()? == b'V' {
            self.bump()?;
            TypeSig::Void
        } else {
            self.java_type_signature()?
        };
        let mut throws = Vec::new();
        while !self.at_end() {
            self.eat(b'^')?;
            // ThrowsSignature is ClassTypeSignature | TypeVariableSignature
            throws.push(self.reference_type_signature()?);
        }
        Ok(MethodSig { type_params, params, result, throws })
    }

    pub fn parse_field_signature(&mut self) -> PResult<TypeSig> {
        // A field signature is a single ReferenceTypeSignature.
        self.reference_type_signature()
    }

    // --- productions ------------------------------------------------------

    fn type_params_opt(&mut self) -> PResult<Vec<TypeParam>> {
        if self.peek()? != b'<' {
            return Ok(Vec::new());
        }
        self.eat(b'<')?;
        let mut out = Vec::new();
        while self.peek()? != b'>' {
            let name = self.identifier()?;
            self.eat(b':')?;
            // ClassBound: possibly empty (next char is ':' or '>').
            let class_bound = if self.peek()? == b':' || self.peek()? == b'>' {
                None
            } else {
                Some(self.reference_type_signature()?)
            };
            let mut interface_bounds = Vec::new();
            while self.peek()? == b':' {
                self.eat(b':')?;
                interface_bounds.push(self.reference_type_signature()?);
            }
            out.push(TypeParam { name, class_bound, interface_bounds });
        }
        self.eat(b'>')?;
        Ok(out)
    }

    fn java_type_signature(&mut self) -> PResult<TypeSig> {
        match self.peek()? {
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => Ok(TypeSig::Base(self.bump()? as char)),
            _ => self.reference_type_signature(),
        }
    }

    fn reference_type_signature(&mut self) -> PResult<TypeSig> {
        match self.peek()? {
            b'L' => self.class_type_signature_as_typesig(),
            b'T' => {
                self.eat(b'T')?;
                let name = self.identifier()?;
                self.eat(b';')?;
                Ok(TypeSig::TypeVar(name))
            }
            b'[' => {
                self.eat(b'[')?;
                Ok(TypeSig::Array(Box::new(self.java_type_signature()?)))
            }
            c => Err(format!("expected reference type signature but found '{}' at pos {}", c as char, self.i)),
        }
    }

    fn class_type_signature_as_typesig(&mut self) -> PResult<TypeSig> {
        Ok(TypeSig::Class(self.class_type_signature()?))
    }

    fn class_type_signature(&mut self) -> PResult<ClassType> {
        self.eat(b'L')?;
        // PackageSpecifier + first SimpleClassTypeSignature.
        // We accumulate identifiers joined by '/' until we hit type args, ';', or '.'.
        let mut name = self.identifier()?;
        while self.peek()? == b'/' {
            self.eat(b'/')?;
            name.push('.');
            name.push_str(&self.identifier()?);
        }
        let args = self.type_arguments_opt()?;
        let mut inners = Vec::new();
        while self.peek()? == b'.' {
            self.eat(b'.')?;
            let iname = self.identifier()?;
            let iargs = self.type_arguments_opt()?;
            inners.push((iname, iargs));
        }
        self.eat(b';')?;
        Ok(ClassType { name, args, inners })
    }

    fn type_arguments_opt(&mut self) -> PResult<Vec<TypeArg>> {
        if self.peek()? != b'<' {
            return Ok(Vec::new());
        }
        self.eat(b'<')?;
        let mut out = Vec::new();
        while self.peek()? != b'>' {
            out.push(self.type_argument()?);
        }
        self.eat(b'>')?;
        Ok(out)
    }

    fn type_argument(&mut self) -> PResult<TypeArg> {
        match self.peek()? {
            b'*' => {
                self.bump()?;
                Ok(TypeArg::Unbounded)
            }
            b'+' => {
                self.bump()?;
                Ok(TypeArg::Extends(self.reference_type_signature()?))
            }
            b'-' => {
                self.bump()?;
                Ok(TypeArg::Super(self.reference_type_signature()?))
            }
            _ => Ok(TypeArg::Exact(self.reference_type_signature()?)),
        }
    }
}

// ---------------------------------------------------------------------------
// Pretty-printing back into Java-like source form
// ---------------------------------------------------------------------------

fn base_name(c: char) -> &'static str {
    match c {
        'B' => "byte",
        'C' => "char",
        'D' => "double",
        'F' => "float",
        'I' => "int",
        'J' => "long",
        'S' => "short",
        'Z' => "boolean",
        _ => "?",
    }
}

/// Strip a leading `java.lang.` / package to a short readable name but keep the
/// last two segments for clarity where it helps (we just keep the simple name).
fn short(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

impl fmt::Display for TypeSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeSig::Base(c) => write!(f, "{}", base_name(*c)),
            TypeSig::Void => write!(f, "void"),
            TypeSig::TypeVar(n) => write!(f, "{}", n),
            TypeSig::Array(inner) => write!(f, "{}[]", inner),
            TypeSig::Class(ct) => write!(f, "{}", ct),
        }
    }
}

impl fmt::Display for ClassType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", short(&self.name))?;
        write_args(f, &self.args)?;
        for (iname, iargs) in &self.inners {
            write!(f, ".{}", iname)?;
            write_args(f, iargs)?;
        }
        Ok(())
    }
}

fn write_args(f: &mut fmt::Formatter<'_>, args: &[TypeArg]) -> fmt::Result {
    if args.is_empty() {
        return Ok(());
    }
    write!(f, "<")?;
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{}", a)?;
    }
    write!(f, ">")
}

impl fmt::Display for TypeArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeArg::Unbounded => write!(f, "?"),
            TypeArg::Extends(t) => write!(f, "? extends {}", t),
            TypeArg::Super(t) => write!(f, "? super {}", t),
            TypeArg::Exact(t) => write!(f, "{}", t),
        }
    }
}

impl fmt::Display for TypeParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        let mut bounds: Vec<String> = Vec::new();
        if let Some(cb) = &self.class_bound {
            // Suppress the vacuous `extends Object` for readability.
            let s = cb.to_string();
            if s != "Object" {
                bounds.push(s);
            }
        }
        for ib in &self.interface_bounds {
            bounds.push(ib.to_string());
        }
        if !bounds.is_empty() {
            write!(f, " extends {}", bounds.join(" & "))?;
        }
        Ok(())
    }
}

fn write_type_params(f: &mut fmt::Formatter<'_>, tps: &[TypeParam]) -> fmt::Result {
    if tps.is_empty() {
        return Ok(());
    }
    write!(f, "<")?;
    for (i, tp) in tps.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{}", tp)?;
    }
    write!(f, "> ")
}

impl fmt::Display for MethodSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_type_params(f, &self.type_params)?;
        write!(f, "{} (", self.result)?;
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", p)?;
        }
        write!(f, ")")?;
        if !self.throws.is_empty() {
            write!(f, " throws ")?;
            for (i, t) in self.throws.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", t)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for ClassSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_type_params(f, &self.type_params)?;
        write!(f, "extends {}", self.superclass)?;
        if !self.interfaces.is_empty() {
            write!(f, " implements ")?;
            for (i, it) in self.interfaces.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", it)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Convenience wrappers
// ---------------------------------------------------------------------------

pub fn parse_class(s: &str) -> PResult<ClassSig> {
    let mut p = SigParser::new(s);
    let r = p.parse_class_signature()?;
    ensure_consumed(&p, s)?;
    Ok(r)
}

pub fn parse_method(s: &str) -> PResult<MethodSig> {
    let mut p = SigParser::new(s);
    let r = p.parse_method_signature()?;
    ensure_consumed(&p, s)?;
    Ok(r)
}

pub fn parse_field(s: &str) -> PResult<TypeSig> {
    let mut p = SigParser::new(s);
    let r = p.parse_field_signature()?;
    ensure_consumed(&p, s)?;
    Ok(r)
}

fn ensure_consumed(p: &SigParser, s: &str) -> PResult<()> {
    if !p.at_end() {
        return Err(format!("trailing input at pos {} in signature `{}`", p.i, s));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_map() {
        // <U:Ljava/lang/Object;>(Ljava/util/function/Function<-TT;+TU;>;)Ljava/util/Optional<TU;>;
        let s = "<U:Ljava/lang/Object;>(Ljava/util/function/Function<-TT;+TU;>;)Ljava/util/Optional<TU;>;";
        let m = parse_method(s).unwrap();
        assert_eq!(m.type_params.len(), 1);
        assert_eq!(m.type_params[0].name, "U");
        assert_eq!(m.params.len(), 1);
        let rendered = m.to_string();
        assert!(rendered.contains("Function<? super T, ? extends U>"), "{rendered}");
        assert!(rendered.contains("Optional<U>"), "{rendered}");
    }

    #[test]
    fn map_entryset() {
        // ()Ljava/util/Set<Ljava/util/Map$Entry<TK;TV;>;>;
        let s = "()Ljava/util/Set<Ljava/util/Map$Entry<TK;TV;>;>;";
        let m = parse_method(s).unwrap();
        let r = m.to_string();
        assert!(r.contains("Set<Map$Entry<K, V>>"), "{r}");
    }

    #[test]
    fn list_class_sig() {
        // interface List<E> extends Collection<E>
        let s = "<E:Ljava/lang/Object;>Ljava/lang/Object;Ljava/util/Collection<TE;>;";
        let c = parse_class(s).unwrap();
        assert_eq!(c.type_params[0].name, "E");
        assert!(c.to_string().contains("Collection<E>"), "{}", c);
    }

    #[test]
    fn wildcard_unbounded() {
        let s = "Ljava/lang/Class<*>;";
        let t = parse_field(s).unwrap();
        assert_eq!(t.to_string(), "Class<?>");
    }

    #[test]
    fn nested_inner_generic() {
        // Map.Entry as top-level dotted inner: Ljava/util/Map<TK;TV;>.Entry;-style not
        // common; here we test the '$' form which is just part of the identifier.
        let s = "Ljava/util/Map$Entry<TK;TV;>;";
        let t = parse_field(s).unwrap();
        assert_eq!(t.to_string(), "Map$Entry<K, V>");
    }

    #[test]
    fn array_of_typevar() {
        let s = "[TT;";
        let t = parse_field(s).unwrap();
        assert_eq!(t.to_string(), "T[]");
    }

    #[test]
    fn list_iterator() {
        // List.iterator(): ()Ljava/util/Iterator<TE;>;
        let s = "()Ljava/util/Iterator<TE;>;";
        let m = parse_method(s).unwrap();
        assert!(m.to_string().contains("Iterator<E>"), "{}", m);
    }
}
