function bar(string $key): void {
    $arr = dict['a' => vec[], 'b' => vec[]];

    if (rand(0, 1)) {
        $arr[$key][] = 5;
    }

    if ($arr['a']) {}
}