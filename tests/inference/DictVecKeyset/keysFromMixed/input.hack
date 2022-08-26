function foo(string $s): keyset<string> {
    $arr = json_decode($s);

    return HH\Lib\Keyset\keys($arr);
}