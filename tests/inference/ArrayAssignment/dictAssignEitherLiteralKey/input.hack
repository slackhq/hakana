function foo(): void {
    $arr = dict['a' => vec[], 'b' => vec[]];

    $key = rand(0, 1) ? 'a' : 'b';

    if (rand(0, 1)) {
        $arr[$key][] = 5;
    }

    if ($arr['a']) {}
}
