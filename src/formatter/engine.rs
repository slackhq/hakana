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
    deps: Map<usize, Vec<usize>>,
    nests: Vec<Nesting>,
    indent: usize,
}
impl Group {
    fn range(&self) -> Range {
        self.chunks.iter().fold((usize::MAX, 0), |(a, b), c| {
            let (s, e) = c.range();
            (a.min(s), b.max(e))
        })
    }
    fn always(&self) -> Map<usize, bool> {
        self.rules
            .iter()
            .enumerate()
            .filter(|(_, r)| **r == Rule::Always)
            .map(|(i, _)| (i, true))
            .collect()
    }
    fn propagate(&self, mut rbm: Map<usize, bool>) -> Map<usize, bool> {
        for id in rbm
            .iter()
            .filter(|(_, v)| **v)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
        {
            for parent in self.deps.get(&id).into_iter().flatten() {
                if self.rules[*parent] == Rule::Parental {
                    rbm.insert(*parent, true);
                }
            }
        }
        rbm
    }
    fn initial(&self) -> Map<usize, bool> {
        self.propagate(self.always())
    }
    fn valid(&self, rbm: &Map<usize, bool>) -> bool {
        rbm.iter().all(|(id, b)| {
            !*b || self
                .deps
                .get(id)
                .into_iter()
                .flatten()
                .all(|p| self.rules[*p] != Rule::Parental || rbm.get(p) != Some(&false))
        })
    }
    fn source_breaks(&self, src: &str, mut rbm: Map<usize, bool>) -> Map<usize, bool> {
        let mut prev = 0;
        for c in &self.chunks {
            let (s, e) = c.range();
            if src
                .as_bytes()
                .get(prev..s)
                .is_some_and(|x| x.contains(&b'\n'))
            {
                rbm.insert(c.rule.unwrap(), true);
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
        self.groups.push(Group {
            chunks: std::mem::take(&mut self.chunks),
            rules: std::mem::take(&mut self.rules),
            deps: std::mem::take(&mut self.deps),
            nests: std::mem::replace(
                &mut self.nests,
                vec![Nesting {
                    parent: None,
                    skip: false,
                }],
            ),
            indent: self.indent,
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
    rbm: Map<usize, bool>,
    nesting: Set<usize>,
    cost: i32,
    overflow: usize,
    candidates: Set<usize>,
    unprocessed: usize,
    partial: Set<usize>,
}
fn broken(rbm: &Map<usize, bool>, r: usize) -> bool {
    rbm.get(&r) == Some(&true)
}
impl State {
    fn indent(&self, g: &Group, c: &Chunk) -> usize {
        let mut result = g.indent;
        let mut n = Some(c.nest);
        while let Some(id) = n {
            let active = self.nesting.contains(&id);
            if active && id != 0 {
                result += 1;
            }
            let nest = &g.nests[id];
            n = if active && nest.skip {
                nest.parent.and_then(|p| g.nests[p].parent)
            } else {
                nest.parent
            };
        }
        result
    }
    fn make(env: &Config, g: &Group, mut rbm: Map<usize, bool>) -> Self {
        let mut s = Self {
            rbm: Map::new(),
            nesting: Set::new(),
            cost: 0,
            overflow: 0,
            candidates: Set::new(),
            unprocessed: 0,
            partial: Set::new(),
        };
        let mut seen = Set::new();
        for c in &g.chunks {
            if seen.insert(c.nest) && broken(&rbm, c.rule.unwrap()) {
                s.nesting.insert(c.nest);
            }
        }
        let mut lines = vec![];
        let mut len: usize = 0;
        let mut rules = Set::new();
        for c in &g.chunks {
            let r = c.rule.unwrap();
            let split = broken(&rbm, r);
            let length = c.len
                + usize::from(c.comma.is_some_and(|(id, _)| broken(&rbm, id)))
                + if split && c.indentable {
                    env.indent_width * s.indent(g, c)
                } else {
                    usize::from(c.space)
                };
            if split {
                lines.push((
                    len.saturating_sub(env.line_width),
                    std::mem::take(&mut rules),
                ));
                len = length;
            } else {
                len += length;
            }
            rules.insert(r);
        }
        lines.push((len.saturating_sub(env.line_width), rules));
        s.overflow = lines.iter().map(|(n, _)| n).sum();
        let mut spans: Set<usize> = Set::new();
        for c in &g.chunks {
            if broken(&rbm, c.rule.unwrap()) {
                spans.extend(&c.spans);
            }
        }
        s.cost = spans.len() as i32
            + rbm
                .iter()
                .filter(|(_, v)| **v)
                .map(|(id, _)| g.rules[*id].cost())
                .sum::<i32>();
        let bound: Set<_> = rbm.keys().copied().collect();
        let mut dead = Set::new();
        for (overflow, rules) in &lines {
            let unbound: Set<_> = rules.difference(&bound).copied().collect();
            if *overflow == 0 || unbound.is_empty() {
                dead.extend(unbound);
            } else {
                s.candidates = rules.clone();
                break;
            }
        }
        for r in s.candidates.clone() {
            s.candidates.extend(g.deps.get(&r).into_iter().flatten());
        }
        for id in dead.difference(&s.candidates) {
            rbm.entry(*id).or_insert(false);
        }
        let bound: Set<_> = rbm.keys().copied().collect();
        for (overflow, rules) in &lines {
            if !rules.is_subset(&bound) {
                s.unprocessed += overflow;
                s.partial.extend(rules.intersection(&bound));
            }
        }
        s.rbm = rbm;
        s
    }
    fn compare(&self, b: &Self) -> Ordering {
        self.cost
            .cmp(&b.cost)
            .then(self.overflow.cmp(&b.overflow))
            .then_with(|| {
                let ids: Set<_> = self.rbm.keys().chain(b.rbm.keys()).copied().collect();
                for id in ids {
                    let c = broken(&self.rbm, id).cmp(&broken(&b.rbm, id));
                    if c != Ordering::Equal {
                        return c;
                    }
                }
                Ordering::Equal
            })
    }
    fn better(&self, b: &Self) -> bool {
        self.overflow.cmp(&b.overflow).then_with(|| self.compare(b)) == Ordering::Less
    }
    fn overlaps(&self, b: &Self) -> bool {
        self.unprocessed == b.unprocessed
            && self.partial.len() == b.partial.len()
            && self.partial.iter().all(|r| self.rbm.get(r) == b.rbm.get(r))
    }
    fn expand(&self, env: &Config, g: &Group) -> Vec<Self> {
        let mut rbm = self.rbm.clone();
        let mut result = vec![];
        for id in &self.candidates {
            if self.rbm.contains_key(id) {
                continue;
            }
            let mut next = rbm.clone();
            next.insert(*id, true);
            if g.valid(&next) {
                rbm.insert(*id, false);
                result.push(Self::make(env, g, next));
            }
        }
        result
    }
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
        let source = g.source_breaks(src, Map::new());
        let mut inside = Set::new();
        let mut outside = Set::new();
        for c in &g.chunks {
            if contains((range.0 + 1, range.1), c.start) {
                inside.insert(c.rule.unwrap());
            } else {
                outside.insert(c.rule.unwrap());
            }
        }
        let mut bindings: Map<_, _> = source
            .into_iter()
            .filter(|(id, b)| *b && outside.contains(id))
            .collect();
        for (id, _) in g.always() {
            if inside.contains(&id) {
                bindings.insert(id, true);
            }
        }
        let propagated = g.propagate(bindings.clone());
        for (id, b) in propagated {
            if inside.contains(&id) && !outside.contains(&id) {
                bindings.insert(id, b);
            }
        }
        bindings
    } else {
        g.initial()
    };
    let mut best = State::make(env, g, rbm);
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
                    group_subs.push(Sub::Indent(s.indent(g, c)));
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
