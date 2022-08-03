function foo(vec<string> $bs): dict<int, vec<string>> {
    $map = dict[];
    foreach ($bs as $b) {
        $c = rand(0, 1000);

        if (HH\Lib\C\contains_key($map, $c)) {
            $map[$c] = Vec\concat($map[$c], vec[$b]);
        } else {
            $map[$c] = vec[$b];
        }
    }

    return $map;
}