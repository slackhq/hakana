//! Lowers the syntax tree ([`crate::ast`]) to the analysis tree
//! ([`crate::aast`]).
//!
//! The analysis tree keeps the conventions Hakana's analyzer has always
//! relied on (they originate in HHVM's lowerer): explicit `Noop`s for empty
//! bodies, constructor-promoted parameters expanded into properties plus
//! assignments, `echo`/`unset`/`print` as calls, class members sorted by
//! kind, `MemberOf<..>` wrappers on enum class members, desugared context
//! polymorphism, and so on. `parse-bench diff` checks the result against
//! HHVM's own output file by file.

use bumpalo::Bump;
use smallvec::SmallVec;

use crate::aast::*;
use crate::aast_visitor::{AstParams, Node, Visitor, visit};
use crate::ast as n;
use crate::expr_tree;
use crate::{Comment as RawComment, CommentKind, LineIndex, Span, SyntaxError};

/// The file uses something the lowering cannot express; treated as a syntax
/// error at the given offset.
#[derive(Debug, Clone, Copy)]
pub struct LowerError {
    pub offset: u32,
    pub message: &'static str,
    /// Set when this is an error HHVM reports too, at the point where its
    /// own translation to the AST gives up on a declaration: where the
    /// text the error is about ends.
    pub hhvm_end: Option<u32>,
}

type R<T> = Result<T, LowerError>;

/// HHVM gives up on expressions nested deeper than this.
const EXPR_RECURSION_LIMIT: usize = 30_000;

/// Beyond this depth of genuinely nested (not chained) expressions the
/// native stack is at risk, so the file is rejected instead.
const MAX_NATIVE_DEPTH: usize = 2_000;

pub struct Lowered<'a> {
    pub program: Program<'a>,
    pub comments: ScouredComments,
    /// The file's line table, for turning positions into lines and columns.
    pub lines: LineIndex,
    /// The errors only found while lowering (those in the expression tree
    /// literals of the file, which take working out what the literals
    /// stand for, and a few others). Each comes with where it belongs
    /// among the other errors of HHVM's translation to its AST, which are
    /// in the order of the code they are about: after those that start
    /// before the offset given.
    pub errors: Vec<(u32, SyntaxError)>,
}

/// Where the expression being lowered stands, as far as expression tree
/// literals are concerned.
#[derive(Default)]
struct EtState<'a> {
    /// The enclosing function is `async`.
    in_async: bool,
    /// An `await` has been met in the function (or splice) being lowered.
    found_await: bool,
    /// Lowering the code of a literal, as opposed to code that is run where
    /// it stands (which the splices in a literal are).
    in_literal: bool,
    /// The DSL of the outermost literal around the expression.
    class: Option<Id<'a>>,
    /// The position of the body of the innermost literal around it.
    enclosing_pos: Option<Pos>,
    /// Lowering a splice at a place where `$$` is that of a pipe outside
    /// the literal. The literal hands it over under another name.
    rename_pipe_var: bool,
}

