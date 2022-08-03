function foo(dict<int, int> $arr) : int {
    $b = rand(0, 3);
    if (!isset($arr[$b])) {
        throw new \Exception("bad");
    }
    return $arr[$b];
}