function literal_first(bool $b, dict<int, string> $d): int {
    $x = $b ? dict['replica' => 0, 'primary' => 0] : $d;
    return $x;
}

function literal_second(bool $b, dict<int, string> $d): int {
    $x = $b ? $d : dict['replica' => 0, 'primary' => 0];
    return $x;
}
