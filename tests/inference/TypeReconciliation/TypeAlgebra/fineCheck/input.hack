function foo(bool $b, bool $c) : void {
    if ((!$b || rand(0, 1) !== 0) && (!$c || rand(0, 1) !== 0)) {}
}