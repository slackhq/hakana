enum Foo: string {
   A = 'a';
   B = 'b';
   C = 'c';
}

function foo(?Foo $f): void {
    switch ($f) {
        case Foo::A:
            break;
        case Foo::B:
            break;
    }
}