function foo(vec<int> $v): void {
    $base = keyset[];
    $res = HH\Lib\Keyset\union($base, $v);
    foreach ($res as $r) {
        echo $r;
    }
}