function foo(int $s) : int {
    return $s;
}

function bar() : void {
    foreach (vec[1, 2, 3] as $i) {
        $i = foo($i);
    }
}