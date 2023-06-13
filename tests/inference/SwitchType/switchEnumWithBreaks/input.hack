enum Foo: string {
   A = 'a';
   B = 'b';
   C = 'c';
}

function foo(Foo $f): void {
    $x = true;
    switch ($f) {
        case Foo::A:
            break;
        case Foo::B:
            $x = null;
            break;
        case Foo::C:
            $x = false;
            break;
    }
    
    if ($x is null) {
        echo 'abc';
    }
}