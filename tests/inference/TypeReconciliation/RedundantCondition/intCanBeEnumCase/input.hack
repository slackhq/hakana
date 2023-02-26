function foo(int $i) : void {
    if ($i === SomeInt::A) {}
    if (SomeInt::A === $i) {}
}

function faa(int $i) : void {
    if ($i === OtherInt::A) {}
    if (SomeInt::A === $i) {}
}

function bar(OtherInt $i) : void {
    if ($i === 0) {}
    if (1 === ($i as int)) {}
}

function baz(OtherInt $i) : void {
    if ($i === 0) {}
    if (1 === ($i as int)) {}
}

enum SomeInt: int as int {
    A = 0;
    B = 1;
}

enum OtherInt: int {
    A = 0;
    B = 1;
}
