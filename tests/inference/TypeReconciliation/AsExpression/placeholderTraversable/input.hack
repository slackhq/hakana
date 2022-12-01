function foo(mixed $m) {
    $m as Traversable<_>;
    foreach ($m as $v) {
        $v as dict<_, _>;
    }
}