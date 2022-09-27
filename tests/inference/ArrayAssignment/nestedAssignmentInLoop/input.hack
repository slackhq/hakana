function foo(
    vec<string> $first,
    vec<string> $second,
): dict<string, dict<string, string>> {
    $nested_dict = dict[];
    foreach ($first as $key1) {
        foreach ($second as $key2) {
            $nested_dict[$key1] = isset($nested_dict[$key1]) ? $nested_dict[$key1] : dict[];
            $nested_dict[$key1][$key2] = 'foo';
        }
    }

    return $nested_dict;
}