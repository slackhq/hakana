function foo(shape(?'a' => dict<string, mixed>) $dict, string $key): mixed {
    return $dict['a'][$key] ?? null;
}