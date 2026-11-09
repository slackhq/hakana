function many_literals(int $n): string {
    $cases = vec[
        dict['input' => 'item0', 'expected' => 0],
        dict['input' => 'item1', 'expected' => 1],
        dict['input' => 'item2', 'expected' => 2],
        dict['input' => 'item3', 'expected' => 3],
        dict['input' => 'item4', 'expected' => 4],
        dict['input' => 'item5', 'expected' => 5],
        dict['input' => 'item6', 'expected' => 6],
        dict['input' => 'item7', 'expected' => 7],
        dict['input' => 'item8', 'expected' => 8],
        dict['input' => 'item9', 'expected' => 9],
        dict['input' => 'item10', 'expected' => 10],
        dict['input' => 'item11', 'expected' => 11],
        dict['input' => 'item12', 'expected' => 12],
        dict['input' => 'item13', 'expected' => 13],
        dict['input' => 'item14', 'expected' => 14],
        dict['input' => 'item15', 'expected' => 15],
        dict['input' => 'item16', 'expected' => 16],
    ];

    return $cases[$n]['input'] as string;
}

function few_literals(bool $b): string {
    $name = $b ? 'alpha' : 'bravo';

    return $name as string;
}

function dict_or_vec(bool $b, dict<arraykey, mixed> $d, vec<mixed> $v): int {
    $container = $b ? $d : $v;

    if ($container is KeyedContainer<_, _>) {
        return 1;
    }

    return 0;
}

function not_redundant(bool $b, string $s, int $i): string {
    $value = $b ? $s : $i;

    return $value as string;
}
