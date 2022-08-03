type alias = int;

function mapper(string $s): alias {
    return 5;
}

function foo(): keyset<int> {
    $v = keyset["key1", "key2"];
    return HH\Lib\Keyset\map($v, mapper<>);
}