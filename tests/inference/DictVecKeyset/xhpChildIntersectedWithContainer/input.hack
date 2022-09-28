function foo(XHPChild $c): dict<arraykey, mixed> {
    $a = $c as KeyedContainer<_, _>;
    $b = $a is dict<_, _> ? $a : exit();
    return $b;
}