struct Lower<'s, 'a> {
    /// The syntax tree lives in its own arena (`'s`), which can be dropped
    /// once lowering is done; nothing of it may end up in the output.
    syntax: std::marker::PhantomData<&'s ()>,
    arena: &'a Bump,
    src: &'a str,
    lines: LineIndex,
    line_hint: usize,
    mode: Mode,
    /// Counter behind the `$$` of each pipe expression.
    local_id: isize,
    /// Type parameters in scope and whether they are reified.
    cls_generics: SmallVec<[(&'a str, bool); 4]>,
    fn_generics: SmallVec<[(&'a str, bool); 4]>,
    /// Lowering the body of a static method.
    in_static_method: bool,
    /// The class being lowered extends one that has type arguments.
    parent_maybe_reified: bool,
    depth: usize,
    /// Inside a function or class declaration (as opposed to top-level
    /// code); in `.hhi` files bodies in there are dropped.
    in_decl: bool,
    et: EtState<'a>,
    errors: Vec<(u32, SyntaxError)>,
    /// Lowering what was parsed of a file with a syntax error, for the
    /// errors that turns up (see [`errors_after_recovery`]).
    recovered: bool,
}

/// Lowers a parsed file. Pass `is_hhi` for files with an `.hhi` extension:
/// those that start with `<?hh` are declaration files, whose function
/// bodies and top-level statements are discarded.
pub fn lower<'s, 'a>(
    arena: &'a Bump,
    program: &n::Program<'s>,
    comments: &[RawComment],
    src: &'a str,
    is_hhi: bool,
) -> R<Lowered<'a>> {
    let has_open_tag = program
        .header
        .is_some_and(|h| h.span.end as usize > h.hashbang.len());
    let mode = if is_hhi && has_open_tag {
        Mode::Mhhi
    } else {
        Mode::Mstrict
    };
    let mut l = Lower {
        syntax: std::marker::PhantomData,
        arena,
        src,
        lines: LineIndex::new(src.as_bytes()),
        line_hint: 0,
        mode,
        local_id: 1,
        cls_generics: SmallVec::new(),
        fn_generics: SmallVec::new(),
        in_static_method: false,
        parent_maybe_reified: false,
        depth: 0,
        in_decl: false,
        et: EtState::default(),
        errors: Vec::new(),
        recovered: false,
    };
    let mut defs: Vec<Def<'a>> = Vec::with_capacity(program.defs.len() + 1);
    if let Some(header) = program.header.as_ref().filter(|_| mode != Mode::Mhhi) {
        let pos = l.pos(header.span);
        let markup = l.arena.alloc((pos, l.s(header.hashbang)));
        let stmt = l.arena.alloc(Stmt(pos, Stmt_::Markup(markup)));
        defs.push(Def::Stmt(stmt));
    }
    l.defs(program.defs, &mut defs)?;
    let comments = l.scour(comments);
    Ok(Lowered {
        program: Program(arena.alloc_slice_copy(&defs)),
        comments,
        lines: l.lines,
        errors: l.errors,
    })
}

/// The errors that lowering finds in what was parsed of a file with a
/// syntax error (see [`Lowered::errors`]): the parser carried on the way
/// HHVM's does, and HHVM translates the tree that results, declaration by
/// declaration. With `clean`, its translation has reported nothing so far,
/// which is when it also names what makes it give up on a declaration.
pub(crate) fn errors_after_recovery(
    program: &n::Program<'_>,
    src: &str,
    is_hhi: bool,
    mut clean: bool,
) -> Vec<(u32, SyntaxError)> {
    let has_open_tag = program
        .header
        .is_some_and(|h| h.span.end as usize > h.hashbang.len());
    let arena = Bump::new();
    // Nothing of the tree built here outlives this function.
    let mut l = Lower {
        syntax: std::marker::PhantomData,
        arena: &arena,
        src,
        lines: LineIndex::new(src.as_bytes()),
        line_hint: 0,
        mode: if is_hhi && has_open_tag {
            Mode::Mhhi
        } else {
            Mode::Mstrict
        },
        local_id: 1,
        cls_generics: SmallVec::new(),
        fn_generics: SmallVec::new(),
        in_static_method: false,
        parent_maybe_reified: false,
        depth: 0,
        in_decl: false,
        et: EtState::default(),
        errors: Vec::new(),
        recovered: true,
    };
    let mut defs = Vec::new();
    for def in program.defs {
        defs.clear();
        if let Err(error) = l.defs(std::slice::from_ref(def), &mut defs) {
            if let Some(end) = error.hhvm_end.filter(|_| clean && l.errors.is_empty()) {
                l.errors.push((
                    error.offset + 1,
                    SyntaxError {
                        start: error.offset,
                        end,
                        message: error.message.into(),
                    },
                ));
            }
            // HHVM starts afresh with the next declaration.
            l.et = EtState::default();
            l.depth = 0;
            l.in_decl = false;
            l.in_static_method = false;
            l.parent_maybe_reified = false;
            l.cls_generics.clear();
            l.fn_generics.clear();
        }
        clean &= l.errors.is_empty();
    }
    l.errors
}

fn err<T>(offset: u32, message: &'static str) -> R<T> {
    Err(LowerError {
        offset,
        message,
        hhvm_end: None,
    })
}

/// Fails the way HHVM's translation to its AST does, with its `message`
/// about the text at `pos`.
fn gives_up<T>(pos: Pos, message: &'static str) -> R<T> {
    Err(LowerError {
        offset: pos.start_offset() as u32,
        message,
        hhvm_end: Some(pos.end_offset() as u32),
    })
}

fn has_soft(attrs: &[UserAttribute<'_>]) -> bool {
    attrs.iter().any(|attr| attr.name.1 == "__Soft")
}

fn lookup(generics: &[(&str, bool)], name: &str) -> Option<bool> {
    generics
        .iter()
        .rev()
        .find(|(n, _)| *n == name)
        .map(|(_, reified)| *reified)
}

impl<'s, 'a> Lower<'s, 'a> {
    // ------------------------------------------------------------------
    // Arena helpers

    #[inline]
    fn a<T>(&self, x: T) -> &'a T {
        self.arena.alloc(x)
    }

    #[inline]
    fn slice<T: Copy>(&self, items: &[T]) -> &'a [T] {
        if items.is_empty() {
            &[]
        } else {
            self.arena.alloc_slice_copy(items)
        }
    }

    /// Maps `items` into a new arena slice.
    fn list<T, U: Copy>(
        &mut self,
        items: &[T],
        mut f: impl FnMut(&mut Self, &T) -> R<U>,
    ) -> R<&'a [U]> {
        let len = items.len();
        if len == 0 {
            return Ok(&[]);
        }
        let layout = std::alloc::Layout::array::<U>(len).unwrap();
        let ptr = self.arena.alloc_layout(layout).as_ptr() as *mut U;
        for (i, item) in items.iter().enumerate() {
            let value = f(self, item)?;
            // SAFETY: `ptr` points to room for `len` values of `U` and
            // `i < len`.
            unsafe { ptr.add(i).write(value) };
        }
        // SAFETY: all `len` elements were initialised above, `U: Copy` has
        // no destructor, and the arena keeps the memory alive for `'a`.
        Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
    }

    fn str(&self, s: std::fmt::Arguments<'_>) -> &'a str {
        bumpalo::format!(in self.arena, "{}", s).into_bump_str()
    }

    // ------------------------------------------------------------------
    // Positions

    /// `(line, offset of the start of that line, offset)`, for comments.
    #[inline]
    fn triple(&mut self, offset: u32) -> (usize, usize, usize) {
        let (line, bol) = self.lines.line_of_hinted(offset, &mut self.line_hint);
        (line as usize, bol as usize, offset as usize)
    }

    #[inline]
    fn pos(&mut self, span: Span) -> Pos {
        if span.is_none() {
            return Pos::NONE;
        }
        Pos::new(span.start, span.end)
    }

    #[inline]
    fn pos2(&mut self, start: u32, end: u32) -> Pos {
        self.pos(Span::new(start, end))
    }

    /// A string of the syntax tree as one the output may keep. Almost all
    /// of them are slices of the source, which outlives both trees; the few
    /// the parser built in its own arena (unescaped text) are copied.
    #[inline]
    fn s(&self, text: &'s str) -> &'a str {
        let source = self.src.as_bytes().as_ptr_range();
        let range = text.as_bytes().as_ptr_range();
        if source.start <= range.start && range.end <= source.end {
            let start = range.start as usize - source.start as usize;
            &self.src[start..start + text.len()]
        } else {
            self.arena.alloc_str(text)
        }
    }

    /// Like [`Lower::s`], for bytes.
    #[inline]
    fn b(&self, bytes: &'s [u8]) -> &'a [u8] {
        let source = self.src.as_bytes().as_ptr_range();
        let range = bytes.as_ptr_range();
        if source.start <= range.start && range.end <= source.end {
            let start = range.start as usize - source.start as usize;
            &self.src.as_bytes()[start..start + bytes.len()]
        } else {
            self.arena.alloc_slice_copy(bytes)
        }
    }

    fn id(&mut self, id: &n::Id<'s>) -> Id<'a> {
        Id(self.pos(id.span), self.s(id.name))
    }

    fn pstring(&mut self, id: &n::Id<'s>) -> Pstring<'a> {
        (self.pos(id.span), self.s(id.name))
    }

    fn doc(&mut self, doc: Option<Span>) -> Option<DocComment<'a>> {
        let src = self.src;
        doc.map(|span| (self.pos(span), span.text(src)))
    }

    fn reification(&self, name: &str) -> Option<bool> {
        lookup(&self.fn_generics, name).or_else(|| lookup(&self.cls_generics, name))
    }

    // ------------------------------------------------------------------
    // Comments

    /// Builds the comment and fixme tables. Comment positions cover the
    /// whole comment including its delimiters.
    fn scour(&mut self, comments: &[RawComment]) -> ScouredComments {
        let mut acc = ScouredComments::default();
        let src = self.src;
        for c in comments {
            match c.kind {
                CommentKind::Block => {
                    let text = &src[c.start as usize + 2..c.end as usize - 2];
                    // Historical quirks kept for compatibility: the start
                    // line is found by counting newlines back from the end,
                    // and for multi-line comments the start column is 1.
                    let end = self.triple(c.end - 2);
                    let newlines = text.bytes().filter(|b| *b == b'\n').count();
                    let mut start = end;
                    start.0 -= newlines;
                    start.2 -= text.len() + 2;
                    if newlines > 0 {
                        start.1 = start.2;
                    }
                    let mut end = self.triple(c.end - 1);
                    end.2 += 1;
                    acc.comments
                        .push((comment_pos(start, end), Comment::CmtBlock(text.to_string())));
                }
                CommentKind::Line => {
                    // A `//` comment after code on the same line has never
                    // been reported, and one that opens the trivia in front
                    // of a token is reported twice (see `first_leading`);
                    // comment-driven features were built on that behaviour.
                    if c.trailing {
                        continue;
                    }
                    let mut text = src[c.start as usize + 2..c.end as usize].to_string();
                    text.push('\n');
                    let mut start = self.triple(c.start + 2);
                    start.2 -= 2;
                    let end = self.triple(c.end - 1);
                    let pos = comment_pos(start, end);
                    if c.first_leading {
                        acc.comments.push((pos, Comment::CmtLine(text.clone())));
                    }
                    acc.comments.push((pos, Comment::CmtLine(text)));
                }
                CommentKind::FixMe | CommentKind::Ignore => {
                    let Some(code) = fixme_code(&src.as_bytes()[c.start as usize..c.end as usize])
                    else {
                        continue;
                    };
                    let line = self.lines.line_of(c.next_token).0 as isize;
                    let pos = self.pos2(c.start, c.end);
                    let table = if c.kind == CommentKind::Ignore {
                        &mut acc.ignores
                    } else if c.in_block {
                        &mut acc.fixmes
                    } else {
                        &mut acc.misuses
                    };
                    table.entry(line).or_default().insert(code, pos);
                }
                CommentKind::IgnoreError => {}
            }
        }
        acc.comments.sort_by_key(|(pos, _)| pos.start_offset());
        acc
    }

    // ------------------------------------------------------------------
    // Hints

    fn hint(&mut self, h: &n::Hint<'s>) -> R<Hint<'a>> {
        let pos = self.pos(h.span);
        let hint_ = self.hint_(h)?;
        let hint = Hint(pos, self.a(hint_));
        if self.in_static_method {
            self.check_reified_in_hint(pos, &hint);
        }
        Ok(hint)
    }

    /// In a static method the reified type parameters of the class are
    /// out of reach: HHVM says so about each mention of one in `hint`,
    /// with the position of the type it was translating.
    fn check_reified_in_hint(&mut self, pos: Pos, hint: &Hint<'a>) {
        struct Names<'a>(Vec<&'a str>);
        impl<'a> Visitor<'a> for Names<'a> {
            type Params = AstParams<(), ()>;

            fn object(&mut self) -> &mut dyn Visitor<'a, Params = Self::Params> {
                self
            }

            fn visit_hint(&mut self, c: &mut (), h: &'a Hint<'a>) -> Result<(), ()> {
                match h.1 {
                    Hint_::Happly(id, _) => self.0.push(id.1),
                    Hint_::Haccess(_, ids) => self.0.extend(ids.iter().map(|id| id.1)),
                    _ => {}
                }
                h.recurse(c, self)
            }
        }
        let mut names = Names(Vec::new());
        // The visitor wants a reference that lives as long as the tree.
        let hint: &'a Hint<'a> = self.a(*hint);
        let _ = names.visit_hint(&mut (), hint);
        for name in names.0 {
            self.check_reified_generic(pos, name);
        }
    }

    /// Reports the use of `name` at `pos` if it is a reified type
    /// parameter of the class and the code is in a static method.
    fn check_reified_generic(&mut self, pos: Pos, name: &str) {
        if self.in_static_method && lookup(&self.cls_generics, name) == Some(true) {
            self.report(
                pos,
                "You may not use reified generics of the class in a static method".to_string(),
            );
        }
    }

    fn hints(&mut self, hs: &[n::Hint<'s>]) -> R<&'a [Hint<'a>]> {
        self.list(hs, |l, h| l.hint(h))
    }

    fn opt_hint(&mut self, h: &Option<n::Hint<'s>>) -> R<Option<Hint<'a>>> {
        h.as_ref().map(|h| self.hint(h)).transpose()
    }

    /// The position of the name of a named type. The type only keeps where
    /// the name starts, which with its length is enough - except for a name
    /// written with blank space or comments between its parts
    /// (`Foo \Bar`): that one was put together, and its extent has to be
    /// read from the source again.
    fn hint_name_pos(&mut self, h: &n::Hint<'s>, name: &str) -> Pos {
        let source = self.src.as_bytes().as_ptr_range();
        if source.contains(&name.as_ptr()) {
            return self.pos(h.name_span(name));
        }
        self.spaced_name_pos(h.span.start, name)
    }

    #[cold]
    fn spaced_name_pos(&mut self, start: u32, name: &str) -> Pos {
        use crate::cst::trivia;
        let bytes = self.src.as_bytes();
        let mut at = start as usize;
        let mut rest = name.as_bytes();
        while !rest.is_empty() {
            while let Some(piece) = trivia::scan(bytes, at) {
                at += piece.width;
            }
            // One part of the name: a backslash, or what is between two.
            let part = if rest[0] == b'\\' {
                1
            } else {
                rest.iter().position(|&b| b == b'\\').unwrap_or(rest.len())
            };
            at += part;
            rest = &rest[part..];
        }
        self.pos2(start, at as u32)
    }

    fn hint_(&mut self, h: &n::Hint<'s>) -> R<Hint_<'a>> {
        Ok(match &h.kind {
            n::HintKind::Name(name) => {
                let name_pos = self.hint_name_pos(h, name);
                Hint_::Happly(Id(name_pos, self.s(name)), &[])
            }
            n::HintKind::Generic(g) => {
                let name_pos = self.hint_name_pos(h, g.name);
                Hint_::Happly(Id(name_pos, self.s(g.name)), self.hints(g.args)?)
            }
            n::HintKind::Option(inner) => Hint_::Hoption(self.hint(inner)?),
            n::HintKind::Like(inner) => Hint_::Hlike(self.hint(inner)?),
            n::HintKind::Soft(inner) => Hint_::Hsoft(self.hint(inner)?),
            n::HintKind::Wildcard => Hint_::Hwildcard,
            n::HintKind::Var(name) => Hint_::Hvar(self.s(name)),
            n::HintKind::FunContext(name) => Hint_::HfunContext(self.s(name)),
            n::HintKind::Union(hs) => Hint_::Hunion(self.hints(hs)?),
            n::HintKind::Intersection(hs) => Hint_::Hintersection(self.hints(hs)?),
            n::HintKind::Refinement(r) => {
                let base = self.hint(&r.base)?;
                let members = self.list(r.members, |l, m| {
                    let name = l.id(&m.name);
                    let exact = match &m.exact {
                        Some(h) => Some(l.hint(h)?),
                        None => None,
                    };
                    let mut lower: SmallVec<[Hint<'a>; 1]> = SmallVec::new();
                    let mut upper: SmallVec<[Hint<'a>; 1]> = SmallVec::new();
                    for (kind, hint) in m.bounds {
                        let hint = l.hint(hint)?;
                        match kind {
                            n::ConstraintKind::As => upper.push(hint),
                            n::ConstraintKind::Super => lower.push(hint),
                            n::ConstraintKind::Eq => {}
                        }
                    }
                    Ok(if m.is_ctx {
                        Refinement::Rctx(
                            name,
                            match exact {
                                Some(hint) => CtxRefinement::CRexact(hint),
                                None => CtxRefinement::CRloose(CtxRefinementBounds {
                                    lower: lower.first().copied(),
                                    upper: upper.first().copied(),
                                }),
                            },
                        )
                    } else {
                        Refinement::Rtype(
                            name,
                            match exact {
                                Some(hint) => TypeRefinement::TRexact(hint),
                                None => TypeRefinement::TRloose(TypeRefinementBounds {
                                    lower: l.slice(&lower),
                                    upper: l.slice(&upper),
                                }),
                            },
                        )
                    })
                })?;
                Hint_::Hrefinement(base, members)
            }
            n::HintKind::ClassPtr(kind, inner) => Hint_::HclassPtr(
                match kind {
                    n::ClassPtrKind::Class => ClassPtrKind::CKclass,
                    n::ClassPtrKind::Enum => ClassPtrKind::CKenum,
                },
                self.hint(inner)?,
            ),
            n::HintKind::Access(access) => {
                let root = self.hint(&access.root)?;
                Hint_::Haccess(root, self.list(access.names, |l, id| Ok(l.id(id)))?)
            }
            n::HintKind::Tuple(t) => {
                let required = self.hints(t.required)?;
                let extra = match &t.splat {
                    Some(h) => TupleExtra::Hsplat(self.hint(h)?),
                    None => TupleExtra::Hextra(TupleExtraInfo {
                        optional: self.hints(t.optional)?,
                        variadic: self.opt_hint(&t.variadic)?,
                    }),
                };
                Hint_::Htuple(TupleInfo { required, extra })
            }
            n::HintKind::Shape(shape) => {
                let field_map = self.list(shape.fields, |l, element| {
                    Ok(match element {
                        n::ShapeHintElement::Field(f) => {
                            let name = l.shape_key(&f.key);
                            let hint = l.hint(&f.hint)?;
                            ShapeElement::SEField(ShapeFieldInfo {
                                optional: f.optional,
                                hint,
                                name,
                            })
                        }
                        n::ShapeHintElement::Splat(hint) => ShapeElement::SESplat(l.hint(hint)?),
                        // "Not present" is an optional field that can hold nothing.
                        n::ShapeHintElement::Absent { span, key } => {
                            let name = l.shape_key(key);
                            let hint = Hint(l.pos(*span), l.a(Hint_::Hnothing));
                            ShapeElement::SEField(ShapeFieldInfo {
                                optional: true,
                                hint,
                                name,
                            })
                        }
                    })
                })?;
                let unknown_fields_type = match &shape.unknown_fields {
                    Some(hint) => Some(self.hint(hint)?),
                    None => None,
                };
                Hint_::Hshape(NastShapeInfo {
                    allows_unknown_fields: shape.open,
                    field_map,
                    unknown_fields_type,
                })
            }
            n::HintKind::Fun(f) => {
                use n::fun_hint_flags as ff;
                let tparams = self.hint_tparams(f.tparams)?;
                let mut param_tys: SmallVec<[Hint<'a>; 8]> = SmallVec::new();
                let mut param_info: SmallVec<[Option<HfParamInfo<'a>>; 8]> = SmallVec::new();
                let mut variadic_ty = None;
                let mut named_variadic_ty = None;
                for p in f.params {
                    if p.flags & ff::VARIADIC != 0 {
                        let hint = Some(self.hint(&p.hint)?);
                        if p.named.is_none() {
                            if variadic_ty.is_none() {
                                variadic_ty = hint;
                            }
                        } else if named_variadic_ty.is_none() {
                            named_variadic_ty = hint;
                        }
                        continue;
                    }
                    let kind = match p.inout {
                        Some(span) => ParamKind::Pinout(self.pos(span)),
                        None => ParamKind::Pnormal,
                    };
                    param_info.push(Some(HfParamInfo {
                        kind,
                        readonlyness: (p.flags & ff::READONLY != 0)
                            .then_some(ReadonlyKind::Readonly),
                        optional: (p.flags & ff::OPTIONAL != 0).then_some(OptionalKind::Optional),
                        splat: (p.flags & ff::SPLAT != 0).then_some(SplatKind::Splat),
                        named: p.named.map(|name| self.s(name)),
                    }));
                    param_tys.push(self.hint(&p.hint)?);
                }
                let ctxs = self.contexts(&f.ctxs)?;
                if let Some(Contexts(_, hints)) = &ctxs {
                    for hint in hints.iter() {
                        if self.is_polymorphic_context(hint, true) {
                            self.report(
                                hint.0,
                                "A closure type hint cannot have a polymorphic context".to_string(),
                            );
                        }
                    }
                }
                let return_ty = self.hint(&f.ret)?;
                Hint_::Hfun(HintFun {
                    is_readonly: f.is_readonly.then_some(ReadonlyKind::Readonly),
                    tparams,
                    param_tys: self.slice(&param_tys),
                    param_info: self.slice(&param_info),
                    variadic_ty,
                    named_variadic_ty,
                    ctxs,
                    return_ty,
                    is_readonly_return: f.readonly_ret.then_some(ReadonlyKind::Readonly),
                })
            }
        })
    }

    fn shape_key(&mut self, key: &n::ShapeKey<'s>) -> ShapeFieldName<'a> {
        match key {
            n::ShapeKey::Str { span, value } => {
                ShapeFieldName::SFlitStr((self.pos(*span), BStr::new(self.b(value))))
            }
            n::ShapeKey::ClassConst((class, name)) => {
                ShapeFieldName::SFclassConst(self.id(class), self.pstring(name))
            }
            n::ShapeKey::Nameof { class, .. } => ShapeFieldName::SFclassname(self.id(class)),
        }
    }

    fn contexts(&mut self, ctxs: &Option<n::Contexts<'s>>) -> R<Option<Contexts<'a>>> {
        let Some(ctxs) = ctxs else {
            return Ok(None);
        };
        let hints = self.hints(ctxs.hints)?;
        Ok(Some(Contexts(self.pos(ctxs.span), hints)))
    }

    fn constraint_kind(kind: n::ConstraintKind) -> ConstraintKind {
        match kind {
            n::ConstraintKind::As => ConstraintKind::ConstraintAs,
            n::ConstraintKind::Eq => ConstraintKind::ConstraintEq,
            n::ConstraintKind::Super => ConstraintKind::ConstraintSuper,
        }
    }

    fn tparams(
        &mut self,
        tparams: &[n::Tparam<'s>],
        is_class: bool,
    ) -> R<SmallVec<[Tparam<'a>; 4]>> {
        let mut out = SmallVec::new();
        for t in tparams {
            let user_attributes = self.attrs(t.attrs)?;
            if is_class {
                self.cls_generics.push((self.s(t.name.name), t.reified));
            } else {
                self.fn_generics.push((self.s(t.name.name), t.reified));
            }
            let reified = match (t.reified, has_soft(&user_attributes)) {
                (true, true) => ReifyKind::SoftReified,
                (true, false) => ReifyKind::Reified,
                _ => ReifyKind::Erased,
            };
            let name = self.id(&t.name);
            let constraints = self.list(t.constraints, |l, (kind, hint)| {
                Ok((Self::constraint_kind(*kind), l.hint(hint)?))
            })?;
            out.push(Tparam {
                variance: match t.variance {
                    n::Variance::Covariant => Variance::Covariant,
                    n::Variance::Contravariant => Variance::Contravariant,
                    n::Variance::Invariant => Variance::Invariant,
                },
                name,
                constraints,
                reified,
                user_attributes,
            });
        }
        Ok(out)
    }

    /// The type parameters of a function type or function expression.
    fn hint_tparams(&mut self, tparams: &[n::Tparam<'s>]) -> R<&'a [HintTparam<'a>]> {
        self.list(tparams, |l, t| {
            l.fn_generics.push((l.s(t.name.name), false));
            let attrs = l.attrs(t.attrs)?;
            let user_attributes = l.list(attrs.0, |_, attr| Ok(attr.name))?;
            let name = l.id(&t.name);
            let constraints = l.list(t.constraints, |l, (kind, hint)| {
                Ok((Self::constraint_kind(*kind), l.hint(hint)?))
            })?;
            Ok(HintTparam {
                name,
                user_attributes,
                constraints,
            })
        })
    }

    fn where_constraints(
        &mut self,
        ws: &[n::WhereConstraint<'s>],
    ) -> R<SmallVec<[WhereConstraintHint<'a>; 2]>> {
        let mut out = SmallVec::new();
        for w in ws {
            let left = self.hint(&w.left)?;
            let right = self.hint(&w.right)?;
            out.push(WhereConstraintHint(
                left,
                Self::constraint_kind(w.kind),
                right,
            ));
        }
        Ok(out)
    }

    fn attrs(&mut self, attrs: &[n::Attribute<'s>]) -> R<UserAttributes<'a>> {
        Ok(UserAttributes(self.list(attrs, |l, a| {
            let name = l.id(&a.name);
            let params = l.exprs(a.args)?;
            Ok(UserAttribute { name, params })
        })?))
    }

    fn soften(&self, attrs: &[UserAttribute<'a>], hint: Hint<'a>) -> Hint<'a> {
        if has_soft(attrs) {
            Hint(hint.0, self.a(Hint_::Hsoft(hint)))
        } else {
            hint
        }
    }

    // ------------------------------------------------------------------
    // Expressions

    fn exprs(&mut self, es: &[n::Expr<'s>]) -> R<&'a [Expr<'a>]> {
        self.list(es, |l, e| l.expr(e))
    }

    fn opt_expr(&mut self, e: &Option<n::Expr<'s>>) -> R<Option<Expr<'a>>> {
        e.as_ref().map(|e| self.expr(e)).transpose()
    }

    fn targs(&mut self, hs: &[n::Hint<'s>]) -> R<&'a [Targ<'a>]> {
        self.list(hs, |l, h| Ok(Targ((), l.hint(h)?)))
    }

    fn args(&mut self, args: &[n::Expr<'s>]) -> R<&'a [Argument<'a>]> {
        self.list(args, |l, a| {
            Ok(match &a.kind {
                n::ExprKind::InoutArg(inner) => {
                    // The keyword sits at the start of the argument.
                    let pos = l.pos2(a.span.start, a.span.start + "inout".len() as u32);
                    Argument::Ainout(pos, l.expr(inner)?)
                }
                n::ExprKind::NamedArg(named) => {
                    let name = l.id(&named.name);
                    Argument::Anamed(name, l.expr(&named.value)?)
                }
                _ => Argument::Anormal(l.expr(a)?),
            })
        })
    }

    fn afield(&mut self, f: &n::Afield<'s>) -> R<Afield<'a>> {
        Ok(match f {
            n::Afield::Value(v) => Afield::AFvalue(self.expr(v)?),
            n::Afield::KeyValue(k, v) => {
                let k = self.expr(k)?;
                Afield::AFkvalue(k, self.expr(v)?)
            }
        })
    }

    /// Turns the expression on the left of `::` (or after `new`) into a
    /// class reference. `pos` is the position of the whole access.
    fn class_id(&mut self, pos: Pos, e: Expr<'a>) -> R<ClassId<'a>> {
        let Expr(_, epos, expr_) = e;
        Ok(match expr_ {
            Expr_::Id(name) => match self.reification(name.1) {
                Some(true) => ClassId((), pos, ClassId_::CIreified(*name)),
                Some(false) => {
                    return gives_up(
                        epos,
                        "Erased generics can only be used in type hints because they do not exist at runtime",
                    );
                }
                None if name.1.eq_ignore_ascii_case("self") => ClassId((), epos, ClassId_::CIself),
                None if name.1.eq_ignore_ascii_case("parent") => {
                    // Kept as an identifier: name resolution and
                    // forwarding-call handling look at the `parent` id.
                    let id = self.a(Id(epos, "parent"));
                    ClassId((), epos, ClassId_::CIexpr(Expr((), epos, Expr_::Id(id))))
                }
                None => ClassId((), pos, ClassId_::CIexpr(e)),
            },
            _ => ClassId((), pos, ClassId_::CIexpr(e)),
        })
    }

    fn expr(&mut self, e: &n::Expr<'s>) -> R<Expr<'a>> {
        // Operator chains (`a . b . c . ...`) lean left and can be thousands
        // of operands long; walk down the spine iteratively so they cost no
        // stack.
        if let n::ExprKind::Binop(_) = e.kind {
            let mut spine: SmallVec<[&n::Expr<'s>; 16]> = SmallVec::new();
            let mut cur = e;
            while let n::ExprKind::Binop(b) = &cur.kind {
                spine.push(cur);
                cur = &b.lhs;
            }
            if self.depth + spine.len() >= EXPR_RECURSION_LIMIT {
                return err(e.span.start, "expression nested too deeply");
            }
            let mut lhs = self.expr(cur)?;
            for node in spine.iter().rev() {
                let n::ExprKind::Binop(b) = &node.kind else {
                    unreachable!()
                };
                let rhs = self.expr(&b.rhs)?;
                let pos = self.pos(node.span);
                let binop = self.a(Binop {
                    bop: bop(b.op),
                    lhs,
                    rhs,
                });
                lhs = Expr((), pos, Expr_::Binop(binop));
            }
            return Ok(lhs);
        }
        if self.depth >= MAX_NATIVE_DEPTH {
            return err(e.span.start, "expression nested too deeply");
        }
        self.depth += 1;
        let pos = self.pos(e.span);
        let r = self.expr_(e, pos);
        self.depth -= 1;
        Ok(Expr((), pos, r?))
    }

    /// Lowers something that is assigned to: a `shape(...)` or `tuple(...)`
    /// there is a destructuring pattern.
    fn lvalue(&mut self, e: &n::Expr<'s>) -> R<Expr<'a>> {
        if !matches!(e.kind, n::ExprKind::Shape(..) | n::ExprKind::Tuple(..)) {
            return self.expr(e);
        }
        // `shape` and `tuple` are both five letters long.
        let pos = self.pos2(e.span.start, e.span.start + 5);
        let expr_ = match e.kind {
            n::ExprKind::Shape(fields, rest) => {
                let mut out: SmallVec<[DestructureShapeField<'a>; 8]> = SmallVec::new();
                for field in fields {
                    // A splat has no place in a pattern (and is reported).
                    let Some(key) = field.key() else {
                        continue;
                    };
                    let name = self.shape_key(key);
                    let target = self.destructure_target(field.value())?;
                    out.push(DestructureShapeField {
                        optional: field.is_optional(),
                        name,
                        target,
                    });
                }
                Expr_::DestructureShape(self.a(DestructureShape {
                    pos,
                    fields: self.slice(&out),
                    ellipsis: rest.is_some(),
                }))
            }
            n::ExprKind::Tuple(items, rest) => {
                let mut seen_optional = false;
                let entries = self.list(items, |l, item| {
                    let (optional, target) = match &item.kind {
                        n::ExprKind::OptionalEntry(inner) => (true, *inner),
                        _ => (false, item),
                    };
                    // Once one entry is optional, those after it are too.
                    seen_optional |= optional;
                    Ok(DestructureTupleEntry {
                        optional: seen_optional,
                        target: l.destructure_target(target)?,
                    })
                })?;
                Expr_::DestructureTuple(self.a(DestructureTuple {
                    pos,
                    entries,
                    ellipsis: rest.is_some(),
                }))
            }
            _ => unreachable!(),
        };
        Ok(Expr((), self.pos(e.span), expr_))
    }

    /// What a pattern binds a field or entry to. Anything but a variable,
    /// `_` or a nested pattern is an error and discards the value.
    fn destructure_target(&mut self, e: &n::Expr<'s>) -> R<DestructureTarget<'a>> {
        let Expr(_, pos, expr_) = self.lvalue(e)?;
        Ok(DestructureTarget(
            (),
            (),
            match expr_ {
                Expr_::Lvar(lid) => DestructureTarget_::DtLvar(*lid),
                Expr_::DestructureShape(shape) => DestructureTarget_::DtShape(*shape),
                Expr_::DestructureTuple(tuple) => DestructureTarget_::DtTuple(*tuple),
                _ => DestructureTarget_::DtWildcard(pos),
            },
        ))
    }

    /// The translation of an expression is spread over a function per kind
    /// of expression (`expr_leaf` and so on), so that an expression nested
    /// in another only has the stack frame of its own kind beneath it.
    fn expr_(&mut self, e: &n::Expr<'s>, pos: Pos) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        match &e.kind {
            // HHVM translates the operand and has nothing for the whole.
            K::Dollar(operand) => {
                self.expr(operand)?;
                Ok(Expr_::Omitted)
            }
            K::Null
            | K::True
            | K::False
            | K::Omitted
            | K::Lvar(..)
            | K::Id(..)
            | K::Int(..)
            | K::Float(..)
            | K::String(..)
            | K::EnumClassLabel(..)
            | K::Package(..)
            | K::InoutArg(..)
            | K::NamedArg(..)
            | K::Nameof(..) => self.expr_leaf(e, pos),
            K::String2(..)
            | K::PrefixedString(..)
            | K::Shape(..)
            | K::ValCollection(..)
            | K::KeyValCollection(..)
            | K::Collection(..)
            | K::Tuple(..)
            | K::OptionalEntry(..)
            | K::List(..)
            | K::Xml(..) => self.expr_literal(e),
            K::Clone(..)
            | K::Await(..)
            | K::Delay(..)
            | K::Readonly(..)
            | K::Print(..)
            | K::Yield(..)
            | K::Cast(..)
            | K::Unop(..)
            | K::Import(..)
            | K::Is(..)
            | K::As(..)
            | K::NullableAs(..)
            | K::Upcast(..) => self.expr_unary(e, pos),
            K::ArrayGet(..) | K::ObjGet(..) | K::ClassGet(..) | K::ClassConst(..) => {
                self.expr_access(e, pos)
            }
            K::Call(..) | K::FunctionPointer(..) | K::New(..) => self.expr_call(e, pos),
            K::Binop(..) | K::Assign(..) | K::Pipe(..) | K::Eif(..) => self.expr_binary(e, pos),
            K::ExpressionTree(..)
            | K::EtSplice(..)
            | K::Efun(..)
            | K::Lfun(..)
            | K::AsyncBlock(..) => self.expr_function(e, pos),
        }
    }

    /// Names, literals that are one token and the like.
    #[inline(never)]
    fn expr_leaf(&mut self, e: &n::Expr<'s>, pos: Pos) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        Ok(match &e.kind {
            K::Null => Expr_::Null,
            K::True => Expr_::True,
            K::False => Expr_::False,
            // An expression that is not there (see the parser's
            // `repair.rs`) is `null` to HHVM.
            K::Omitted if self.recovered && e.span.start == e.span.end => Expr_::Null,
            K::Omitted => Expr_::Omitted,
            K::Lvar(name) => {
                let name = if self.et.rename_pipe_var && *name == "$$" {
                    expr_tree::DOLLARDOLLAR_TMP_VAR
                } else {
                    self.s(name)
                };
                Expr_::Lvar(self.a(Lid(pos, (0, name))))
            }
            K::Id(name) => Expr_::Id(self.a(Id(pos, self.s(name)))),
            K::Int(s) => Expr_::Int(self.s(s)),
            K::Float(s) => Expr_::Float(self.s(s)),
            K::String(v) => Expr_::String(BStr::new(self.b(v))),
            K::EnumClassLabel(l) => {
                let class = l.class.as_ref().map(|c| self.id(c));
                Expr_::EnumClassLabel(self.a((class, self.s(l.label))))
            }
            K::InoutArg(_) | K::NamedArg(_) => {
                return err(e.span.start, "argument modifier outside a call");
            }
            K::Package(name) => {
                let name = self.id(name);
                Expr_::Package(self.a(name))
            }
            K::Nameof(target) => {
                let target = self.expr(target)?;
                let target_pos = target.1;
                let kind = match target.2 {
                    Expr_::Id(id) if self.reification(id.1).is_some() => {
                        return gives_up(target_pos, "`nameof` cannot be used with a generic type");
                    }
                    Expr_::Id(id) if id.1.eq_ignore_ascii_case("self") => ClassId_::CIself,
                    Expr_::Id(id) if id.1.eq_ignore_ascii_case("parent") => {
                        let id = self.a(Id(target_pos, "parent"));
                        ClassId_::CIexpr(Expr((), target_pos, Expr_::Id(id)))
                    }
                    _ => ClassId_::CIexpr(target),
                };
                Expr_::Nameof(self.a(ClassId((), target_pos, kind)))
            }
            _ => unreachable!("not one of these"),
        })
    }

    /// Literals with expressions in them.
    #[inline(never)]
    fn expr_literal(&mut self, e: &n::Expr<'s>) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        Ok(match &e.kind {
            K::String2(parts) => Expr_::String2(self.exprs(parts)?),
            K::PrefixedString(p) => {
                let inner = self.expr(&p.expr)?;
                Expr_::PrefixedString(self.a((self.s(p.prefix), inner)))
            }
            K::Shape(fields, _) => Expr_::Shape(self.list(fields, |l, f| {
                Ok(match f.key() {
                    Some(key) => {
                        let name = l.shape_key(key);
                        ShapeExprField::SFField(name, l.expr(f.value())?)
                    }
                    None => ShapeExprField::SFSplat(l.expr(f.value())?),
                })
            })?),
            K::ValCollection(c) => {
                let targ = match &c.targ {
                    Some(h) => Some(Targ((), self.hint(h)?)),
                    None => None,
                };
                let kind = match c.kind {
                    n::VcKind::Vec => VcKind::Vec,
                    n::VcKind::Keyset => VcKind::Keyset,
                };
                let kw = self.pos(c.keyword);
                let items = self.exprs(c.items)?;
                Expr_::ValCollection(self.a(((kw, kind), targ, items)))
            }
            K::KeyValCollection(c) => {
                let targs = match &c.targs {
                    Some((k, v)) => {
                        let k = Targ((), self.hint(k)?);
                        Some((k, Targ((), self.hint(v)?)))
                    }
                    None => None,
                };
                let kw = self.pos(c.keyword);
                let fields = self.list(c.fields, |l, (k, v)| {
                    let k = l.expr(k)?;
                    Ok(Field(k, l.expr(v)?))
                })?;
                Expr_::KeyValCollection(self.a(((kw, KvcKind::Dict), targs, fields)))
            }
            K::Collection(c) => {
                let hints = self.hints(c.targs)?;
                let targs = match hints {
                    [k, v] => Some(CollectionTarg::CollectionTKV(Targ((), *k), Targ((), *v))),
                    [v] => Some(CollectionTarg::CollectionTV(Targ((), *v))),
                    _ => None,
                };
                let name = self.id(&c.name);
                let fields = self.list(c.fields, |l, f| l.afield(f))?;
                Expr_::Collection(self.a((name, targs, fields)))
            }
            K::Tuple(items, _) => Expr_::Tuple(self.exprs(items)?),
            // Only a pattern may have one, and patterns are lowered by
            // `lvalue`; HHVM has no tree for anything else.
            K::OptionalEntry(inner) => self.expr(inner)?.2,
            K::List(items) => Expr_::List(self.exprs(items)?),
            K::Xml(xml) => {
                let name_pos = self.pos(xml.name.span);
                let attrs = self.list(xml.attrs, |l, a| {
                    Ok(match a {
                        n::XhpAttribute::Simple(name, value) => {
                            let name = l.pstring(name);
                            // Braced attribute values are not evaluated in
                            // declaration files.
                            let expr = if l.mode == Mode::Mhhi
                                && l.src.as_bytes()[..value.span.start as usize]
                                    .iter()
                                    .rev()
                                    .find(|b| !b.is_ascii_whitespace())
                                    == Some(&b'{')
                            {
                                Expr((), Pos::NONE, Expr_::Null)
                            } else {
                                l.expr(value)?
                            };
                            XhpAttribute::XhpSimple(XhpSimple {
                                name,
                                type_: (),
                                expr,
                            })
                        }
                        n::XhpAttribute::Spread(value) => XhpAttribute::XhpSpread(l.expr(value)?),
                    })
                })?;
                let children = self.exprs(xml.children)?;
                let name = Id(name_pos, self.str(format_args!(":{}", xml.name.name)));
                Expr_::Xml(self.a((name, attrs, children)))
            }
            _ => unreachable!("not one of these"),
        })
    }

    /// An operator and its one operand.
    #[inline(never)]
    fn expr_unary(&mut self, e: &n::Expr<'s>, pos: Pos) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        Ok(match &e.kind {
            K::Clone(inner) => {
                let inner = self.expr(inner)?;
                Expr_::Clone(self.a(inner))
            }
            K::Await(inner) => {
                let inner = self.expr(inner)?;
                self.et.found_await = true;
                Expr_::Await(self.a(inner))
            }
            K::Delay(inner) => {
                let inner = self.expr(inner)?;
                Expr_::Delay(self.a(inner))
            }
            K::Readonly(inner) => {
                // `readonly` in front of a closure marks the closure itself.
                let inner = self.expr(inner)?;
                match inner.2 {
                    Expr_::Efun(efun) if efun.fun.readonly_this.is_none() => {
                        let mut efun = *efun;
                        efun.fun.readonly_this = Some(ReadonlyKind::Readonly);
                        Expr_::Efun(self.a(efun))
                    }
                    Expr_::Lfun(l) if l.0.readonly_this.is_none() => {
                        let mut l = *l;
                        l.0.readonly_this = Some(ReadonlyKind::Readonly);
                        Expr_::Lfun(self.a(l))
                    }
                    _ => Expr_::ReadonlyExpr(self.a(inner)),
                }
            }
            K::Print(inner) => {
                let arg = self.expr(inner)?;
                let func = Expr((), pos, Expr_::Id(self.a(Id(pos, "echo"))));
                Expr_::Call(self.a(CallExpr {
                    func,
                    targs: &[],
                    args: self.slice(&[Argument::Anormal(arg)]),
                    unpacked_arg: None,
                }))
            }
            K::Yield(f) => {
                let f = self.afield(f)?;
                Expr_::Yield(self.a(f))
            }
            K::Cast(c) => {
                let hint = self.hint(&c.hint)?;
                let inner = self.expr(&c.expr)?;
                Expr_::Cast(self.a((hint, inner)))
            }
            K::Unop(op, inner) => {
                let op = match op {
                    n::Uop::Tilde => Uop::Utild,
                    n::Uop::Not => Uop::Unot,
                    n::Uop::Plus => Uop::Uplus,
                    n::Uop::Minus => Uop::Uminus,
                    n::Uop::Incr => Uop::Uincr,
                    n::Uop::Decr => Uop::Udecr,
                    n::Uop::PostIncr => Uop::Upincr,
                    n::Uop::PostDecr => Uop::Updecr,
                };
                let inner = self.expr(inner)?;
                Expr_::Unop(self.a((op, inner)))
            }
            K::Is(t) => {
                let inner = self.expr(&t.expr)?;
                let hint = self.hint(&t.hint)?;
                Expr_::Is(self.a((inner, hint)))
            }
            K::As(t) | K::NullableAs(t) => {
                let expr = self.expr(&t.expr)?;
                let hint = self.hint(&t.hint)?;
                Expr_::As(self.a(As_ {
                    expr,
                    hint,
                    is_nullable: matches!(e.kind, K::NullableAs(_)),
                    enforce_deep: true,
                }))
            }
            K::Upcast(t) => {
                let inner = self.expr(&t.expr)?;
                let hint = self.hint(&t.hint)?;
                Expr_::Upcast(self.a((inner, hint)))
            }
            K::Import(flavor, inner) => {
                let inner = self.expr(inner)?;
                Expr_::Import(self.a((
                    match flavor {
                        n::ImportFlavor::Include => ImportFlavor::Include,
                        n::ImportFlavor::Require => ImportFlavor::Require,
                        n::ImportFlavor::IncludeOnce => ImportFlavor::IncludeOnce,
                        n::ImportFlavor::RequireOnce => ImportFlavor::RequireOnce,
                    },
                    inner,
                )))
            }
            _ => unreachable!("not one of these"),
        })
    }

    /// Elements and members.
    #[inline(never)]
    fn expr_access(&mut self, e: &n::Expr<'s>, pos: Pos) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        Ok(match &e.kind {
            K::ArrayGet(g) => {
                let base = self.expr(&g.base)?;
                let index = self.opt_expr(&g.index)?;
                Expr_::ArrayGet(self.a((base, index)))
            }
            K::ObjGet(g) => {
                let object = self.expr(&g.object)?;
                let member = self.expr(&g.member)?;
                Expr_::ObjGet(self.a((
                    object,
                    member,
                    if g.nullsafe {
                        OperatorNullFlavor::Nullsafe
                    } else {
                        OperatorNullFlavor::Regular
                    },
                    if g.is_method {
                        PropOrMethod::IsMethod
                    } else {
                        PropOrMethod::IsProp
                    },
                )))
            }
            K::ClassGet(a) => {
                let class = self.expr(&a.class)?;
                if let Expr_::Id(id) = class.2 {
                    self.check_reified_generic(pos, id.1);
                }
                let class_id = self.class_id(pos, class)?;
                let name = self.pstring(&a.name);
                Expr_::ClassGet(self.a((class_id, name, PropOrMethod::IsProp)))
            }
            K::ClassConst(a) => {
                let class = self.expr(&a.class)?;
                if let Expr_::Id(id) = class.2 {
                    self.check_reified_generic(pos, id.1);
                }
                let class_id = self.class_id(pos, class)?;
                let name = self.pstring(&a.name);
                Expr_::ClassConst(self.a((class_id, name)))
            }
            _ => unreachable!("not one of these"),
        })
    }

    /// Calls and object creation.
    #[inline(never)]
    fn expr_call(&mut self, e: &n::Expr<'s>, pos: Pos) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        Ok(match &e.kind {
            K::Call(c) => {
                let targs = self.targs(c.targs)?;
                let func = self.expr(&c.func)?;
                let args = self.args(c.args)?;
                let unpacked_arg = match c.unpacked {
                    Some(e) => Some(self.expr(e)?),
                    None => None,
                };
                Expr_::Call(self.a(CallExpr {
                    func,
                    targs,
                    args,
                    unpacked_arg,
                }))
            }
            K::FunctionPointer(fp) => {
                let targs = self.targs(fp.targs)?;
                let recv = self.expr(&fp.target)?;
                let id = match recv.2 {
                    Expr_::Id(id) => FunctionPtrId::FPId(*id),
                    Expr_::ClassConst(c) => {
                        let (class_id, name) = *c;
                        match &class_id.2 {
                            ClassId_::CIexpr(Expr(_, _, Expr_::Id(_)))
                            | ClassId_::CIself
                            | ClassId_::CIparent
                            | ClassId_::CIreified(_) => {}
                            _ => return err(e.span.start, "invalid function pointer target"),
                        }
                        FunctionPtrId::FPClassConst(class_id, name)
                    }
                    _ => return err(e.span.start, "invalid function pointer target"),
                };
                Expr_::FunctionPointer(self.a((id, targs, FunctionPointerSource::Code)))
            }
            K::New(new) => {
                let args = self.args(new.args)?;
                let unpacked = match new.unpacked {
                    Some(e) => Some(self.expr(e)?),
                    None => None,
                };
                let targs = self.targs(new.targs)?;
                let class = self.expr(&new.class)?;
                if let Expr_::Id(id) = class.2 {
                    // HHVM's node for this starts at the class.
                    let call_pos = self.pos(Span::new(new.class.span.start, e.span.end));
                    self.check_reified_generic(call_pos, id.1);
                    let reified = if id.1.eq_ignore_ascii_case("self") {
                        self.cls_generics.iter().any(|(_, reified)| *reified)
                    } else {
                        id.1.eq_ignore_ascii_case("parent") && self.parent_maybe_reified
                    };
                    if self.in_static_method && reified {
                        self.report(
                            call_pos,
                            "You may not use object creation for potentially reified `self` or `parent` from a static method".to_string(),
                        );
                    }
                }
                let class_id = self.class_id(pos, class)?;
                if matches!(class_id.2, ClassId_::CIreified(_)) && !targs.is_empty() {
                    // HHVM says so about the arguments of the constructor.
                    let (start, end) = self.new_arguments_extent(new, e.span.end);
                    let pos = self.pos2(start, end);
                    self.report(
                        pos,
                        "Type arguments are not allowed in this position".to_string(),
                    );
                }
                Expr_::New(self.a((class_id, targs, args, unpacked, ())))
            }
            _ => unreachable!("not one of these"),
        })
    }

    /// Operators with two or three operands.
    #[inline(never)]
    fn expr_binary(&mut self, e: &n::Expr<'s>, pos: Pos) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        Ok(match &e.kind {
            K::Binop(_) => unreachable!("handled in `expr`"),
            K::Assign(a) => {
                let lhs = if a.op.is_none() {
                    self.lvalue(&a.lhs)?
                } else {
                    self.expr(&a.lhs)?
                };
                let rhs = self.expr(&a.rhs)?;
                Expr_::Assign(self.a((lhs, a.op.map(bop), rhs)))
            }
            K::Pipe(p) => {
                let lhs = self.expr(&p.lhs)?;
                // On the right, `$$` is this pipe's.
                let rename = std::mem::replace(&mut self.et.rename_pipe_var, false);
                let rhs = self.expr(&p.rhs);
                self.et.rename_pipe_var = rename;
                let rhs = rhs?;
                self.local_id += 1;
                let lid = Lid(pos, (self.local_id, "$$"));
                Expr_::Pipe(self.a((
                    lid,
                    lhs,
                    rhs,
                    if p.nullsafe {
                        OperatorNullFlavor::Nullsafe
                    } else {
                        OperatorNullFlavor::Regular
                    },
                )))
            }
            K::Eif(c) => {
                if c.then.is_none() {
                    let cond = self.expr(&c.cond)?;
                    let otherwise = self.expr(&c.otherwise)?;
                    Expr_::Eif(self.a((cond, None, otherwise)))
                } else {
                    // Branches first, then the condition: this fixes the
                    // numbering of any pipe variables inside.
                    let otherwise = self.expr(&c.otherwise)?;
                    let then = self.opt_expr(&c.then)?;
                    let cond = self.expr(&c.cond)?;
                    Expr_::Eif(self.a((cond, then, otherwise)))
                }
            }
            _ => unreachable!("not one of these"),
        })
    }

    /// Functions and expression trees.
    #[inline(never)]
    fn expr_function(&mut self, e: &n::Expr<'s>, pos: Pos) -> R<Expr_<'a>> {
        use n::ExprKind as K;
        Ok(match &e.kind {
            K::ExpressionTree(tree) => self.expression_tree(e, tree)?,
            K::EtSplice(splice) => self.et_splice(splice)?,
            K::Efun(efun) => {
                let fun = self.closure_fun(&efun.fun, pos, true)?;
                let use_ = self.list(efun.uses, |l, u| {
                    Ok(CaptureLid((), Lid(l.pos(u.span), (0, l.s(u.name)))))
                })?;
                Expr_::Efun(self.a(Efun {
                    fun,
                    use_,
                    closure_class_name: None,
                    is_expr_tree_virtual_expr: false,
                }))
            }
            K::Lfun(fun) => {
                let fun = self.closure_fun(fun, pos, false)?;
                Expr_::Lfun(self.a((fun, &[][..])))
            }
            K::AsyncBlock(fun) => {
                let fun = self.closure_fun(fun, pos, false)?;
                let func = Expr((), pos, Expr_::Lfun(self.a((fun, &[][..]))));
                Expr_::Call(self.a(CallExpr {
                    func,
                    targs: &[],
                    args: &[],
                    unpacked_arg: None,
                }))
            }
            _ => unreachable!("not one of these"),
        })
    }

    // ------------------------------------------------------------------
    // Functions

    fn fun_kind(fun: &n::Fun<'_>) -> FunKind {
        match (fun.is_async(), fun.has_yield()) {
            (false, false) => FunKind::FSync,
            (true, false) => FunKind::FAsync,
            (false, true) => FunKind::FGenerator,
            (true, true) => FunKind::FAsyncGenerator,
        }
    }

    fn param(&mut self, p: &n::Param<'s>) -> R<FunParam<'a>> {
        use n::param_flags as pf;
        let user_attributes = self.attrs(p.attrs)?;
        let pos = self.pos(p.name.span);
        let hint = self
            .opt_hint(&p.hint)?
            .map(|h| self.soften(&user_attributes, h));
        let expr = self.opt_expr(&p.default)?;
        let callconv = match p.inout {
            Some(span) => ParamKind::Pinout(self.pos(span)),
            None => ParamKind::Pnormal,
        };
        let info = if p.flags & pf::VARIADIC != 0 {
            FunParamInfo::ParamVariadic
        } else if expr.is_some() {
            FunParamInfo::ParamOptional(expr)
        } else if p.flags & pf::OPTIONAL != 0 {
            FunParamInfo::ParamOptional(None)
        } else {
            FunParamInfo::ParamRequired
        };
        Ok(FunParam {
            annotation: (),
            type_hint: TypeHint((), hint),
            pos,
            name: self.s(p.name.name),
            info,
            readonly: (p.flags & pf::READONLY != 0).then_some(ReadonlyKind::Readonly),
            splat: (p.flags & pf::SPLAT != 0).then_some(SplatKind::Splat),
            callconv,
            named: (p.flags & pf::NAMED != 0).then_some(ParamNamed::ParamNamed),
            user_attributes,
            visibility: p.visibility.map(visibility),
        })
    }

    fn params(&mut self, ps: &[n::Param<'s>]) -> R<SmallVec<[FunParam<'a>; 4]>> {
        let mut out = SmallVec::new();
        for p in ps {
            out.push(self.param(p)?);
        }
        Ok(out)
    }

    fn noop(&self) -> Stmt<'a> {
        Stmt(Pos::NONE, Stmt_::Noop)
    }

    /// The body of a function-like that has one.
    fn function_body(&mut self, fun: &n::Fun<'s>) -> R<Block<'a>> {
        // The code of an expression tree literal is not run where it
        // stands, so a function in it does not start afresh.
        if self.et.in_literal {
            return self.body_block(fun.body);
        }
        let outer = (self.et.found_await, self.et.in_async);
        self.et.found_await = false;
        self.et.in_async = fun.is_async();
        let body = self.body_block(fun.body);
        (self.et.found_await, self.et.in_async) = outer;
        body
    }

    fn body_block(&mut self, stmts: &[n::Stmt<'s>]) -> R<Block<'a>> {
        // Declaration files only describe signatures.
        if self.mode == Mode::Mhhi && self.in_decl {
            return Ok(Block(self.slice(&[self.noop()])));
        }
        self.block(stmts, false)
    }

    /// `` Dsl`...` ``: the literal stands for calls on the DSL class that
    /// describe the code in it.
    fn expression_tree(&mut self, e: &n::Expr<'s>, tree: &n::ExpressionTree<'s>) -> R<Expr_<'a>> {
        let is_nested = self.et.class.is_some();
        let class = match (&tree.prefix, self.et.class) {
            (Some(prefix), Some(enclosing)) => {
                let nested = self.id(prefix);
                if nested.1 != enclosing.1 {
                    self.errors.push((
                        e.span.start + 1,
                        SyntaxError {
                            start: e.span.start,
                            end: e.span.end,
                            message: format!(
                                "Nested expression trees must use the same DSL name as the enclosing expression tree, got {} inside of {}",
                                nested.1, enclosing.1
                            )
                            .into(),
                        },
                    ));
                }
                nested
            }
            (None, Some(enclosing)) => enclosing,
            (Some(prefix), None) => self.id(prefix),
            (None, None) => Id(self.pos2(e.span.start, e.span.start), "MISSING_DSL"),
        };
        if !is_nested {
            self.et.class = Some(class);
        }
        self.et.in_literal = true;
        let rename = std::mem::replace(&mut self.et.rename_pipe_var, false);
        let body_pos = self.pos(tree.body_span);
        let enclosing_pos = self.et.enclosing_pos.replace(body_pos);
        let source = match &tree.body {
            n::EtBody::Expr(body) => self.expr(body),
            // A block is a function that is called on the spot.
            n::EtBody::Block(block) => self.body_block(block).map(|body| {
                let fun = Fun_ {
                    span: body_pos,
                    readonly_this: None,
                    annotation: (),
                    readonly_ret: None,
                    tparams: &[],
                    ret: TypeHint((), None),
                    params: &[],
                    ctxs: None,
                    unsafe_ctxs: None,
                    body: FuncBody { fb_ast: body },
                    fun_kind: FunKind::FSync,
                    user_attributes: UserAttributes(&[]),
                    external: false,
                    hidden: false,
                    doc_comment: None,
                };
                let func = Expr((), body_pos, Expr_::Lfun(self.a((fun, &[][..]))));
                let call = CallExpr {
                    func,
                    targs: &[],
                    args: &[],
                    unpacked_arg: None,
                };
                Expr((), body_pos, Expr_::Call(self.a(call)))
            }),
        };
        self.et.enclosing_pos = enclosing_pos;
        self.et.rename_pipe_var = rename;
        let source = source?;
        let env = expr_tree::Env {
            arena: expr_tree::Arena {
                bump: self.arena,
                line_starts: self.arena.alloc_slice_copy(self.lines.line_starts()),
            },
            in_async: self.et.in_async,
            enclosing_et_pos: enclosing_pos,
        };
        let result = expr_tree::desugar(&class, source, &env, is_nested);
        // HHVM finds these once it has translated what is in the literal.
        let after = e.span.end;
        self.errors
            .extend(result.errors.into_iter().map(|(pos, message)| {
                (
                    after,
                    SyntaxError {
                        start: pos.start_offset() as u32,
                        end: pos.end_offset() as u32,
                        message: message.into(),
                    },
                )
            }));
        if !is_nested {
            self.et.class = None;
        }
        self.et.in_literal = false;
        Ok(result.expr.2)
    }

    /// `${...}` in an expression tree literal: code that is run where the
    /// literal stands, and whose value takes its place in the literal.
    fn et_splice(&mut self, splice: &n::EtSplice<'s>) -> R<Expr_<'a>> {
        let outer_await = std::mem::replace(&mut self.et.found_await, false);
        self.et.in_literal = false;
        let rename = std::mem::replace(&mut self.et.rename_pipe_var, true);
        let pos = self.pos(splice.span);
        let inner = self.expr(&splice.expr);
        self.et.rename_pipe_var = rename;
        let contains_await = self.et.found_await;
        self.et.found_await |= outer_await;
        self.et.in_literal = true;
        let spliced_expr = Expr((), pos, inner?.2);
        // The variables the literals inside the splice leave unbound make
        // it a macro over them.
        struct FreeVars<'a>(Vec<Lid<'a>>);
        impl<'a> Visitor<'a> for FreeVars<'a> {
            type Params = AstParams<(), std::convert::Infallible>;

            fn object(&mut self) -> &mut dyn Visitor<'a, Params = Self::Params> {
                self
            }

            fn visit_expr_(
                &mut self,
                c: &mut (),
                e: &'a Expr_<'a>,
            ) -> Result<(), std::convert::Infallible> {
                match e {
                    Expr_::ExpressionTree(tree) => {
                        self.0.extend_from_slice(tree.free_vars.unwrap_or_default());
                        Ok(())
                    }
                    _ => e.recurse(c, self),
                }
            }
        }
        let spliced_expr = self.a(spliced_expr);
        let mut free_vars = FreeVars(Vec::new());
        let Ok(()) = visit(&mut free_vars, &mut (), spliced_expr);
        Ok(Expr_::ETSplice(self.a(EtSplice {
            extract_client_type: true,
            contains_await,
            macro_variables: (!free_vars.0.is_empty()).then(|| self.slice(&free_vars.0)),
            temp_lid: None,
            spliced_expr: *spliced_expr,
        })))
    }

    /// A lambda, closure or async block.
    fn closure_fun(&mut self, fun: &n::Fun<'s>, span: Pos, is_efun: bool) -> R<Fun_<'a>> {
        // In a closure `$$` is no longer that of a pipe around a literal.
        let rename = std::mem::replace(&mut self.et.rename_pipe_var, false);
        let lowered = self.closure_fun_(fun, span, is_efun);
        self.et.rename_pipe_var = rename;
        lowered
    }

    fn closure_fun_(&mut self, fun: &n::Fun<'s>, span: Pos, is_efun: bool) -> R<Fun_<'a>> {
        use n::fun_flags as ff;
        // A function in the code of an expression tree literal is never a
        // generator: its `yield`s are not run there.
        let fun_kind = match Self::fun_kind(fun) {
            FunKind::FGenerator if self.et.in_literal => FunKind::FSync,
            FunKind::FAsyncGenerator if self.et.in_literal => FunKind::FAsync,
            kind => kind,
        };
        let saved_in_decl = self.in_decl;
        let (tparams, params, ctxs, ret, body, user_attributes);
        if is_efun {
            // Anonymous functions number their pipe variables body first.
            self.in_decl = true;
            ctxs = self.contexts(&fun.ctxs)?;
            self.check_closure_contexts(ctxs.as_ref(), "An anonymous function");
            body = self.function_body(fun)?;
            user_attributes = self.attrs(fun.attrs)?;
            params = self.params(fun.params)?;
            tparams = self.hint_tparams(fun.tparams)?;
            ret = self.opt_hint(&fun.ret)?;
        } else {
            tparams = self.hint_tparams(fun.tparams)?;
            params = self.params(fun.params)?;
            ctxs = self.contexts(&fun.ctxs)?;
            self.check_closure_contexts(ctxs.as_ref(), "A lambda");
            ret = self.opt_hint(&fun.ret)?;
            // A lambda with a block body is a declaration context of its
            // own; an expression-bodied one is not.
            let is_expr_body = matches!(fun.body, [n::Stmt { kind: n::StmtKind::Return(Some(e)), span }] if *span == e.span);
            if !is_expr_body {
                self.in_decl = true;
            }
            body = self.function_body(fun)?;
            user_attributes = self.attrs(fun.attrs)?;
        }
        self.in_decl = saved_in_decl;
        Ok(Fun_ {
            span,
            readonly_this: None,
            annotation: (),
            readonly_ret: (fun.flags & ff::READONLY_RETURN != 0).then_some(ReadonlyKind::Readonly),
            tparams,
            ret: TypeHint((), ret),
            params: self.slice(&params),
            unsafe_ctxs: ctxs,
            ctxs,
            body: FuncBody { fb_ast: body },
            fun_kind,
            user_attributes,
            external: false,
            hidden: false,
            doc_comment: self.doc(fun.doc_comment),
        })
    }

    // ------------------------------------------------------------------
    // Statements

    /// A block. Empty braces become nothing when `remove_noop` is set and a
    /// lone `Noop` otherwise.
    fn block(&mut self, stmts: &[n::Stmt<'s>], remove_noop: bool) -> R<Block<'a>> {
        if stmts.is_empty() {
            return Ok(if remove_noop {
                Block(&[])
            } else {
                Block(self.slice(&[self.noop()]))
            });
        }
        self.stmt_list(stmts)
    }

    /// Statements of a `{ ... }` block. A function-scoped `using` swallows
    /// the statements after it.
    fn stmt_list(&mut self, stmts: &[n::Stmt<'s>]) -> R<Block<'a>> {
        let using_at = stmts.iter().position(|s| {
            matches!(&s.kind, n::StmtKind::Using(u) if u.body.is_none() && !u.enclosing_block.is_none())
        });
        let Some(i) = using_at else {
            return Ok(Block(self.list(stmts, |l, s| l.stmt(s))?));
        };
        let mut out: Vec<Stmt<'a>> = Vec::with_capacity(i + 1);
        for s in &stmts[..i] {
            out.push(self.stmt(s)?);
        }
        let n::StmtKind::Using(using) = &stmts[i].kind else {
            unreachable!()
        };
        let body = if i + 1 < stmts.len() {
            self.stmt_list(&stmts[i + 1..])?
        } else {
            Block(&[])
        };
        self.et.found_await |= using.has_await;
        let exprs_pos = self.pos(using.exprs_span);
        let exprs = self.exprs(using.exprs)?;
        let pos = self.pos(using.enclosing_block);
        out.push(Stmt(
            pos,
            Stmt_::Using(self.a(UsingStmt {
                is_block_scoped: false,
                has_await: using.has_await,
                exprs: (exprs_pos, exprs),
                block: body,
            })),
        ));
        Ok(Block(self.slice(&out)))
    }

    fn keyword_call(
        &mut self,
        pos: Pos,
        name: &'static str,
        echo: &n::Echo<'s>,
        args_first: bool,
    ) -> R<Stmt_<'a>> {
        let kw = self.pos(echo.keyword);
        let func = Expr((), kw, Expr_::Id(self.a(Id(kw, name))));
        // `unset` numbers pipe variables in its arguments before anything
        // else; either way the arguments are plain expressions.
        let _ = args_first;
        let args = self.list(echo.args, |l, e| Ok(Argument::Anormal(l.expr(e)?)))?;
        let call = self.a(CallExpr {
            func,
            targs: &[],
            args,
            unpacked_arg: None,
        });
        Ok(Stmt_::Expr(self.a(Expr((), pos, Expr_::Call(call)))))
    }

    fn stmt(&mut self, s: &n::Stmt<'s>) -> R<Stmt<'a>> {
        use n::StmtKind as K;
        let pos = self.pos(s.span);
        let stmt_ = match &s.kind {
            K::Noop => Stmt_::Noop,
            K::Expr(e) => {
                let e = self.expr(e)?;
                Stmt_::Expr(self.a(e))
            }
            K::Break => Stmt_::Break,
            K::Continue => Stmt_::Continue,
            K::Throw(e) => {
                let e = self.expr(e)?;
                Stmt_::Throw(self.a(e))
            }
            K::Return(e) => {
                let e = match e {
                    Some(e) => Some(self.expr(e)?),
                    None => None,
                };
                Stmt_::Return(self.a(e))
            }
            K::YieldBreak => Stmt_::YieldBreak,
            K::Echo(echo) => self.keyword_call(pos, "echo", echo, false)?,
            K::Unset(unset) => self.keyword_call(pos, "unset", unset, true)?,
            K::If(i) => {
                let cond = self.expr(&i.cond)?;
                let then = self.block(i.then, true)?;
                let otherwise = match i.otherwise {
                    Some(b) => self.block(b, true)?,
                    None => Block(self.slice(&[self.noop()])),
                };
                Stmt_::If(self.a((cond, then, otherwise)))
            }
            K::While(w) => {
                let cond = LoopCond(None, None, self.expr(&w.cond)?);
                let body = self.block(w.body, true)?;
                Stmt_::While(self.a((cond, body)))
            }
            K::Do(w) => {
                let body = self.block(w.body, false)?;
                let cond = LoopCond(None, None, self.expr(&w.cond)?);
                Stmt_::Do(self.a((body, cond)))
            }
            K::For(f) => {
                let init = self.exprs(f.init)?;
                let cond = match &f.cond {
                    Some(c) => Some(LoopCond(None, None, self.expr(c)?)),
                    None => None,
                };
                let step = LoopIter(None, None, self.exprs(f.step)?);
                let body = self.block(f.body, true)?;
                Stmt_::For(self.a((init, cond, step, body)))
            }
            K::Foreach(f) => {
                let collection = self.expr(&f.collection)?;
                let await_pos = f.await_span.map(|span| self.pos(span));
                self.et.found_await |= await_pos.is_some();
                let value = self.lvalue(&f.value)?;
                let key = match &f.key {
                    Some(key) => Some(self.lvalue(key)?),
                    None => None,
                };
                let as_expr = match (await_pos, key) {
                    (Some(p), None) => AsExpr::AwaitAsV(p, value),
                    (None, None) => AsExpr::AsV(value),
                    (Some(p), Some(key)) => AsExpr::AwaitAsKv(p, key, value),
                    (None, Some(key)) => AsExpr::AsKv(key, value),
                };
                let body = self.block(f.body, true)?;
                Stmt_::Foreach(self.a((collection, as_expr, body)))
            }
            K::Switch(sw) => {
                let mut cases: SmallVec<[Case<'a>; 8]> = SmallVec::new();
                let mut default = None;
                for case in sw.cases {
                    let mut body: SmallVec<[Stmt<'a>; 8]> = SmallVec::new();
                    for s in case.body {
                        body.push(self.stmt(s)?);
                    }
                    if case.fallthrough {
                        body.push(Stmt(Pos::NONE, Stmt_::Fallthrough));
                    }
                    let body = Block(self.slice(&body));
                    match &case.label {
                        Some(label) => cases.push(Case(self.expr(label)?, body)),
                        None => default = Some(DefaultCase(self.pos(case.label_span), body)),
                    }
                }
                let subject = self.expr(&sw.subject)?;
                Stmt_::Switch(self.a((subject, self.slice(&cases), default)))
            }
            K::Match(m) => {
                let expr = self.expr(&m.subject)?;
                let arms = self.list(m.arms, |l, arm| {
                    let pos = l.pos(arm.pattern.span);
                    let id = match arm.pattern.binding {
                        n::PatternBinding::Variable(name) => Some(Lid(pos, (0, l.s(name)))),
                        n::PatternBinding::Wildcard(_) => None,
                    };
                    let pat = match &arm.pattern.hint {
                        Some(hint) => {
                            let hint = l.hint(hint)?;
                            Pattern::PRefinement(l.a(PatRefinement { pos, id, hint }))
                        }
                        None => Pattern::PVar(l.a(PatVar { pos, id })),
                    };
                    Ok(StmtMatchArm {
                        pat,
                        body: l.block(arm.body, true)?,
                    })
                })?;
                Stmt_::Match(self.a(StmtMatch { expr, arms }))
            }
            K::Try(t) => {
                let body = self.block(t.body, false)?;
                let catches = self.list(t.catches, |l, c| {
                    let class = l.id(&c.class);
                    let var = Lid(l.pos(c.var.span), (0, l.s(c.var.name)));
                    Ok(Catch(class, var, l.block(c.body, true)?))
                })?;
                let finally = match t.finally {
                    Some(b) => FinallyBlock(self.block(b, false)?.0),
                    None => FinallyBlock(&[]),
                };
                Stmt_::Try(self.a((body, catches, finally)))
            }
            K::Using(u) => {
                self.et.found_await |= u.has_await;
                let exprs_pos = self.pos(u.exprs_span);
                let exprs = self.exprs(u.exprs)?;
                let block = match u.body {
                    Some(b) => self.block(b, false)?,
                    None => Block(self.slice(&[self.noop()])),
                };
                Stmt_::Using(self.a(UsingStmt {
                    is_block_scoped: u.body.is_some(),
                    has_await: u.has_await,
                    exprs: (exprs_pos, exprs),
                    block,
                }))
            }
            K::Concurrent(b) => Stmt_::Concurrent(self.block(b, true)?),
            K::Block(b) => {
                let block = self.block(b, false)?;
                Stmt_::Block(self.a((None, block)))
            }
        };
        Ok(Stmt(pos, stmt_))
    }

    // ------------------------------------------------------------------
    // Definitions

    fn defs(&mut self, defs: &[n::Def<'s>], out: &mut Vec<Def<'a>>) -> R<()> {
        for def in defs {
            match def {
                n::Def::Fun(f) => {
                    let saved = std::mem::replace(&mut self.in_decl, true);
                    let def = self.fun_def(f);
                    self.in_decl = saved;
                    out.push(def?);
                }
                n::Def::Class(c) => {
                    let saved = std::mem::replace(&mut self.in_decl, true);
                    let class = self.class(c);
                    self.in_decl = saved;
                    let class = class?;
                    out.push(Def::Class(self.a(class)));
                }
                n::Def::ClassAlias(alias) => {
                    let name = self.id(&alias.name);
                    let tparams = self.tparams(alias.tparams, true)?;
                    let original = self.id(&alias.original);
                    let original_tparams = self.tparams(alias.original_tparams, true)?;
                    out.push(Def::ClassAlias(self.a(ClassAlias {
                        name,
                        tparams: self.slice(&tparams),
                        original,
                        original_tparams: self.slice(&original_tparams),
                    })));
                }
                n::Def::Typedef(t) => out.push(self.typedef(t)?),
                n::Def::Const(c) => {
                    for item in c.items {
                        let name = self.id(&item.name);
                        let type_ = self.opt_hint(&c.hint)?;
                        let value = match &item.value {
                            Some(value) => self.expr(value)?,
                            // Declaration files may leave the value out.
                            None if self.mode == Mode::Mhhi => Expr((), name.0, Expr_::Omitted),
                            None => return err(item.name.span.start, "constant without a value"),
                        };
                        let span = self.pos(c.span);
                        out.push(Def::Constant(self.a(Gconst {
                            annotation: (),
                            mode: self.mode,
                            name,
                            type_,
                            value,
                            span,
                        })));
                    }
                }
                n::Def::Namespace(ns) => {
                    let name = self.id(&ns.name);
                    let mut inner = Vec::with_capacity(ns.defs.len());
                    let saved = self.in_decl;
                    self.defs(ns.defs, &mut inner)?;
                    self.in_decl = saved;
                    out.push(Def::Namespace(self.a((name, self.slice(&inner)))));
                }
                n::Def::Use(clauses) => {
                    let uses = self.list(clauses, |l, c| {
                        let pos = l.pos(c.name.span);
                        let name = l.s(c.name.name);
                        let alias = match &c.alias {
                            Some(alias) => l.id(alias),
                            None => Id(pos, name.rsplit('\\').next().unwrap_or(name)),
                        };
                        let full = if name.starts_with('\\') {
                            name
                        } else {
                            l.str(format_args!("\\{}", name))
                        };
                        Ok((
                            match c.kind {
                                n::UseKind::Default => NsKind::NSClassAndNamespace,
                                n::UseKind::Namespace => NsKind::NSNamespace,
                                n::UseKind::Type => NsKind::NSClass,
                                n::UseKind::Function => NsKind::NSFun,
                                n::UseKind::Const => NsKind::NSConst,
                            },
                            Id(pos, full),
                            alias,
                        ))
                    })?;
                    out.push(Def::NamespaceUse(uses));
                }
                n::Def::FileAttributes(attrs) => {
                    let user_attributes = self.attrs(attrs)?;
                    out.push(Def::FileAttributes(
                        self.a(FileAttribute { user_attributes }),
                    ));
                }
                n::Def::SetModule(id) => {
                    let id = self.id(id);
                    out.push(Def::SetModule(self.a(id)));
                }
                n::Def::Module(m) => {
                    let user_attributes = self.attrs(m.attrs)?;
                    let module = ModuleDef {
                        annotation: (),
                        name: self.id(&m.name),
                        user_attributes,
                        span: self.pos(m.span),
                        mode: self.mode,
                        doc_comment: self.doc(m.doc_comment),
                    };
                    out.push(Def::Module(self.a(module)));
                }
                n::Def::Stmt(s) => {
                    // Declaration files have no top-level code.
                    if self.mode == Mode::Mhhi {
                        continue;
                    }
                    let stmt = self.stmt(s)?;
                    out.push(Def::Stmt(self.a(stmt)));
                }
            }
        }
        Ok(())
    }

    fn fun_def(&mut self, f: &n::FunDef<'s>) -> R<Def<'a>> {
        use n::fun_flags as ff;
        let fun = &f.fun;
        self.cls_generics.clear();
        self.fn_generics.clear();
        let mut tparams = self.tparams(fun.tparams, false)?;
        let mut params = self.params(fun.params)?;
        let ctxs = self.contexts(&fun.ctxs)?;
        let mut where_constraints = self.where_constraints(fun.where_constraints)?;
        self.rewrite_effect_polymorphism(
            &mut params,
            &mut tparams,
            ctxs.as_ref(),
            &mut where_constraints,
        )?;
        let ret = self.opt_hint(&fun.ret)?;
        let name = self.id(&f.name);
        let external = !fun.has_body();
        let body = if external {
            Block(&[])
        } else {
            self.function_body(fun)?
        };
        let user_attributes = self.attrs(fun.attrs)?;
        self.check_memoized_contexts(ctxs.as_ref(), &user_attributes, "function");
        self.check_context_has_this(ctxs.as_ref());
        self.check_low_priority(fun, &user_attributes);
        let fun_ = Fun_ {
            span: self.pos(fun.span),
            readonly_this: None,
            annotation: (),
            readonly_ret: (fun.flags & ff::READONLY_RETURN != 0).then_some(ReadonlyKind::Readonly),
            tparams: &[],
            ret: TypeHint((), ret),
            params: self.slice(&params),
            unsafe_ctxs: ctxs,
            ctxs,
            body: FuncBody { fb_ast: body },
            fun_kind: Self::fun_kind(fun),
            user_attributes,
            external,
            hidden: false,
            doc_comment: self.doc(fun.doc_comment),
        };
        Ok(Def::Fun(self.a(FunDef {
            mode: self.mode,
            name,
            fun: fun_,
            internal: f.internal,
            tparams: self.slice(&tparams),
            where_constraints: self.slice(&where_constraints),
        })))
    }

    fn docs_url(&mut self, attrs: &[UserAttribute<'a>]) -> Option<&'a str> {
        let mut url = None;
        for attr in attrs {
            if attr.name.1 == "__Docs"
                && let [param] = attr.params
            {
                let problem = match param.2 {
                    Expr_::String(s) => match std::str::from_utf8(s) {
                        Ok(s) => {
                            url = Some(s);
                            continue;
                        }
                        Err(_) => "`__Docs` URLs must be valid UTF-8",
                    },
                    _ => "`__Docs` URLs must be a string literal",
                };
                self.report(attr.name.0, problem.to_string());
            }
        }
        url
    }

    fn typedef(&mut self, t: &n::Typedef<'s>) -> R<Def<'a>> {
        let tparams = self.tparams(t.tparams, false)?;
        let user_attributes = self.attrs(t.attrs)?;
        let docs_url = self.docs_url(&user_attributes);
        let mut as_constraints: SmallVec<[Hint<'a>; 2]> = SmallVec::new();
        let mut super_constraints: SmallVec<[Hint<'a>; 2]> = SmallVec::new();
        let is_ctx = t.kind == n::TypedefKind::CtxAlias;
        for (kind, hint) in t.constraints {
            let hint = self.hint(hint)?;
            if is_ctx {
                self.check_context_bound(&hint);
            }
            match kind {
                n::ConstraintKind::As => as_constraints.push(hint),
                n::ConstraintKind::Super => super_constraints.push(hint),
                n::ConstraintKind::Eq => {}
            }
        }
        let (assignment, runtime_type, as_constraint);
        if t.kind == n::TypedefKind::CaseType {
            let variants = self.list(t.variants, |l, v| {
                let hint = l.hint(&v.hint)?;
                let where_constraints = l.where_constraints(v.where_constraints)?;
                Ok(TypedefCaseTypeVariant {
                    hint,
                    where_constraints: l.slice(&where_constraints),
                })
            })?;
            // None that could be read (see the parser's `repair.rs`).
            let Some((first, rest)) = variants.split_first() else {
                return err(t.span.start, "case type without variants");
            };
            runtime_type = if rest.is_empty() {
                first.hint
            } else {
                self.hint(&t.hint)?
            };
            assignment = TypedefAssignment::CaseType(*first, rest);
            as_constraint = if as_constraints.len() > 1 {
                let start = t.constraints.first().unwrap().1.span.start;
                let end = t.constraints.last().unwrap().1.span.end;
                let pos = self.pos2(start, end);
                Some(Hint(
                    pos,
                    self.a(Hint_::Hintersection(self.slice(&as_constraints))),
                ))
            } else {
                as_constraints.pop()
            };
        } else {
            let hint = match t.hint.kind {
                // A context alias without a right-hand side: the placeholder
                // takes the position of the alias name.
                n::HintKind::Name(name) if is_ctx => {
                    let pos = self.pos(t.hint.span);
                    Hint(pos, self.a(Hint_::Happly(Id(pos, self.s(name)), &[])))
                }
                _ => {
                    let hint = self.hint(&t.hint)?;
                    if let (true, Hint_::Hintersection(hints)) = (is_ctx, hint.1) {
                        self.check_fixed_contexts(
                            hints,
                            "Context aliases cannot alias polymorphic contexts",
                        );
                    }
                    hint
                }
            };
            runtime_type = hint;
            assignment = TypedefAssignment::SimpleTypeDef(TypedefVisibilityAndHint {
                vis: match t.kind {
                    // `newctx` is an opaque alias
                    n::TypedefKind::Transparent => TypedefVisibility::Transparent,
                    n::TypedefKind::OpaqueModule => TypedefVisibility::OpaqueModule,
                    _ => TypedefVisibility::Opaque,
                },
                hint,
            });
            as_constraint = as_constraints.pop();
        }
        let name = self.id(&t.name);
        let span = self.pos(t.span);
        let doc_comment = self.doc(t.doc_comment);
        Ok(Def::Typedef(self.a(Typedef {
            annotation: (),
            name,
            tparams: self.slice(&tparams),
            as_constraint,
            super_constraint: super_constraints.pop(),
            assignment,
            runtime_type,
            user_attributes,
            mode: self.mode,
            span,
            is_ctx,
            internal: t.internal,
            docs_url,
            doc_comment,
        })))
    }

    fn class(&mut self, c: &n::Class<'s>) -> R<Class_<'a>> {
        use n::class_flags as cf;
        let mut user_attributes = self.attrs(c.attrs)?;
        let is_enum_class = c.kind == n::ClassKind::EnumClass;
        let name = self.id(&c.name);
        if is_enum_class {
            let mut attrs: SmallVec<[UserAttribute<'a>; 4]> =
                user_attributes.0.iter().copied().collect();
            attrs.push(UserAttribute {
                name: Id(name.0, "__EnumClass"),
                params: &[],
            });
            user_attributes = UserAttributes(self.slice(&attrs));
        }
        let docs_url = self.docs_url(&user_attributes);
        let is_abstract = c.flags & cf::ABSTRACT != 0;
        let abstraction = if is_abstract {
            Abstraction::Abstract
        } else {
            Abstraction::Concrete
        };
        let kind = match c.kind {
            n::ClassKind::Class => ClassishKind::Cclass(abstraction),
            n::ClassKind::Interface => ClassishKind::Cinterface,
            n::ClassKind::Trait => ClassishKind::Ctrait,
            n::ClassKind::Enum => ClassishKind::Cenum,
            n::ClassKind::EnumClass => ClassishKind::CenumClass(abstraction),
        };
        let is_enum = matches!(c.kind, n::ClassKind::Enum | n::ClassKind::EnumClass);
        if !is_enum {
            self.cls_generics.clear();
            self.fn_generics.clear();
        }
        let tparams = self.tparams(c.tparams, true)?;
        let base = self.opt_hint(&c.enum_base)?;
        let extends = self.hints(c.extends)?;
        if !is_enum {
            self.parent_maybe_reified = matches!(
                extends.first(),
                Some(Hint(_, Hint_::Happly(_, targs))) if !targs.is_empty()
            );
        }
        let implements = self.hints(c.implements)?;
        let span = self.pos(c.span);
        let doc_comment = self.doc(c.doc_comment);

        let mut m = Members::default();
        for member in c.members {
            self.member(c, member, span, &mut m)?;
        }
        let enum_ = match base {
            Some(base) => {
                let constraint = self.opt_hint(&c.enum_constraint)?;
                Some(Enum_ {
                    base,
                    constraint,
                    includes: if is_enum_class {
                        extends
                    } else {
                        self.slice(&m.includes)
                    },
                })
            }
            None => None,
        };
        Ok(Class_ {
            span,
            annotation: (),
            mode: self.mode,
            final_: !is_enum && c.flags & cf::FINAL != 0,
            is_xhp: c.flags & cf::XHP != 0,
            has_xhp_keyword: c.flags & cf::XHP_KEYWORD != 0,
            kind,
            name,
            tparams: self.slice(&tparams),
            extends,
            uses: self.slice(&m.uses),
            xhp_attr_uses: self.slice(&m.xhp_attr_uses),
            xhp_category: m.xhp_category,
            reqs: self.slice(&m.reqs),
            implements,
            consts: self.slice(&m.consts),
            typeconsts: self.slice(&m.typeconsts),
            vars: self.slice(&m.vars),
            methods: self.slice(&m.methods),
            xhp_children: self.slice(&m.xhp_children),
            xhp_attrs: self.slice(&m.xhp_attrs),
            user_attributes,
            docs_url,
            enum_,
            doc_comment,
            internal: c.flags & cf::INTERNAL != 0,
        })
    }

    fn member(
        &mut self,
        c: &n::Class<'s>,
        member: &n::Member<'s>,
        class_span: Pos,
        m: &mut Members<'a>,
    ) -> R<()> {
        match member {
            n::Member::Const(decl) => {
                let user_attributes = self.attrs(decl.attrs)?;
                let type_ = self.opt_hint(&decl.hint)?;
                let span = self.pos(decl.span);
                let doc_comment = self.doc(decl.doc_comment);
                for item in decl.items {
                    let id = self.id(&item.name);
                    let kind = if decl.is_abstract {
                        ClassConstKind::CCAbstract(self.opt_expr(&item.value)?)
                    } else {
                        match &item.value {
                            Some(value) => ClassConstKind::CCConcrete(self.expr(value)?),
                            None if self.mode == Mode::Mhhi => {
                                ClassConstKind::CCConcrete(Expr((), id.0, Expr_::Omitted))
                            }
                            None => return err(item.name.span.start, "constant without a value"),
                        }
                    };
                    m.consts.push(ClassConst {
                        user_attributes,
                        type_,
                        id,
                        kind,
                        span,
                        doc_comment,
                    });
                }
            }
            n::Member::TypeConst(tc) => self.type_const(tc, m)?,
            n::Member::Property(decl) => {
                use n::prop_flags as pf;
                let user_attributes = self.attrs(decl.attrs)?;
                let type_ = self
                    .opt_hint(&decl.hint)?
                    .map(|h| self.soften(&user_attributes, h));
                let doc_comment = self.doc(decl.doc_comment);
                let first = m.vars.len();
                for item in decl.items {
                    let name = self.s(item.name.name.strip_prefix('$').unwrap_or(item.name.name));
                    let id = Id(self.pos(item.name.span), name);
                    let span = self.pos(item.span);
                    let expr = self.opt_expr(&item.value)?;
                    m.vars.push(ClassVar {
                        final_: decl.flags & pf::FINAL != 0,
                        xhp_attr: None,
                        abstract_: decl.flags & pf::ABSTRACT != 0,
                        readonly: decl.flags & pf::READONLY != 0,
                        visibility: visibility(decl.visibility),
                        type_: TypeHint((), type_),
                        id,
                        expr,
                        user_attributes,
                        doc_comment: if m.vars.len() == first {
                            doc_comment
                        } else {
                            None
                        },
                        is_static: decl.flags & pf::STATIC != 0,
                        span,
                    });
                }
            }
            n::Member::Method(method) => self.method(method, m)?,
            n::Member::Use(hints) => {
                let hints = self.hints(hints)?;
                if c.kind == n::ClassKind::Enum {
                    m.includes.extend_from_slice(hints);
                } else {
                    m.uses.extend_from_slice(hints);
                }
            }
            n::Member::Require(req) => {
                let hint = self.hint(&req.hint)?;
                m.reqs.push(ClassReq(
                    hint,
                    match req.kind {
                        n::RequireKind::Extends => RequireKind::RequireExtends,
                        n::RequireKind::Implements => RequireKind::RequireImplements,
                        n::RequireKind::Class => RequireKind::RequireClass,
                        n::RequireKind::ThisAs => RequireKind::RequireThisAs,
                    },
                ));
            }
            n::Member::Enumerator(e) => {
                let doc_comment = self.doc(e.doc_comment);
                let id = self.id(&e.name);
                let value = self.expr(&e.value)?;
                m.consts.push(ClassConst {
                    user_attributes: UserAttributes(&[]),
                    type_: None,
                    id,
                    kind: ClassConstKind::CCConcrete(value),
                    span: class_span,
                    doc_comment,
                });
            }
            n::Member::EnumClassMember(member) => {
                let id = self.id(&member.name);
                let pos = id.0;
                let elt_type = self.hint(&member.hint)?;
                // `Type NAME` in `enum class E` has type `MemberOf<E, Type>`.
                let enum_class = Hint(
                    pos,
                    self.a(Hint_::Happly(Id(pos, self.s(c.name.name)), &[])),
                );
                let full_type = Hint(
                    pos,
                    self.a(Hint_::Happly(
                        Id(pos, "\\HH\\MemberOf"),
                        self.slice(&[enum_class, elt_type]),
                    )),
                );
                let kind = match &member.value {
                    Some(value) if !member.is_abstract => {
                        ClassConstKind::CCConcrete(self.expr(value)?)
                    }
                    _ => ClassConstKind::CCAbstract(None),
                };
                m.consts.push(ClassConst {
                    user_attributes: UserAttributes(&[]),
                    type_: Some(full_type),
                    id,
                    kind,
                    span: class_span,
                    doc_comment: None,
                });
            }
            n::Member::XhpAttributes(decls) => {
                for decl in *decls {
                    match decl {
                        n::XhpAttrDecl::Use(h) => {
                            let h = self.hint(h)?;
                            m.xhp_attr_uses.push(h);
                        }
                        n::XhpAttrDecl::Attr(attr) => {
                            let attr = self.xhp_attr(attr)?;
                            m.xhp_attrs.push(attr);
                        }
                    }
                }
            }
            n::Member::XhpChildren(decl) => {
                let pos = self.pos(decl.span);
                let child = self.xhp_child(&decl.child);
                m.xhp_children.push((pos, child));
            }
            n::Member::XhpCategory(decl) => {
                let pos = self.pos(decl.span);
                let categories = self.list(decl.categories, |l, c| {
                    Ok((
                        l.pos(c.span),
                        l.s(c.name.strip_prefix('%').unwrap_or(c.name)),
                    ))
                })?;
                m.xhp_category = Some((pos, categories));
            }
        }
        Ok(())
    }

    fn xhp_child(&mut self, child: &n::XhpChild<'s>) -> XhpChild<'a> {
        match child {
            n::XhpChild::Name(id) => XhpChild::ChildName(self.id(id)),
            n::XhpChild::List(items) => {
                let items: SmallVec<[XhpChild<'a>; 8]> =
                    items.iter().map(|c| self.xhp_child(c)).collect();
                XhpChild::ChildList(self.slice(&items))
            }
            n::XhpChild::Unary(inner, op) => {
                let inner = self.xhp_child(inner);
                XhpChild::ChildUnary(
                    self.a(inner),
                    match op {
                        n::XhpChildOp::Star => XhpChildOp::ChildStar,
                        n::XhpChildOp::Plus => XhpChildOp::ChildPlus,
                        n::XhpChildOp::Question => XhpChildOp::ChildQuestion,
                    },
                )
            }
            n::XhpChild::Binary(l, r) => {
                let l = self.xhp_child(l);
                let r = self.xhp_child(r);
                XhpChild::ChildBinary(self.a(l), self.a(r))
            }
        }
    }

    fn xhp_attr(&mut self, attr: &n::XhpAttr<'s>) -> R<XhpAttr<'a>> {
        let name_pos = self.pos(attr.name.span);
        let tag = attr.tag.map(|t| match t {
            n::XhpAttrTag::Required => XhpAttrTag::Required,
            n::XhpAttrTag::LateInit => XhpAttrTag::LateInit,
        });
        let span = if attr.default.is_some() {
            self.pos2(attr.name.span.start, attr.default_span.end)
        } else {
            name_pos
        };
        let (hint, like, enum_values, enum_): (_, _, &'a [XhpEnumValue<'a>], _) = match &attr.ty {
            n::XhpAttrType::Enum { span, like, values } => {
                let pos = self.pos(*span);
                let like = like.map(|l| self.pos(l));
                let vals = self.exprs(values)?;
                let mut enum_vals: SmallVec<[XhpEnumValue<'a>; 8]> = SmallVec::new();
                for v in vals {
                    match v.2 {
                        Expr_::String(s) => {
                            let s: &'a [u8] = s;
                            let s = match std::str::from_utf8(s) {
                                Ok(s) => s,
                                Err(_) => self.str(format_args!("{}", BStr::new(s))),
                            };
                            enum_vals.push(XhpEnumValue::XEVString(s));
                        }
                        Expr_::Int(i) => {
                            if let Ok(i) = i.parse() {
                                enum_vals.push(XhpEnumValue::XEVInt(i));
                            }
                        }
                        _ => {}
                    }
                }
                (None, like, self.slice(&enum_vals), Some((pos, vals)))
            }
            n::XhpAttrType::Hint(h) => (Some(self.hint(h)?), None, &[][..], None),
        };
        let expr = self.opt_expr(&attr.default)?;
        Ok(XhpAttr(
            TypeHint((), hint),
            ClassVar {
                final_: false,
                xhp_attr: Some(XhpAttrInfo {
                    like,
                    tag,
                    enum_values,
                }),
                abstract_: false,
                readonly: false,
                visibility: Visibility::Public,
                type_: TypeHint((), hint),
                id: Id(name_pos, self.str(format_args!(":{}", attr.name.name))),
                expr,
                user_attributes: UserAttributes(&[]),
                doc_comment: None,
                is_static: false,
                span,
            },
            tag,
            enum_,
        ))
    }

    fn type_const(&mut self, tc: &n::TypeConst<'s>, m: &mut Members<'a>) -> R<()> {
        match &tc.value {
            n::TypeConstValue::Type(value) => {
                let user_attributes = self.attrs(tc.attrs)?;
                let type_ = self
                    .opt_hint(value)?
                    .map(|h| self.soften(&user_attributes, h));
                let name = self.id(&tc.name);
                let mut lower: SmallVec<[Hint<'a>; 2]> = SmallVec::new();
                let mut upper: SmallVec<[Hint<'a>; 2]> = SmallVec::new();
                for (kind, hint) in tc.constraints {
                    let hint = self.hint(hint)?;
                    match kind {
                        n::ConstraintKind::As => upper.push(hint),
                        n::ConstraintKind::Super => lower.push(hint),
                        n::ConstraintKind::Eq => {}
                    }
                }
                // Several bounds of one kind combine: `as` bounds intersect,
                // `super` bounds unite.
                let as_constraint = match upper.len() {
                    0 => None,
                    1 => Some(upper[0]),
                    _ => Some(Hint(
                        upper[0].0,
                        self.a(Hint_::Hintersection(self.slice(&upper))),
                    )),
                };
                let super_constraint = match lower.len() {
                    0 => None,
                    1 => Some(lower[0]),
                    _ => Some(Hint(lower[0].0, self.a(Hint_::Hunion(self.slice(&lower))))),
                };
                let span = self.pos(tc.span);
                let kind = if tc.is_abstract {
                    ClassTypeconst::TCAbstract(ClassAbstractTypeconst {
                        as_constraint,
                        super_constraint,
                        default: type_,
                    })
                } else if let Some(c_tc_type) = type_ {
                    ClassTypeconst::TCConcrete(ClassConcreteTypeconst { c_tc_type })
                } else {
                    return err(tc.span.start, "type constant without a value");
                };
                m.typeconsts.push(ClassTypeconstDef {
                    user_attributes,
                    name,
                    kind,
                    span,
                    doc_comment: self.doc(tc.doc_comment),
                    is_ctx: false,
                });
            }
            n::TypeConstValue::Ctx(value) => {
                let name = self.id(&tc.name);
                let context = match value {
                    Some(ctxs) => {
                        let hints = self.hints(ctxs.hints)?;
                        self.check_fixed_contexts(
                            hints,
                            "Context constants cannot alias polymorphic contexts",
                        );
                        let list_pos = self.pos(ctxs.span);
                        for hint in hints {
                            if matches!(hint.1, Hint_::Happly(Id(_, id), _) if id.ends_with("_local"))
                            {
                                self.report(
                                    list_pos,
                                    "Local contexts on ctx constants are not allowed".to_string(),
                                );
                            }
                        }
                        Some(Hint(
                            self.pos(ctxs.span),
                            self.a(Hint_::Hintersection(hints)),
                        ))
                    }
                    None => None,
                };
                let span = self.pos(tc.span);
                let mut as_constraint = None;
                let mut super_constraint = None;
                for (kind, hint) in tc.constraints {
                    let hint = self.hint(hint)?;
                    self.check_context_bound(&hint);
                    match kind {
                        n::ConstraintKind::As => as_constraint = Some(hint),
                        n::ConstraintKind::Super => super_constraint = Some(hint),
                        n::ConstraintKind::Eq => {}
                    }
                }
                let kind = if tc.is_abstract {
                    ClassTypeconst::TCAbstract(ClassAbstractTypeconst {
                        as_constraint,
                        super_constraint,
                        default: context,
                    })
                } else if let Some(c_tc_type) = context {
                    ClassTypeconst::TCConcrete(ClassConcreteTypeconst { c_tc_type })
                } else {
                    return err(tc.span.start, "context constant without a value");
                };
                m.typeconsts.push(ClassTypeconstDef {
                    user_attributes: UserAttributes(&[]),
                    name,
                    kind,
                    span,
                    doc_comment: self.doc(tc.doc_comment),
                    is_ctx: true,
                });
            }
        }
        Ok(())
    }

    fn method(&mut self, method: &n::Method<'s>, m: &mut Members<'a>) -> R<()> {
        use n::fun_flags as ff;
        use n::method_flags as mf;
        let fun = &method.fun;
        self.fn_generics.clear();
        let mut tparams = self.tparams(fun.tparams, false)?;
        let mut params = self.params(fun.params)?;
        let ctxs = self.contexts(&fun.ctxs)?;
        let mut where_constraints = self.where_constraints(fun.where_constraints)?;
        self.rewrite_effect_polymorphism(
            &mut params,
            &mut tparams,
            ctxs.as_ref(),
            &mut where_constraints,
        )?;
        let ret = self.opt_hint(&fun.ret)?;
        let name = self.id(&method.name);

        // Constructor parameters with a visibility become properties, each
        // initialised by an assignment at the top of the body.
        let mut body_stmts: SmallVec<[Stmt<'a>; 8]> = SmallVec::new();
        let mut member_def: SmallVec<[ClassVar<'a>; 4]> = SmallVec::new();
        for param in params.iter() {
            let Some(vis) = param.visibility else {
                continue;
            };
            let cvname = param.name.strip_prefix('$').unwrap_or(param.name);
            let p = param.pos;
            let span = match &param.info {
                FunParamInfo::ParamOptional(Some(Expr(_, pos_end, _))) => Pos::btw(&p, pos_end),
                _ => p,
            };
            let prop_type_hint = match (&param.type_hint.1, &param.info) {
                (Some(h), FunParamInfo::ParamVariadic) => TypeHint(
                    (),
                    Some(Hint(
                        p,
                        self.a(Hint_::Happly(Id(p, "vec"), self.slice(&[*h]))),
                    )),
                ),
                _ => param.type_hint,
            };
            let this = Expr((), p, Expr_::Lvar(self.a(Lid(p, (0, "$this")))));
            let prop = Expr((), p, Expr_::Id(self.a(Id(p, cvname))));
            let target = Expr(
                (),
                p,
                Expr_::ObjGet(self.a((
                    this,
                    prop,
                    OperatorNullFlavor::Regular,
                    PropOrMethod::IsProp,
                ))),
            );
            let value = Expr((), p, Expr_::Lvar(self.a(Lid(p, (0, param.name)))));
            let assign = Expr((), p, Expr_::Assign(self.a((target, None, value))));
            body_stmts.push(Stmt(p, Stmt_::Expr(self.a(assign))));
            member_def.push(ClassVar {
                final_: false,
                xhp_attr: None,
                abstract_: false,
                readonly: param.readonly.is_some(),
                visibility: vis,
                type_: prop_type_hint,
                id: Id(p, cvname),
                expr: None,
                user_attributes: param.user_attributes,
                doc_comment: None,
                is_static: false,
                span,
            });
        }

        let is_abstract = method.flags & mf::ABSTRACT != 0;
        let has_body = fun.has_body();
        self.in_static_method = method.flags & mf::STATIC != 0;
        if self.in_static_method {
            // Contexts hanging off a reified type parameter of the class.
            if let Some(Contexts(_, hints)) = ctxs {
                for hint in hints.iter() {
                    if let Hint_::Haccess(Hint(_, Hint_::Happly(Id(_, id), _)), _) = hint.1 {
                        let method_pos = self.pos(fun.span);
                        self.check_reified_generic(method_pos, id.strip_prefix('\\').unwrap_or(id));
                    }
                }
            }
        }
        if has_body {
            let body = self.function_body(fun);
            self.in_static_method = false;
            body_stmts.extend_from_slice(body?.0);
        }
        self.in_static_method = false;
        let user_attributes = self.attrs(fun.attrs)?;
        self.check_memoized_contexts(ctxs.as_ref(), &user_attributes, "method");
        self.check_low_priority(fun, &user_attributes);
        m.vars.extend_from_slice(&member_def);
        m.methods.push(Method_ {
            span: self.pos(fun.span),
            annotation: (),
            final_: method.flags & mf::FINAL != 0,
            abstract_: is_abstract,
            static_: method.flags & mf::STATIC != 0,
            readonly_this: fun.flags & ff::READONLY_THIS != 0,
            visibility: method
                .visibility
                .map(visibility)
                .unwrap_or(Visibility::Public),
            name,
            tparams: self.slice(&tparams),
            where_constraints: self.slice(&where_constraints),
            params: self.slice(&params),
            unsafe_ctxs: ctxs,
            ctxs,
            body: FuncBody {
                fb_ast: Block(self.slice(&body_stmts)),
            },
            fun_kind: Self::fun_kind(fun),
            user_attributes,
            readonly_ret: (fun.flags & ff::READONLY_RETURN != 0).then_some(ReadonlyKind::Readonly),
            ret: TypeHint((), ret),
            external: !is_abstract && !has_body,
            hidden: false,
            doc_comment: self.doc(fun.doc_comment),
        });
        Ok(())
    }

    // ------------------------------------------------------------------
    // Context polymorphism
    //
    // `function f((function()[_]: void) $g)[ctx $g]` and
    // `function f(Foo $x)[$x::C]` are sugar for extra type parameters and
    // `where` constraints. The generated parameter names (`T/[ctx $g]`,
    // `T/$x`, `T/[$x::C]`) cannot clash with anything a user can write.

    /// Whether a context depends on something else: on a parameter, a
    /// type parameter or - unless `this_is_fixed` - on the class.
    fn is_polymorphic_context(&self, hint: &Hint<'a>, this_is_fixed: bool) -> bool {
        match hint.1 {
            Hint_::HfunContext(_) => true,
            Hint_::Haccess(root, _) => match root.1 {
                Hint_::Happly(Id(_, id), _) => {
                    (id.strip_prefix('\\').unwrap_or(id) == "this" && !this_is_fixed)
                        || lookup(&self.fn_generics, id).is_some()
                        || lookup(&self.cls_generics, id).is_some()
                }
                Hint_::Hvar(_) => true,
                _ => false,
            },
            _ => false,
        }
    }

    /// Replaces the `[_]` context of a function-typed parameter with a
    /// reference to the parameter itself, returning the new type.
    fn rewrite_fun_ctx(
        &mut self,
        tparams: &mut SmallVec<[Tparam<'a>; 4]>,
        hint: Hint<'a>,
        name: &'a str,
    ) -> R<Hint<'a>> {
        // What is not `(function(..)[_]: ..)` is an error, and stays as it
        // is: HHVM says where the type stops being that.
        let invalid = |this: &mut Self, pos: Pos| {
            this.report(
                pos,
                format!(
                    "Type hint for parameter {name} used for contextful function must be a function type hint whose context is exactly `[_]`, e.g. `(function (ts)[_]: t)`"
                ),
            );
            Ok(hint)
        };
        Ok(match *hint.1 {
            Hint_::Hfun(mut hf) => {
                let Some(Contexts(ctx_pos, ctxs)) = hf.ctxs else {
                    return invalid(self, hint.0);
                };
                let [h] = ctxs else {
                    return invalid(self, ctx_pos);
                };
                if !matches!(h.1, Hint_::Hwildcard) {
                    return invalid(self, h.0);
                }
                let new_ctx = Hint(h.0, self.a(Hint_::HfunContext(name)));
                hf.ctxs = Some(Contexts(ctx_pos, self.slice(&[new_ctx])));
                tparams.push(Tparam {
                    variance: Variance::Invariant,
                    name: Id(h.0, self.str(format_args!("T/[ctx {}]", name))),
                    constraints: &[],
                    reified: ReifyKind::Erased,
                    user_attributes: UserAttributes(&[]),
                });
                Hint(hint.0, self.a(Hint_::Hfun(hf)))
            }
            Hint_::Hlike(h) => {
                let h = self.rewrite_fun_ctx(tparams, h, name)?;
                Hint(hint.0, self.a(Hint_::Hlike(h)))
            }
            Hint_::Hoption(h) => {
                let h = self.rewrite_fun_ctx(tparams, h, name)?;
                Hint(hint.0, self.a(Hint_::Hoption(h)))
            }
            Hint_::Happly(id, targs) if id.1 == "supportdyn" => {
                let Some((first, rest)) = targs.split_first() else {
                    return invalid(self, hint.0);
                };
                let first = self.rewrite_fun_ctx(tparams, *first, name)?;
                let mut new_targs: SmallVec<[Hint<'a>; 2]> = SmallVec::new();
                new_targs.push(first);
                new_targs.extend_from_slice(rest);
                Hint(hint.0, self.a(Hint_::Happly(id, self.slice(&new_targs))))
            }
            _ => return invalid(self, hint.0),
        })
    }

    /// A closure cannot have a context that depends on something else:
    /// HHVM says so about each one that does.
    fn check_closure_contexts(&mut self, ctxs: Option<&Contexts<'a>>, what: &str) {
        let Some(Contexts(_, hints)) = ctxs else {
            return;
        };
        for hint in hints.iter() {
            if self.is_polymorphic_context(hint, false) {
                self.report(hint.0, format!("{what} cannot have a polymorphic context"));
            }
        }
    }

    /// The contexts a constant or an alias stands for, or is bounded by,
    /// cannot depend on anything: HHVM says `message` about each that does.
    fn check_fixed_contexts(&mut self, hints: &[Hint<'a>], message: &str) {
        for hint in hints {
            if self.is_polymorphic_context(hint, false) {
                self.report(hint.0, message.to_string());
            }
        }
    }

    /// The same for a bound, which is the intersection of its contexts.
    fn check_context_bound(&mut self, bound: &Hint<'a>) {
        if let Hint_::Hintersection(hints) = bound.1 {
            self.check_fixed_contexts(hints, "Contexts cannot be bounded by polymorphic contexts");
        }
    }

    /// `this::C` as a context only makes sense in a class.
    fn check_context_has_this(&mut self, ctxs: Option<&Contexts<'a>>) {
        let Some(Contexts(pos, hints)) = ctxs else {
            return;
        };
        for hint in hints.iter() {
            if let Hint_::Haccess(Hint(_, Hint_::Happly(Id(_, id), _)), _) = hint.1
                && id.strip_prefix('\\').unwrap_or(id) == "this"
            {
                self.report(
                    *pos,
                    "this:: context is not allowed on top level functions".to_string(),
                );
            }
        }
    }

    /// `<<__AsioLowPri>>` is for `async` functions that are no generators.
    fn check_low_priority(&mut self, fun: &n::Fun<'_>, attrs: &UserAttributes<'a>) {
        if let Some(attr) = attrs.0.iter().find(|attr| attr.name.1 == "__AsioLowPri")
            && Self::fun_kind(fun) != FunKind::FAsync
        {
            self.report(
                attr.name.0,
                "Only non-generator async functions can be marked low priority.".into(),
            );
        }
    }

    /// What HHVM has to say about memoizing a function (or method, as
    /// `kind` says) given its contexts.
    fn check_memoized_contexts(
        &mut self,
        ctxs: Option<&Contexts<'a>>,
        attrs: &UserAttributes<'a>,
        kind: &str,
    ) {
        let memoized =
            |attr: &&UserAttribute<'a>| matches!(attr.name.1, "__Memoize" | "__MemoizeLSB");
        if !attrs.0.iter().any(|attr| memoized(&attr)) {
            return;
        }
        // The names of the plain contexts, if every context is one.
        let named = |hint: &Hint<'a>| match hint.1 {
            Hint_::Happly(Id(_, id), _) => Some(*id),
            _ => None,
        };
        let hints = ctxs.map(|Contexts(_, hints)| *hints);
        let any = |names: &[&str]| {
            hints.is_some_and(|hints| {
                hints
                    .iter()
                    .any(|hint| named(hint).is_some_and(|id| names.contains(&id)))
            })
        };
        let keyed_by_ic = |attr: &&UserAttribute<'a>| {
            memoized(attr)
                && attr
                    .params
                    .iter()
                    .any(|p| matches!(p.2, Expr_::EnumClassLabel((_, "KeyedByIC"))))
        };
        // A function whose contexts depend on something else cannot be
        // memoized.
        if hints.is_some_and(|hints| {
            hints
                .iter()
                .any(|hint| self.is_polymorphic_context(hint, false))
        }) && let Some(attr) = attrs.0.iter().find(memoized)
        {
            self.report(
                attr.name.0,
                format!("This {kind} cannot be memoized because it has a polymorphic context"),
            );
        }
        // With `zoned`, memoization has to be keyed by the implicit
        // context.
        if any(&["zoned"])
            && let Some(attr) = attrs
                .0
                .iter()
                .find(|attr| memoized(attr) && attr.params.is_empty())
        {
            self.report(
                    attr.name.0,
                    format!(
                        "This {kind} can only be memoized using __Memoize(#KeyedByIC) or __MemoizeLSB(#KeyedByIC) because it has zoned context"
                    ),
                );
        }
        // And keying by it takes a context that has one.
        if let Some(attr) = attrs.0.iter().find(keyed_by_ic)
            && hints.is_some()
            && !any(&["zoned", "zoned_local", "zoned_shallow", "defaults"])
        {
            self.report(
                    attr.name.0,
                    format!(
                        "This {kind} requires a zoned context to be memoized using __Memoize(#KeyedByIC) or __MemoizeLSB(#KeyedByIC)"
                    ),
                );
        }
        // Contexts that cannot get at the implicit context leave nothing
        // to choose.
        let cannot_access_ic = hints.is_some_and(|hints| {
            hints.iter().all(|hint| {
                named(hint).is_some_and(|id| {
                    matches!(id, "leak_safe" | "globals" | "read_globals" | "write_props")
                })
            })
        });
        if cannot_access_ic
            && let Some(attr) = attrs.0.iter().find(memoized)
            && !attr.params.is_empty()
        {
            self.report(
                        attr.name.0,
                        format!(
                            "{kind}s that can neither directly nor indirectly call a function requiring [zoned] do not need to and cannot pass arguments to __Memoize or __MemoizeLSB"
                        ),
                    );
        }
    }

    /// The extent HHVM gives the argument list of a `new` expression with
    /// type arguments that ends at `end`: from the first argument to the
    /// last and any comma after it, or the place an empty list is at.
    fn new_arguments_extent(&self, new: &n::New<'s>, end: u32) -> (u32, u32) {
        use crate::cst::trivia;
        let bytes = self.src.as_bytes();
        let skip_trivia = |mut at: usize| {
            while let Some(piece) = trivia::scan(bytes, at) {
                at += piece.width;
            }
            at
        };
        let missing = |after: usize| {
            let at = (after + trivia::hhvm_trailing_width(bytes, after)) as u32;
            (at, at + 1)
        };
        // The `(` follows what closes the type arguments.
        let mut at = new.targs.last().map_or(new.class.span.end, |t| t.span.end) as usize;
        loop {
            at = skip_trivia(at);
            match bytes.get(at) {
                Some(b'>' | b',') => at += 1,
                Some(b'(') => break,
                // No argument list at all.
                _ => return missing(at),
            }
        }
        let open_end = at + 1;
        let close = (end as usize).saturating_sub(1);
        let first = skip_trivia(open_end);
        if first >= close {
            return missing(open_end);
        }
        // The last argument ends where the parentheses around it do.
        let mut last = new
            .args
            .iter()
            .chain(new.unpacked)
            .map(|arg| arg.span.end as usize)
            .max()
            .unwrap_or(first);
        let mut at = last;
        loop {
            at = skip_trivia(at);
            if at >= close || !matches!(bytes[at], b')' | b',') {
                break;
            }
            at += 1;
            last = at;
        }
        (first as u32, last as u32)
    }

    /// Records an error HHVM reports while translating to its AST and
    /// carries on from.
    fn report(&mut self, pos: Pos, message: String) {
        let start = pos.start_offset() as u32;
        self.errors.push((
            start + 1,
            SyntaxError {
                start,
                end: pos.end_offset() as u32,
                message: message.into(),
            },
        ));
    }

    fn rewrite_effect_polymorphism(
        &mut self,
        params: &mut SmallVec<[FunParam<'a>; 4]>,
        tparams: &mut SmallVec<[Tparam<'a>; 4]>,
        contexts: Option<&Contexts<'a>>,
        where_constraints: &mut SmallVec<[WhereConstraintHint<'a>; 2]>,
    ) -> R<()> {
        let Some(Contexts(_, context_hints)) = contexts else {
            return Ok(());
        };
        if !context_hints
            .iter()
            .any(|c| self.is_polymorphic_context(c, false))
        {
            return Ok(());
        }
        let tp = |name, constraints| Tparam {
            variance: Variance::Invariant,
            name,
            constraints,
            reified: ReifyKind::Erased,
            user_attributes: UserAttributes(&[]),
        };
        for context_hint in context_hints.iter() {
            match context_hint.1 {
                Hint_::HfunContext(name) => {
                    // Later parameters of the same name win.
                    let Some(i) = params.iter().rposition(|p| p.name == *name) else {
                        self.report(
                            context_hint.0,
                            format!("Could not find parameter {name} for dependent context"),
                        );
                        continue;
                    };
                    let Some(param_hint) = params[i].type_hint.1 else {
                        self.report(
                            params[i].pos,
                            format!(
                                "Parameter {name} used for dependent context must have a type hint"
                            ),
                        );
                        continue;
                    };
                    let new_hint = self.rewrite_fun_ctx(tparams, param_hint, name)?;
                    params[i].type_hint = TypeHint((), Some(new_hint));
                }
                Hint_::Haccess(root, csts) => {
                    if let Hint_::Hvar(name) = root.1 {
                        let Some(i) = params.iter().rposition(|p| p.name == *name) else {
                            self.report(
                                root.0,
                                format!("Could not find parameter {name} for dependent context"),
                            );
                            continue;
                        };
                        let param_pos = params[i].pos;
                        if matches!(params[i].info, FunParamInfo::ParamVariadic) {
                            self.report(
                                param_pos,
                                format!(
                                    "Parameter {name} used for dependent context cannot be variadic"
                                ),
                            );
                            continue;
                        }
                        let Some(param_hint) = params[i].type_hint.1 else {
                            self.report(
                                param_pos,
                                format!(
                                    "Parameter {name} used for dependent context must have a type hint"
                                ),
                            );
                            continue;
                        };
                        // The type that carries the context constant sits
                        // under any `~` / `?` wrappers.
                        #[derive(Clone, Copy)]
                        enum Wrap {
                            None,
                            Like,
                            Option,
                            LikeOption,
                        }
                        let (wrap, target) = match *param_hint.1 {
                            Hint_::Hlike(h) => match *h.1 {
                                Hint_::Hoption(inner) => (Wrap::LikeOption, inner),
                                _ => (Wrap::Like, h),
                            },
                            Hint_::Hoption(h) => (Wrap::Option, h),
                            _ => (Wrap::None, param_hint),
                        };
                        let Hint_::Happly(Id(_, type_name), _) = *target.1 else {
                            self.report(
                                target.0,
                                format!(
                                    "Type hint for parameter {name} used for dependent context must be a class or a generic"
                                ),
                            );
                            continue;
                        };
                        let mut new_target = target;
                        if !tparams.iter().any(|h| h.name.1 == type_name) {
                            let id = Id(param_pos, self.str(format_args!("T/{}", name)));
                            tparams.push(tp(
                                id,
                                self.slice(&[(ConstraintKind::ConstraintAs, target)]),
                            ));
                            new_target = Hint(param_pos, self.a(Hint_::Happly(id, &[])));
                            // Put the wrappers back around the new type.
                            let rebuilt = match wrap {
                                Wrap::None => new_target,
                                Wrap::Like => Hint(param_hint.0, self.a(Hint_::Hlike(new_target))),
                                Wrap::Option => {
                                    Hint(param_hint.0, self.a(Hint_::Hoption(new_target)))
                                }
                                Wrap::LikeOption => {
                                    let Hint_::Hlike(option) = *param_hint.1 else {
                                        unreachable!()
                                    };
                                    let option = Hint(option.0, self.a(Hint_::Hoption(new_target)));
                                    Hint(param_hint.0, self.a(Hint_::Hlike(option)))
                                }
                            };
                            params[i].type_hint = TypeHint((), Some(rebuilt));
                        }
                        let context_pos = context_hint.0;
                        let cst = csts[0];
                        let right = Hint(
                            context_pos,
                            self.a(Hint_::Haccess(new_target, self.slice(&[cst]))),
                        );
                        let left_id = Id(
                            context_pos,
                            self.str(format_args!("T/[{}::{}]", name, cst.1)),
                        );
                        tparams.push(tp(left_id, &[]));
                        let left = Hint(context_pos, self.a(Hint_::Happly(left_id, &[])));
                        where_constraints.push(WhereConstraintHint(
                            left,
                            ConstraintKind::ConstraintEq,
                            right,
                        ));
                    } else if let Hint_::Happly(Id(_, id), _) = root.1 {
                        let mut path = String::new();
                        for (i, c) in csts.iter().enumerate() {
                            if i > 0 {
                                path.push_str("::");
                            }
                            path.push_str(c.1);
                        }
                        match self.reification(id) {
                            None => {}
                            Some(false) => self.report(
                                root.0,
                                format!(
                                    "Type parameter {id} used for dependent context {id}::{path} must be reified"
                                ),
                            ),
                            Some(true) => {
                                let left_id = Id(
                                    context_hint.0,
                                    self.str(format_args!("T/[{}::{}]", id, path)),
                                );
                                tparams.push(tp(left_id, &[]));
                                let left =
                                    Hint(context_hint.0, self.a(Hint_::Happly(left_id, &[])));
                                where_constraints.push(WhereConstraintHint(
                                    left,
                                    ConstraintKind::ConstraintEq,
                                    *context_hint,
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Class members collected by kind while walking the member list.
#[derive(Default)]
struct Members<'a> {
    uses: Vec<Hint<'a>>,
    includes: Vec<Hint<'a>>,
    xhp_attr_uses: Vec<Hint<'a>>,
    xhp_category: Option<(Pos, &'a [Pstring<'a>])>,
    reqs: Vec<ClassReq<'a>>,
    consts: Vec<ClassConst<'a>>,
    typeconsts: Vec<ClassTypeconstDef<'a>>,
    vars: Vec<ClassVar<'a>>,
    methods: Vec<Method_<'a>>,
    xhp_children: Vec<(Pos, XhpChild<'a>)>,
    xhp_attrs: Vec<XhpAttr<'a>>,
}

fn visibility(v: n::Visibility) -> Visibility {
    match v {
        n::Visibility::Private => Visibility::Private,
        n::Visibility::Public => Visibility::Public,
        n::Visibility::Protected => Visibility::Protected,
        n::Visibility::Internal => Visibility::Internal,
        n::Visibility::ProtectedInternal => Visibility::ProtectedInternal,
    }
}

fn bop(op: n::Bop) -> Bop {
    match op {
        n::Bop::Plus => Bop::Plus,
        n::Bop::Minus => Bop::Minus,
        n::Bop::Star => Bop::Star,
        n::Bop::Slash => Bop::Slash,
        n::Bop::EqEq => Bop::Eqeq,
        n::Bop::EqEqEq => Bop::Eqeqeq,
        n::Bop::StarStar => Bop::Starstar,
        n::Bop::Diff => Bop::Diff,
        n::Bop::Diff2 => Bop::Diff2,
        n::Bop::AmpAmp => Bop::Ampamp,
        n::Bop::BarBar => Bop::Barbar,
        n::Bop::Lt => Bop::Lt,
        n::Bop::Lte => Bop::Lte,
        n::Bop::Gt => Bop::Gt,
        n::Bop::Gte => Bop::Gte,
        n::Bop::Dot => Bop::Dot,
        n::Bop::Amp => Bop::Amp,
        n::Bop::Bar => Bop::Bar,
        n::Bop::LtLt => Bop::Ltlt,
        n::Bop::GtGt => Bop::Gtgt,
        n::Bop::Percent => Bop::Percent,
        n::Bop::Xor => Bop::Xor,
        n::Bop::Cmp => Bop::Cmp,
        n::Bop::QuestionQuestion => Bop::QuestionQuestion,
    }
}

/// Extracts the error code from an `HH_FIXME[1234]` / `HH_IGNORE[1234]`
/// comment: the first occurrence of the marker followed by optional blanks
/// and a bracketed number.
fn comment_pos(start: (usize, usize, usize), end: (usize, usize, usize)) -> CommentPos {
    CommentPos {
        start: FilePos::new(start.0, start.1, start.2),
        end: FilePos::new(end.0, end.1, end.2),
    }
}

fn fixme_code(text: &[u8]) -> Option<isize> {
    let mut i = 0;
    while i + 3 <= text.len() {
        if !text[i..].starts_with(b"HH_") {
            i += 1;
            continue;
        }
        let rest = &text[i + 3..];
        for marker in [&b"FIXME"[..], b"IGNORE_ERROR", b"IGNORE"] {
            let Some(after) = rest.strip_prefix(marker) else {
                continue;
            };
            let mut j = 0;
            while j < after.len() && matches!(after[j], b' ' | b'\t' | b'\n') {
                j += 1;
            }
            if after.get(j) != Some(&b'[') {
                continue;
            }
            let digits_start = j + 1;
            let mut k = digits_start;
            while k < after.len() && after[k].is_ascii_digit() {
                k += 1;
            }
            if k == digits_start || after.get(k) != Some(&b']') {
                continue;
            }
            return std::str::from_utf8(&after[digits_start..k])
                .ok()?
                .parse()
                .ok();
        }
        i += 1;
    }
    None
}
