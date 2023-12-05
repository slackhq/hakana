function foo(vec<int> $v): void {
    $c = \HH\Lib\C\first($v);
    $d = vec[0, 1, 2];
    $e = \HH\Lib\C\last($d);
    echo $e;
}