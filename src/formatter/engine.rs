// Ported from HHVM hackfmt. Copyright (c) Meta Platforms, Inc. and affiliates.
// Licensed under the MIT license; see NOTICE and LICENSE in this directory.
use crate::{
    Config,
    doc::{Doc, Rule},
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap as Map, BTreeSet as Set},
};
pub type Range = (usize, usize);
// Rule IDs are dense within a group. Keep unbound rules distinct from rules
// bound to false: the search uses that distinction when pruning candidates.
type Bindings = Vec<Option<bool>>;
#[derive(Clone, Debug)]
pub struct Atom {
    pub text: String,
    pub range: Range,
    space: bool,
}
#[derive(Clone, Debug)]
struct Nesting {
    parent: Option<usize>,
    skip: bool,
}
#[derive(Clone, Debug)]
pub struct Chunk {
    spans: Vec<usize>,
    append: bool,
    space: bool,
    comma: Option<(usize, Option<Range>)>,
    rule: Option<usize>,
    nest: usize,
    start: usize,
    end: usize,
    indentable: bool,
    pub atoms: Vec<Atom>,
    len: usize,
}
impl Chunk {
    fn new(rule: Option<usize>, nest: usize, start: usize) -> Self {
        Self {
            spans: vec![],
            append: true,
            space: false,
            comma: None,
            rule,
            nest,
            start,
            end: start,
            indentable: true,
            atoms: vec![],
            len: 0,
        }
    }
    fn range(&self) -> Range {
        (
            self.atoms.first().unwrap().range.0,
            self.atoms.last().unwrap().range.1,
        )
    }
    fn empty(&self) -> bool {
        self.atoms.is_empty() || self.atoms.len() == 1 && self.atoms[0].text.is_empty()
    }
}
#[derive(Debug)]
pub struct Group {
    pub chunks: Vec<Chunk>,
    rules: Vec<Rule>,
    deps: Vec<Vec<usize>>,
    nests: Vec<Nesting>,
    nest_rules: Vec<Option<usize>>,
    chunk_rules: Vec<usize>,
    span_count: usize,
    indent: usize,
}
impl Group {
    fn range(&self) -> Range {
        self.chunks.iter().fold((usize::MAX, 0), |(a, b), c| {
            let (s, e) = c.range();
            (a.min(s), b.max(e))
        })
    }
    fn always(&self) -> Bindings {
        self.rules
            .iter()
            .map(|r| (*r == Rule::Always).then_some(true))
            .collect()
    }
    fn propagate(&self, mut rbm: Bindings) -> Bindings {
        for id in rbm
            .iter()
            .enumerate()
            .filter_map(|(id, v)| (*v == Some(true)).then_some(id))
            .collect::<Vec<_>>()
        {
            for parent in &self.deps[id] {
                if self.rules[*parent] == Rule::Parental {
                    rbm[*parent] = Some(true);
                }
            }
        }
        rbm
    }
    fn initial(&self) -> Bindings {
        self.propagate(self.always())
    }
    fn valid(&self, rbm: &Bindings) -> bool {
        rbm.iter().enumerate().all(|(id, b)| {
            *b != Some(true)
                || self.deps[id]
                    .iter()
                    .all(|p| self.rules[*p] != Rule::Parental || rbm[*p] != Some(false))
        })
    }
    fn source_breaks(&self, src: &str, mut rbm: Bindings) -> Bindings {
        let mut prev = 0;
        for c in &self.chunks {
            let (s, e) = c.range();
            if src
                .as_bytes()
                .get(prev..s)
                .is_some_and(|x| x.contains(&b'\n'))
            {
                rbm[c.rule.unwrap()] = Some(true);
            }
            prev = e;
        }
        rbm
    }
}
#[derive(Clone, Copy)]
enum Next {
    None,
    Kind(Rule),
}
#[derive(Clone, Copy, PartialEq)]
enum Last {
    Other,
    Doc,
    Number,
    Concat,
    Comment,
}
struct Builder<'a> {
    env: &'a Config,
    chunks: Vec<Chunk>,
    groups: Vec<Group>,
    rules: Vec<Rule>,
    deps: Map<usize, Vec<usize>>,
    active: Vec<usize>,
    lazy: Set<usize>,
    next_lazy: Set<usize>,
    nests: Vec<Nesting>,
    nest: usize,
    next: Next,
    pending_spans: usize,
    spans: Vec<usize>,
    next_span: usize,
    space: bool,
    pending_space: bool,
    comma: Option<(usize, Option<Range>)>,
    indent: usize,
    seen: usize,
    end: usize,
    last: Last,
    after_string: bool,
}
impl<'a> Builder<'a> {
    fn new(env: &'a Config) -> Self {
        Self {
            env,
            chunks: vec![],
            groups: vec![],
            rules: vec![],
            deps: Map::new(),
            active: vec![],
            lazy: Set::new(),
            next_lazy: Set::new(),
            nests: vec![Nesting {
                parent: None,
                skip: false,
            }],
            nest: 0,
            next: Next::Kind(Rule::Always),
            pending_spans: 0,
            spans: vec![],
            next_span: 0,
            space: false,
            pending_space: false,
            comma: None,
            indent: 0,
            seen: 0,
            end: 0,
            last: Last::Other,
            after_string: false,
        }
    }
    fn create(&mut self, r: Rule) -> usize {
        let id = self.rules.len();
        self.rules.push(r);
        id
    }
    fn start_id(&mut self, id: usize) {
        for p in self
            .lazy
            .iter()
            .copied()
            .chain(self.active.iter().rev().copied())
        {
            if self.rules[p] == Rule::Parental {
                self.deps.entry(id).or_default().insert(0, p);
            }
        }
        self.active.push(id);
    }
    fn start(&mut self, r: Rule) {
        if matches!(self.next,Next::Kind(k) if k!=Rule::Always) {
            self.next = Next::None;
        }
        let id = self.create(r);
        self.start_id(id);
    }
    fn start_lazy(&mut self, id: usize) {
        assert!(self.next_lazy.remove(&id) || self.lazy.remove(&id));
        self.start_id(id);
    }
    fn set_next(&mut self, next: Next) {
        if !matches!(self.next, Next::Kind(Rule::Always)) {
            self.next = next;
        }
    }
    fn boundary(&self) -> bool {
        self.active.is_empty()
            && self.lazy.is_empty()
            && self.next_lazy.is_empty()
            && self.nest == 0
    }
    fn nest(&mut self, skip: bool) {
        let id = self.nests.len();
        self.nests.push(Nesting {
            parent: Some(self.nest),
            skip,
        });
        self.nest = id;
    }
    fn unnest(&mut self) {
        self.nest = self.nests[self.nest].parent.expect("unbalanced nesting");
    }
    fn split(&mut self) {
        if self.chunks.last().is_some_and(|c| c.append) {
            let r = match self
                .chunks
                .last()
                .unwrap()
                .rule
                .or(self.active.last().copied())
            {
                Some(r) => r,
                None => self.create(Rule::Simple(1)),
            };
            let c = self.chunks.last_mut().unwrap();
            c.rule = Some(r);
            c.append = false;
            c.end = self.seen.max(c.start);
            c.space = self.space;
            c.comma = self.comma.take();
            c.len = c
                .atoms
                .iter()
                .map(|a| a.text.len() + usize::from(a.space))
                .sum();
            self.end = self.seen;
            self.space = self.pending_space;
        }
    }
    fn simple(&mut self, cost: i32) {
        self.split();
        self.set_next(Next::Kind(Rule::Simple(cost)));
    }
    fn hard(&mut self) {
        if !self.chunks.is_empty() {
            self.split();
            self.set_next(Next::Kind(Rule::Always));
        }
        if self.boundary() && !self.chunks.is_empty() {
            self.push_group();
        }
    }
    fn push_group(&mut self) {
        let deps = (0..self.rules.len())
            .map(|id| self.deps.remove(&id).unwrap_or_default())
            .collect();
        // Only the first chunk at each nesting level controls whether that
        // level contributes indentation. This is independent of search state.
        let mut nest_rules = vec![None; self.nests.len()];
        let chunk_rules = self
            .chunks
            .iter()
            .map(|c| {
                let rule = c.rule.unwrap();
                nest_rules[c.nest].get_or_insert(rule);
                rule
            })
            .collect();
        self.groups.push(Group {
            chunks: std::mem::take(&mut self.chunks),
            rules: std::mem::take(&mut self.rules),
            deps,
            nest_rules,
            chunk_rules,
            nests: std::mem::replace(
                &mut self.nests,
                vec![Nesting {
                    parent: None,
                    skip: false,
                }],
            ),
            indent: self.indent,
            span_count: self.next_span,
        });
        self.nest = 0;
        self.next_span = 0;
    }
    fn add_space(&mut self) {
        self.pending_space = true;
        if self.comma.is_some() && self.chunks.last().is_some_and(|c| c.append) {
            self.simple(1);
        }
    }
    fn string(&mut self, s: &str, width: usize, trivia: bool, multiline: bool) {
        if self.last == Last::Doc && (trivia || s != ";") {
            self.hard();
        }
        if self.chunks.last().is_some_and(|c| c.append) {
            let c = self.chunks.last_mut().unwrap();
            c.indentable &= !multiline;
            c.atoms.push(Atom {
                text: s.into(),
                range: (self.seen, self.seen + width),
                space: self.pending_space,
            });
            self.pending_space = false;
        } else {
            self.space = self.pending_space;
            self.pending_space = false;
            let started = match self.next {
                Next::None => false,
                Next::Kind(r) => {
                    self.start(r);
                    true
                }
            };
            let mut c = Chunk::new(self.active.last().copied(), self.nest, self.end);
            c.indentable = !multiline;
            c.atoms.push(Atom {
                text: s.into(),
                range: (self.seen, self.seen + width),
                space: false,
            });
            self.chunks.push(c);
            if started {
                self.active.pop();
                self.next = Next::None;
            }
        }
        self.seen += width;
        if !trivia {
            self.lazy.append(&mut self.next_lazy);
            for _ in 0..self.pending_spans {
                self.spans.push(self.chunks.len() - 1);
            }
            self.pending_spans = 0;
            if self.last == Last::Doc && s == ";" {
                self.seen += 1;
                self.hard();
                self.seen -= 1;
            }
        }
        self.last = Last::Other;
        if self.after_string {
            self.after_string = false;
            self.start(Rule::Parental);
        }
    }
    fn consume(&mut self, d: &Doc) {
        use Doc::*;
        match d {
            Nil => (),
            Concat(xs) => {
                for x in xs {
                    self.consume(x);
                }
            }
            Text(s, w) => self.string(s, *w, false, false),
            Comment(s, w) => self.string(s, *w, true, false),
            LineComment(s, w) => {
                self.string(s, *w, true, false);
                self.last = Last::Comment;
            }
            Ignore(w) => self.seen += w,
            Multiline(xs, w) => {
                let seen = self.seen;
                if let Some((first, rest)) = xs.split_first() {
                    self.string(first, first.len(), false, false);
                    for s in rest {
                        self.seen += 1;
                        self.hard();
                        self.string(s, s.len(), false, true);
                    }
                }
                self.seen = seen + w;
            }
            Literal(x) => {
                if !self.env.version(3) {
                    self.set_next(Next::Kind(Rule::Simple(1)));
                }
                self.consume(x);
                self.last = Last::Doc;
            }
            Number(x) => {
                if self.last == Last::Concat {
                    self.add_space();
                }
                self.consume(x);
                self.last = Last::Number;
            }
            ConcatOp(x) => {
                if self.last == Last::Number {
                    self.add_space();
                }
                self.consume(x);
                self.last = Last::Concat;
            }
            Split => self.split(),
            SplitWith(c) => self.simple(*c),
            Newline => self.hard(),
            BlankLine => {
                self.hard();
                if self.chunks.last().is_none_or(|c| !c.append) {
                    let r = self.create(Rule::Always);
                    let mut c = Chunk::new(Some(r), self.nest, self.end);
                    c.atoms.push(Atom {
                        text: String::new(),
                        range: (self.end, self.end),
                        space: false,
                    });
                    c.append = false;
                    c.end = self.seen.max(c.start);
                    self.end = self.seen;
                    self.chunks.push(c);
                }
            }
            Space => self.add_space(),
            Span(xs) => {
                self.pending_spans += 1;
                for x in xs {
                    self.consume(x);
                }
                if self.pending_spans > 0 {
                    self.pending_spans -= 1;
                } else {
                    let start = self.spans.pop().unwrap();
                    let span = self.next_span;
                    self.next_span += 1;
                    for c in self.chunks.iter_mut().skip(start + 1) {
                        c.spans.push(span);
                    }
                }
            }
            Nest(xs) | Conditional(xs) => {
                self.nest(matches!(d, Conditional(_)));
                for x in xs {
                    self.consume(x);
                }
                self.unnest();
            }
            Block(xs) => {
                if self.boundary() {
                    self.indent += 1;
                } else {
                    self.nest(false);
                }
                for x in xs {
                    self.consume(x);
                }
                if self.boundary() {
                    self.indent -= 1;
                } else {
                    self.unnest();
                }
            }
            With(r, x) => {
                self.start(*r);
                self.consume(x);
                self.active.pop();
            }
            Lazy(r, a, b) => {
                let id = self.create(*r);
                self.next_lazy.insert(id);
                self.consume(a);
                self.start_lazy(id);
                self.consume(b);
                self.active.pop();
            }
            Override(x) => {
                let over = matches!(self.next,Next::Kind(r) if r!=Rule::Always);
                if over {
                    self.start(Rule::Parental);
                }
                self.after_string = true;
                self.consume(x);
                self.active.pop();
                if over {
                    self.active.pop();
                }
            }
            Comma(present) => {
                if self.last != Last::Comment {
                    if self.last != Last::Doc {
                        let range = present.then_some((self.seen, self.seen + 1));
                        let comma = Some((*self.active.last().expect("comma without rule"), range));
                        if let Some(c) = self.chunks.last_mut().filter(|c| !c.append) {
                            c.comma = comma;
                        } else {
                            self.comma = comma;
                        }
                        if self.env.version(3) {
                            self.last = Last::Other;
                        }
                    }
                } else {
                    self.string(",", 1, false, false);
                    self.seen -= 1;
                }
                if !self.env.version(3) {
                    self.last = Last::Other;
                }
            }
        }
    }
}
pub fn build(env: &Config, doc: &Doc) -> Vec<Group> {
    let mut b = Builder::new(env);
    b.consume(doc);
    b.hard();
    assert!(b.chunks.is_empty(), "unbalanced document");
    b.groups
}
#[derive(Clone, Debug)]
struct State {
    rbm: Bindings,
    indents: Vec<usize>,
    cost: i32,
    overflow: usize,
    candidates: Vec<usize>,
    unprocessed: usize,
    partial: Vec<usize>,
}
fn broken(rbm: &Bindings, r: usize) -> bool {
    rbm[r] == Some(true)
}
impl State {
    fn indent(&self, c: &Chunk) -> usize {
        self.indents[c.nest]
    }
    fn make(env: &Config, g: &Group, mut rbm: Bindings) -> Self {
        let mut s = Self {
            rbm: vec![],
            indents: Vec::with_capacity(g.nests.len()),
            cost: 0,
            overflow: 0,
            candidates: vec![],
            unprocessed: 0,
            partial: vec![],
        };
        // Parents precede children, so compute each indentation depth once
        // instead of walking the same ancestor chain for every split chunk.
        for (id, nest) in g.nests.iter().enumerate() {
            let active = g.nest_rules[id].is_some_and(|r| broken(&rbm, r));
            let parent = if active && nest.skip {
                nest.parent.and_then(|p| g.nests[p].parent)
            } else {
                nest.parent
            };
            s.indents
                .push(parent.map_or(g.indent, |p| s.indents[p]) + usize::from(active && id != 0));
        }
        let mut lines = vec![];
        let mut len: usize = 0;
        let mut line_start = 0;
        let mut spans = vec![false; g.span_count];
        for (i, c) in g.chunks.iter().enumerate() {
            let r = c.rule.unwrap();
            let split = broken(&rbm, r);
            let length = c.len
                + usize::from(c.comma.is_some_and(|(id, _)| broken(&rbm, id)))
                + if split && c.indentable {
                    env.indent_width * s.indent(c)
                } else {
                    usize::from(c.space)
                };
            if split {
                // Each line borrows its rule IDs from the group. Repeated IDs
                // are harmless: the flags below deduplicate set membership.
                lines.push((
                    len.saturating_sub(env.line_width),
                    &g.chunk_rules[line_start..i],
                ));
                line_start = i;
                len = length;
                for id in &c.spans {
                    spans[*id] = true;
                }
            } else {
                len += length;
            }
        }
        lines.push((
            len.saturating_sub(env.line_width),
            &g.chunk_rules[line_start..],
        ));
        s.overflow = lines.iter().map(|(n, _)| n).sum();
        s.cost = spans.iter().filter(|b| **b).count() as i32
            + rbm
                .iter()
                .enumerate()
                .filter(|(_, v)| **v == Some(true))
                .map(|(id, _)| g.rules[id].cost())
                .sum::<i32>();
        if s.overflow == 0 {
            // Every line fits: all remaining line rules are dead, and there
            // are no candidates or partially processed lines to collect.
            for id in &g.chunk_rules {
                rbm[*id].get_or_insert(false);
            }
            s.rbm = rbm;
            return s;
        }
        let mut dead = vec![false; g.rules.len()];
        let mut candidates = vec![false; g.rules.len()];
        for &(overflow, rules) in &lines {
            if overflow == 0 || rules.iter().all(|r| rbm[*r].is_some()) {
                for r in rules {
                    if rbm[*r].is_none() {
                        dead[*r] = true;
                    }
                }
            } else {
                for r in rules {
                    candidates[*r] = true;
                }
                break;
            }
        }
        // Expand the original candidate set once, as in hackfmt. Iterating a
        // growing set here would also expand dependencies of dependencies.
        for r in set_ids(&candidates) {
            for parent in &g.deps[r] {
                candidates[*parent] = true;
            }
        }
        for (id, dead) in dead.into_iter().enumerate() {
            if dead && !candidates[id] && rbm[id].is_none() {
                rbm[id] = Some(false);
            }
        }
        let mut partial = vec![false; g.rules.len()];
        for &(overflow, rules) in &lines {
            if rules.iter().any(|r| rbm[*r].is_none()) {
                s.unprocessed += overflow;
                for r in rules {
                    if rbm[*r].is_some() {
                        partial[*r] = true;
                    }
                }
            }
        }
        s.candidates = set_ids(&candidates);
        s.partial = set_ids(&partial);
        s.rbm = rbm;
        s
    }
    fn compare(&self, b: &Self) -> Ordering {
        self.cost
            .cmp(&b.cost)
            .then(self.overflow.cmp(&b.overflow))
            .then_with(|| {
                self.rbm
                    .iter()
                    .map(|v| v.unwrap_or(false))
                    .cmp(b.rbm.iter().map(|v| v.unwrap_or(false)))
            })
    }
    fn better(&self, b: &Self) -> bool {
        self.overflow.cmp(&b.overflow).then_with(|| self.compare(b)) == Ordering::Less
    }
    fn overlaps(&self, b: &Self) -> bool {
        self.unprocessed == b.unprocessed
            && self.partial.len() == b.partial.len()
            && self.partial.iter().all(|r| self.rbm[*r] == b.rbm[*r])
    }
    fn expand(&self, env: &Config, g: &Group) -> Vec<Self> {
        let mut rbm = self.rbm.clone();
        let mut result = vec![];
        for id in &self.candidates {
            if self.rbm[*id].is_some() {
                continue;
            }
            let mut next = rbm.clone();
            next[*id] = Some(true);
            if g.valid(&next) {
                rbm[*id] = Some(false);
                result.push(Self::make(env, g, next));
            }
        }
        result
    }
}
// Preserve the ascending ID order used by hackfmt's ordered sets. Candidate
// order and binding comparison both affect which equally good layout wins.
fn set_ids(flags: &[bool]) -> Vec<usize> {
    flags
        .iter()
        .enumerate()
        .filter_map(|(id, set)| set.then_some(id))
        .collect()
}
#[derive(Default)]
struct Queue(Vec<State>);
impl Queue {
    fn up(&mut self, mut i: usize) {
        while i > 0 {
            let p = (i - 1) / 2;
            if self.0[i].compare(&self.0[p]) != Ordering::Less {
                break;
            }
            self.0.swap(i, p);
            i = p;
        }
    }
    fn down(&mut self, mut i: usize) {
        loop {
            let l = 2 * i + 1;
            let r = l + 1;
            if l >= self.0.len() {
                break;
            }
            let j = if r < self.0.len() && self.0[r].compare(&self.0[l]) == Ordering::Less {
                r
            } else {
                l
            };
            if self.0[i].compare(&self.0[j]) != Ordering::Greater {
                break;
            }
            self.0.swap(i, j);
            i = j;
        }
    }
    fn push(&mut self, s: State) {
        if let Some(i) = self.0.iter().position(|b| s.overlaps(b)) {
            if s.better(&self.0[i]) {
                self.0[i] = s;
            }
            self.down(i);
            self.up(i);
        } else {
            self.0.push(s);
            self.up(self.0.len() - 1);
        }
    }
    fn pop(&mut self) -> State {
        let s = self.0.swap_remove(0);
        if !self.0.is_empty() {
            self.down(0);
        }
        s
    }
}
fn contains(r: Range, x: usize) -> bool {
    r.0 <= x && x < r.1
}
pub fn overlaps(a: Range, b: Range) -> bool {
    a.0 == b.0 || a.0 < b.0 && b.0 < a.1 || b.0 < a.0 && a.0 < b.1
}
fn solve_group(env: &Config, g: &Group, range: Option<Range>, src: &str) -> State {
    let rbm = if let Some(range) =
        range.filter(|r| contains(g.range(), r.0) || contains(g.range(), r.1))
    {
        let source = g.source_breaks(src, vec![None; g.rules.len()]);
        let mut inside = Set::new();
        let mut outside = Set::new();
        for c in &g.chunks {
            if contains((range.0 + 1, range.1), c.start) {
                inside.insert(c.rule.unwrap());
            } else {
                outside.insert(c.rule.unwrap());
            }
        }
        let mut bindings: Bindings = source
            .into_iter()
            .enumerate()
            .map(|(id, b)| b.filter(|b| *b && outside.contains(&id)))
            .collect();
        for (id, b) in g.always().into_iter().enumerate() {
            if b.is_some() && inside.contains(&id) {
                bindings[id] = b;
            }
        }
        let propagated = g.propagate(bindings.clone());
        for (id, b) in propagated.into_iter().enumerate() {
            if b.is_some() && inside.contains(&id) && !outside.contains(&id) {
                bindings[id] = b;
            }
        }
        bindings
    } else {
        g.initial()
    };
    let mut best = State::make(env, g, rbm);
    if best.overflow == 0 {
        return best;
    }
    let mut q = Queue::default();
    for s in best.expand(env, g) {
        q.push(s);
    }
    for count in 0.. {
        if count > 100 {
            return State::make(env, g, g.propagate(g.source_breaks(src, g.always())));
        }
        if q.0.is_empty() || best.overflow == 0 {
            return best;
        }
        let next = q.pop();
        for s in next.expand(env, g) {
            q.push(s);
        }
        if next.better(&best) {
            best = next;
        }
    }
    unreachable!()
}
#[derive(Debug)]
enum Sub {
    Atom(String, Range),
    Comma,
    Space,
    Newline,
    Indent(usize),
}
pub fn solve(
    env: &Config,
    groups: &[Group],
    range: Option<Range>,
    src: &str,
    leading: bool,
    trailing: bool,
) -> String {
    let mut subs = vec![];
    for g in groups {
        if range.is_some_and(|r| !overlaps(r, g.range())) {
            continue;
        }
        let s = solve_group(env, g, range, src);
        let mut group_subs = vec![];
        for c in &g.chunks {
            if broken(&s.rbm, c.rule.unwrap()) {
                group_subs.push(Sub::Newline);
                if !c.empty() && c.indentable {
                    group_subs.push(Sub::Indent(s.indent(c)));
                }
            } else if c.space {
                group_subs.push(Sub::Space);
            }
            for a in &c.atoms {
                if a.space {
                    group_subs.push(Sub::Space);
                }
                group_subs.push(Sub::Atom(a.text.clone(), a.range));
            }
            if let Some((r, original)) = c.comma
                && broken(&s.rbm, r)
            {
                group_subs.push(match original {
                    Some(range) => Sub::Atom(",".into(), range),
                    None => Sub::Comma,
                });
            }
        }
        group_subs.push(Sub::Newline);
        subs.extend(
            group_subs
                .into_iter()
                .skip_while(|x| matches!(x, Sub::Newline)),
        );
    }
    let mut start = 0;
    let mut end = subs.len();
    if let Some((st, ed)) = range {
        end = subs
            .iter()
            .position(|x| matches!(x,Sub::Atom(_, (a,b)) if !(*a<ed||*a==ed&&*b==ed)))
            .unwrap_or(end);
        start = subs[..end]
            .iter()
            .rposition(|x| matches!(x,Sub::Atom(_, (a,b)) if !(*b>st||*a==st&&*b==st)))
            .map_or(0, |i| i + 1);
        while end > start && matches!(subs[end - 1], Sub::Space | Sub::Indent(_)) {
            end -= 1;
        }
        if !trailing {
            while end > start && matches!(subs[end - 1], Sub::Newline) {
                end -= 1;
            }
        }
        while start < end && matches!(subs[start], Sub::Newline | Sub::Space | Sub::Comma) {
            start += 1;
        }
        if !leading {
            while start < end && matches!(subs[start], Sub::Indent(_)) {
                start += 1;
            }
        }
    }
    let mut out = String::new();
    for s in &subs[start..end] {
        match s {
            Sub::Atom(s, _) => out.push_str(s),
            Sub::Comma => out.push(','),
            Sub::Space => out.push(' '),
            Sub::Newline => out.push('\n'),
            Sub::Indent(n) => out.push_str(&if env.tabs {
                "\t".repeat(*n)
            } else {
                " ".repeat(n * env.indent_width)
            }),
        }
    }
    out
}
