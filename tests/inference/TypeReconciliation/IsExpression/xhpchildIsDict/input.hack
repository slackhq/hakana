function foo(XHPChild $x): dict<arraykey, mixed> {
    if ($x is dict<_, _>) {
        return $x;
    }

    return dict[];
}