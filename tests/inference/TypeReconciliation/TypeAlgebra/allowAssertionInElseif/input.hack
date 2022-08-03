class X {
    public bool $a = false;
    public bool $b = false;
    public bool $c = false;
}

function foo(X $x) : void {
    $a = false;
    if ($x->b && $x->a) {
    } else {
        if ($x->c) {
            $a = true;
        }
    }

    if ($x->c) {}
    if ($a) {}
}