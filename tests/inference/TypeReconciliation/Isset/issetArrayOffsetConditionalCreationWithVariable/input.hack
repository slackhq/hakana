function foo(dict<int, string> $arr) : string {
    $b = 5;

    if (!isset($arr[$b])) {
        $arr[$b] = "hello";
    }

    return $arr[$b];
}