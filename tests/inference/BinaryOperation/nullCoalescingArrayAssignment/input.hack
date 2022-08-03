function foo(dict<arraykey, string> $arr) : void {
    $b = dict[];

    foreach ($arr as $a) {
        $b[0] ??= $a;
    }
}