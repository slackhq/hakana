function bar(string $key): void {
    $arr = dict['a' => vec[], 'b' => vec[]];

    if (rand(0, 1) !== 0) {
        $arr[$key][] = 5;
    }

    if (!C\is_empty($arr['a'])) {}
}
