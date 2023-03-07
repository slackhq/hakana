function foo(vec<my_id_t> $a): void {
    if ($a === vec['a']) {}
    if ($a !== vec['a']) {}
    if ($a === vec[0]) {}
}